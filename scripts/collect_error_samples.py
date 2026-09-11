#!/usr/bin/env python3
"""自动采集真实世界编译/测试错误日志，构建贝叶斯分类器与覆盖率画像的训练语料。

来源双轨：
1. Stack Exchange API——按工具的标签帖 tag 查询；
2. GitHub 搜索（默认 Issues Search，带 --gh-token 可启用 Code Search）——issue 正文里的错误输出。
目标工具与类别见 TARGETS 表，采集项会落到 ``.tokenslim/audit/corpus/<tool>.jsonl``。

每个候选块做四件事：
- 提取 markdown 代码栅栏 / HTML <pre><code> 里的疑似命令输出；
- 用与 TokenSlim 线上管线相同的正则剥离 ANSI；
- 按来源查询标注类别（cargo→Cargo / gcc→Gcc / pytest→Test 等）；
- 行首启发式过滤（error[/warning:/--> 等锚点），过滤纯散文。

只依赖标准库，零第三方安装。运行：
    python scripts/collect_error_samples.py                          # 双源匿名
    python scripts/collect_error_samples.py --site so --pages 40
    python scripts/collect_error_samples.py --gh-token <PAT>         # GitHub 走 Code Search
    python scripts/collect_error_samples.py --se-key <key>           # 提限额 300→10000 req/day

配额与续采：
- Stack Exchange 匿名限频 ~300 req/day/ip，注册 App Key（--se-key）后可提到 10000 req/day；
  GitHub 匿名 Issues Search 限 10 req/min，带 PAT（--gh-token）升到 30 req/min 且可走 Code Search。
- 本次已抓过的来源会以 ``(来源, id)`` 记入已有 jsonl 记录，重跑时自动跳过（断点续采），
  不会重复请求已取正文的问题/issue，省配额。
"""

import argparse
import html
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, asdict

# ---------------------------------------------------------------------------
# ANSI 剥离正则：与 src/core/plugin_dispatcher/methods.rs 的 ansi_re/naked_csi_re 保持同构，
# 确保采集语料与线上管线看到的是同一份"去噪后"文本，避免判别词挖掘偏置。
# 裸 CSI 残留（`[Nm`）只在文本连带真实 ESC 字节时才剥离——纯净文本里的
# `path/to/[2m]odule.rs` 这类合法字面量绝不能误伤（否则砍坏路径）。
# ---------------------------------------------------------------------------
ANSI_RE = re.compile(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])")
NAKED_CSI_RE = re.compile(r"\[[0-9;]*m")


def strip_ansi(text: str) -> str:
    """剥离 ANSI 转义码；裸 CSI 残留仅当原文曾含真实 ESC（曾彩色化）时才一并剥离。"""
    cleaned = ANSI_RE.sub("", text)
    if "\x1b" in text:  # 原文带真实 ESC → 曾彩色化，才清残留裸 CSI，避免误伤普通路径
        cleaned = NAKED_CSI_RE.sub("", cleaned)
    return cleaned


# ---------------------------------------------------------------------------
# 目标工具表：分类器自然类别 (category) + 建议插件 (plugin) + 各平台检索参数。
# ---------------------------------------------------------------------------
@dataclass
class Target:
    category: str          # 分类器 Category.name()
    plugin: str            # 建议压缩插件名
    se_tags: list          # Stack Exchange 标签（按相关性叠加，勿用 cargo——那是 Java Maven 插件）
    se_query: str          # Stack Exchange 关键词
    gh_query: str          # GitHub 搜索关键词（命中工具错误锚点）


TARGETS = [
    # rust_go 同时覆盖 Rust(cargo/rustc) 与 Go，故拆两个目标但写同一 plugin 文件
    Target("cargo", "rust_go", ["rust"], "cargo build warning",
           'repo:rust-lang/rust "warning: unused"'),
    Target("cargo", "rust_go", ["go", "golang"], "goroutine panic",
           '"panic:" "goroutine" lang:Go'),
    Target("gcc", "gcc_log", ["gcc", "g++"], "gcc error In file included",
           '"In file included from" lang:C'),
    Target("test", "pytest", ["pytest"], "pytest failures",
           '"FAILED" pytest lang:Python'),
]

# ---------------------------------------------------------------------------
# 疑似裸命令输出的行首启发式：命中任一即保留，用于过滤嵌插件式的纯散文。
# ---------------------------------------------------------------------------
_OUTPUT_ANCHORS = (
    re.compile(r"[a-zA-Z]:[\\/][^ ]+\.rs:\d+:\d+"),      # rust 定位 src\f.rs:1:2
    re.compile(r"(?m)^\s*-->\s+\S+:\d+:\d+"),            # --> path:l:c
    re.compile(r"(?m)^\s*(error|warning|note|help)\b"),  # error:[ / warning:
    re.compile(r"error\[[A-Z0-9]+\]"),                   # error[E0308]
    re.compile(r"test result:\s*ok"),                    # test result: ok.
    re.compile(r"FAILED\]|PASSED\]"),                    # pytest 状态
    re.compile(r"goroutine\s+\d+\s+\["),                 # go panic 头
    re.compile(r"^\s*\d+\s+\|"),                         # 源码行 NN | code
)


def looks_like_output(block: str) -> bool:
    """判断一个代码块是否像裸命令输出而非普通代码片段/散文。"""
    return any(pat.search(block) for pat in _OUTPUT_ANCHORS)


# ---------------------------------------------------------------------------
# 提取工具：从 SE(HTML body) 与 GitHub(markdown body) 抽出代码块文本。
# ---------------------------------------------------------------------------
_MD_FENCE_RE = re.compile(r"```[a-zA-Z0-9_+-]*\n(.*?)```", re.S)
_HTML_PRE_RE = re.compile(r"<pre><code>(.*?)</code></pre>", re.S)


def extract_blocks(body: str) -> list:
    """从回复体提取代码块：优先 markdown 栅栏，其次 HTML <pre><code>。"""
    blocks = _MD_FENCE_RE.findall(body)
    if not blocks:
        blocks = [html.unescape(re.sub(r"<[^>]+>", "", m))
                  for m in _HTML_PRE_RE.findall(body)]
    return blocks


# ---------------------------------------------------------------------------
# 网络请求（stdlib，带 UA 与节流，可重试）
# ---------------------------------------------------------------------------
UA = ("Mozilla/5.0 (compatible; TokenSlim-collector/0.1; +local)"
      " python-urllib")


def http_get_json(url: str, headers: dict = None, retries: int = 3) -> dict:
    req = urllib.request.Request(url, headers={
        "User-Agent": UA, "Accept": "application/json",
        **(headers or {}),
    })
    for attempt in range(1, retries + 1):
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except Exception as exc:  # noqa: BLE001 —— 网络异常统一重试
            if attempt == retries:
                raise
            time.sleep(2 * attempt)
            _ = exc


# ---------------------------------------------------------------------------
# 极简 .env 解析（stdlib）：KEY=VALUE，忽略 # 注释与空行，支持 export 前缀与引号
# ---------------------------------------------------------------------------
def load_dotenv(path: str) -> dict:
    env = {}
    if not path or not os.path.isfile(path):
        return env
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("export "):
                line = line[len("export "):]
            if "=" not in line:
                continue
            k, _, v = line.partition("=")
            k = k.strip()
            v = v.strip()
            if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
                v = v[1:-1]
            if k:
                env[k] = v
    return env


def split_tokens(value: str | None) -> list:
    """把逗号分隔的多 token 切成 list（如 GitHub 多 PAT）。"""
    if not value:
        return []
    return [t.strip() for t in value.split(",") if t.strip()]


# ---------------------------------------------------------------------------
# 采集器主体
# ---------------------------------------------------------------------------
class Collector:
    def __init__(self, out_dir: str, gh_tokens: list, delay: float,
                 se_key: str | None = None):
        self.out_dir = out_dir
        self.gh_tokens = list(gh_tokens)  # GitHub PAT 池，403 自动轮换
        self._gh_idx = 0
        self.se_key = se_key  # Stack Exchange App Key（提配额）
        self.delay = delay
        os.makedirs(out_dir, exist_ok=True)
        self._wrote = 0
        self._uniques = set()
        # 断点续采：记录已正确处理过的 (来源, id)，重跑跳过，不重复请求省配额
        self._seen_ids = set()
        self._load_existing()

    # ---- GitHub 请求：多 PAT 轮换 + 403 限频时切到下一个 token ----
    def _gh_get(self, url: str) -> dict:
        n = len(self.gh_tokens)
        assert n > 0, "请提供 GitHub token（.env GITHUB_TOKEN 或 --gh-token）"
        attempts = 0
        while attempts <= n:
            tok = self.gh_tokens[self._gh_idx]
            headers = {"Authorization": f"Bearer {tok}"} if tok else {}
            try:
                return http_get_json(url, headers=headers)
            except urllib.error.HTTPError as exc:
                # 限频/权限问题才轮换；其余（4xx 参数错等）直接抛
                if exc.code not in (403, 429):
                    raise
                self._gh_idx = (self._gh_idx + 1) % n
                attempts += 1
                time.sleep(1.0)
        raise RuntimeError("GitHub 全 token 均被限频(403/429)，请稍后再试")

    def _load_existing(self):
        """从已有 jsonl 记录恢复去重集与已完成来源 id，实现断点续采。"""
        if not os.path.isdir(self.out_dir):
            return
        for fn in os.listdir(self.out_dir):
            if not fn.endswith(".jsonl"):
                continue
            path = os.path.join(self.out_dir, fn)
            for line in open(path, encoding="utf-8"):
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                text = rec.get("text", "")
                if text:
                    self._uniques.add(text)
                src = rec.get("source")
                sid = rec.get("site_id")
                if src and sid:
                    self._seen_ids.add((src, str(sid)))
        print(f"   [resume] 已有去重 {len(self._uniques)} 条，已处理来源 id {len(self._seen_ids)} 个")

    # ---- 写入（按工具分文件，逐条 flush，去重 + 断点标记） ----
    def _record(self, target: Target, src: str, sid: str | None,
                url: str, text: str):
        text = strip_ansi(text).strip()
        if not text or text in self._uniques:
            return
        if not looks_like_output(text):
            return
        self._uniques.add(text)
        rec = asdict(target)
        rec.update({
            "source": src, "url": url, "text": text,
            "site_id": sid,  # 断点续采键：重跑据此跳过
        })
        with open(os.path.join(self.out_dir, f"{target.plugin}.jsonl"),
                  "a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
        self._wrote += 1
        if self._wrote % 20 == 0:
            print(f"  ...已累计 {self._wrote} 条命中")

    # ---- 判断某来源是否已抓过（断点续采） ----
    def _already_done(self, src: str, sid: str) -> bool:
        if not sid:
            return False
        return (src, str(sid)) in self._seen_ids

    # ---- 凭据校验：填好配置后先跑 --check 确认有效再规模化 ----
    def check_credentials(self):
        print("== 凭据校验 ==")
        if not self.gh_tokens:
            print("  [gh] 无 GitHub token（.env GITHUB_TOKEN 或 --gh-token）")
        for i, tok in enumerate(self.gh_tokens, 1):
            try:
                d = http_get_json(
                    "https://api.github.com/rate_limit",
                    headers={"Authorization": f"Bearer {tok}"})
                rs = (d.get("resources", {}).get("search") or {})
                print(f"  [gh] token#{i}: 有效, search 剩余 {rs.get('remaining')}/{rs.get('limit')}")
            except urllib.error.HTTPError as exc:
                print(f"  [gh] token#{i}: 无效/受限 ({exc.code})")
            except Exception as exc:  # noqa: BLE001
                print(f"  [gh] token#{i}: 校验失败 {exc}")
        if self.se_key:
            print(f"  [se] App Key 已配 ({len(self.se_key)} 位)")
        else:
            print("  [se] 未配 App Key → 匿名采集(300/day)，今日极可能已 429")
        print("== 结束 ==")

    # ---- GitHub：匿名 Issues Search；带 token 时可选 Code Search ----
    def collect_github(self, target: Target, use_code_search: bool):
        if not self.gh_tokens and use_code_search:
            return  # Code Search 必须带 token，匿名只能走 Issues Search
        endpoint = ("https://api.github.com/search/code?q="
                    if use_code_search
                    else "https://api.github.com/search/issues?q=")
        # GitHub 分页上限 1000 条结果；per_page 限 100（匿名 issues 限 30 防限频）
        per_page = 100 if use_code_search else 30
        page = 1
        while True:
            url = (endpoint + urllib.parse.quote(target.gh_query)
                   + f"&per_page={per_page}&page={page}")
            try:
                data = self._gh_get(url)
            except Exception as exc:  # 整轮成功但单目标失败，不阻断其他目标
                print(f"   [gh] {target.plugin} page {page} 采集失败: {exc}")
                return
            items = data.get("items", [])
            if not items:
                break
            for item in items:
                sid = item.get("id")
                if sid and self._already_done("gh", str(sid)):
                    continue  # 断点续采：已抓过的 issue 跳过，省配额
                body = item.get("body") or ""
                for block in extract_blocks(body):
                    self._record(target, "gh", str(sid) if sid else None,
                                 item.get("html_url", ""), block)
                time.sleep(self.delay)
            if not data.get("items") or page >= 10:
                break  # 健康上限：每个目标最多 10 页
            page += 1

    # ---- Stack Exchange：两段式 —— 搜索得 id → 批量 filter=withbody 取正文 ----
    def collect_stackexchange(self, target: Target, tags: list, query: str,
                              pages: int):
        # 提配额：带 App Key 后 SE 限额升到 10000 req/day，且过滤权限更稳
        base_qs = {
            "site": "stackoverflow",
            "order": "desc",
            "sort": "votes",
            "tagged": ";".join(tags),
            "q": query,
            "pagesize": 100,
        }
        if self.se_key:
            base_qs["key"] = self.se_key

        # 第一段：搜索拿 question_id（配额友好，per_page=100 一次抓一大页）
        ids: list[int] = []
        for page in range(1, pages + 1):
            qs = dict(base_qs, page=page)
            url = ("https://api.stackexchange.com/2.3/search/advanced?"
                   + urllib.parse.urlencode(qs))
            try:
                data = http_get_json(url)
            except Exception as exc:  # noqa: BLE001
                print(f"   [se] {target.plugin} page {page} 失败: {exc}")
                break
            items = data.get("items", [])
            if not items:
                break
            ids.extend(it.get("question_id") for it in items
                       if it.get("question_id"))
            time.sleep(self.delay)
            if not data.get("has_more"):
                break

        # 第二段：批量路由（1 请求/100 id，逗号分隔会被 SE 判 no_method，须用分号）。
        # 批量响应若缺 body 再按需退化为单 id 重拉，保证正文可靠。
        for chunk_start in range(0, len(ids), 100):
            chunk = ids[chunk_start:chunk_start + 100]
            todo = [qid for qid in chunk
                    if not self._already_done("se", str(qid))]
            if not todo:
                continue  # 断点续采：这批 id 全已抓过，跳过整批省配额
            path = ";".join(str(qid) for qid in todo)
            url = ("https://api.stackexchange.com/2.3/questions/%s?"
                   "filter=withbody&site=stackoverflow&%s"
                   % (path, urllib.parse.urlencode(
                       {"key": self.se_key} if self.se_key else {})))
            try:
                full = http_get_json(url)
            except Exception as exc:  # noqa: BLE001
                print(f"   [se] {target.plugin} 批量拉正文失败: {exc}")
                continue
            by_id = {it.get("question_id"): it for it in full.get("items", [])}
            for qid in todo:
                item = by_id.get(qid)
                # 批量响应带 body 才够用，否则单 id 退化重拉保证拿到正文
                if item is None or "body" not in item:
                    try:
                        u = ("https://api.stackexchange.com/2.3/questions/%s?"
                             "filter=withbody&site=stackoverflow&%s"
                             % (qid, urllib.parse.urlencode(
                                 {"key": self.se_key} if self.se_key else {})))
                        one = http_get_json(u)
                    except Exception as exc:  # noqa: BLE001
                        print(f"   [se] {target.plugin} 拉正文 {qid} 失败: {exc}")
                        continue
                    item = next(iter(one.get("items") or []), {})
                body = item.get("body") or ""
                for block in extract_blocks(body):
                    self._record(target, "se", str(qid),
                                 item.get("link", ""), block)
                time.sleep(self.delay)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--site", choices=["all", "so", "gh"], default="all",
                    help="采样来源：so=Stack Exchange / gh=GitHub / all")
    ap.add_argument("--pages", type=int, default=10,
                    help="每个目标 Stack Exchange 拉取页数（默认 10）")
    ap.add_argument("--se-key", default=None,
                    help="Stack Exchange App Key（可选）；注册后限额 300→10000 req/day")
    ap.add_argument("--gh-token", "--token", dest="gh_token", default=None,
                    help="GitHub PAT（可选）；提供则启用 Code Search 并升限频")
    ap.add_argument("--out", default=".tokenslim/audit/corpus",
                    help="输出目录（默认 .tokenslim/audit/corpus）")
    ap.add_argument("--delay", type=float, default=1.0,
                    help="请求间隔秒数，规避限频（默认 1.0）")
    ap.add_argument("--check", action="store_true",
                    help="只校验 .env/CLI 里的凭据是否有效，不采集")
    args = ap.parse_args()

    # 读 .env：脚本位于 scripts/ 下，项目根 .env 在上一级
    dotenv = load_dotenv(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                      "..", ".env"))
    # 配额来源优先级：CLI > .env(RADAR_GITHUB_TOKEN > GITHUB_TOKEN) > 系统环境变量
    se_key = (args.se_key or dotenv.get("SE_KEY")
              or os.environ.get("SE_KEY"))
    gh_tokens = (split_tokens(args.gh_token)
                 or split_tokens(dotenv.get("RADAR_GITHUB_TOKEN"))
                 or split_tokens(dotenv.get("GITHUB_TOKEN"))
                 or split_tokens(os.environ.get("GITHUB_TOKEN")))
    use_code = bool(gh_tokens) and bool(args.gh_token or dotenv.get("GITHUB_TOKEN")
                                        or dotenv.get("RADAR_GITHUB_TOKEN"))

    col = Collector(args.out, gh_tokens, args.delay, se_key=se_key)
    if args.check:
        col.check_credentials()
        return
    print(f"采集器启动 | 输出目录: {args.out} | 来源: {args.site}"
          + (f" | SE key({len(se_key)}位)" if se_key else " | SE 匿名(300/day)")
          + (f" | GH token x{len(gh_tokens)}" if gh_tokens else " | GH 匿名(10/min)"))
    for t in TARGETS:
        print(f"[目标] {t.plugin} (category={t.category})")
        if args.site in ("all", "so"):
            col.collect_stackexchange(t, t.se_tags, t.se_query, args.pages)
        if args.site in ("all", "gh"):
            col.collect_github(t, use_code_search=use_code)
    print(f"完成：共写入 {col._wrote} 条去重命中到 {args.out}/")


if __name__ == "__main__":
    main()