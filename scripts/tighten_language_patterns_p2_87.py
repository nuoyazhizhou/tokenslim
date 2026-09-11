# -*- coding: utf-8 -*-
"""A-2 / P2-87：config/languages 宽松 pattern 外科收紧（配置面 only，不改 Rust 代码）。

决策表（依据 docs/reports 覆盖度对账 §8 + per-pattern 误判实证 2026-09-09）：
- 删（del）：行级判别力为零或与更强锚点冗余的单字符/标点/常用词 contains。
- 收紧（sw）：确有判别力但 contains 过宽 → starts_with（match_pattern 实现为 text.trim_start()，缩进行仍命中）。
- 改写（mod）：换成更长、更具体的签名。
"""
import io, json, os, sys

BASE = os.path.join(os.path.dirname(__file__), "..", "config", "languages")

# file -> list of ops
#   ("del", pattern)                 删除该 pattern
#   ("sw", pattern)                  contains -> starts_with
#   ("mod", old_pt, old, new_pt, new) 改写
OPS = {
    "yaml.json": [
        ("sw", "---"), ("sw", "..."), ("sw", "  "), ("sw", "- "),
        ("del", ": "),          # 24.1% 误判：日志 KV 行 "level: error" 与 YAML KV 行级同构，零判别力
        ("sw", "#"),
        ("del", "*"), ("del", "|"), ("del", "~"),
        ("sw", "<<"), ("sw", ">"),
        ("del", "null"), ("del", "true"), ("del", "false"),
    ],
    "json-config.json": [
        ("del", ":"), ("del", ","),          # 49.4% / 11.7%：URL 与一切标点行全中
        ("mod", "contains", '"', "contains", '": '),  # JSON KV 签名
        ("sw", "//"), ("sw", "/*"),
    ],
    "properties.json": [
        ("del", ":"), ("sw", "#"), ("del", "!"), ("del", "\\"),
        ("del", "%("), ("del", ")s"),        # printf 残渣
    ],
    "toml.json": [
        ("sw", "#"),
        ("del", "true"), ("del", "false"), ("del", "null"),
        ("del", "inf"), ("del", "nan"),      # 子串劫持 "info"/"nano"
        ("del", 'Z"'), ("del", "+00:00"),    # 日志 ISO 时间戳主体，非代码特征
    ],
    "ini.json": [
        ("sw", ";"), ("sw", "#"), ("del", "%("), ("del", ")s"),
    ],
    "xml-config.json": [
        ("sw", "<"), ("del", ">"),
    ],
    "shell.json": [
        ("sw", "echo "), ("sw", "then"), ("sw", "fi"), ("sw", "for "),
        ("sw", "done"), ("sw", "while "), ("sw", "case "), ("sw", "esac"),
        ("sw", "function "), ("sw", "export "), ("sw", "source "),
    ],
    "makefile.json": [
        ("sw", "all:"), ("sw", "clean:"), ("sw", "install:"), ("sw", "build:"), ("sw", "test:"),
        ("sw", "else"), ("sw", "endif"),
        ("sw", "include "), ("sw", "-include "), ("sw", "export "),
        ("sw", "override "), ("sw", "define "), ("sw", "endef"),
    ],
    "nginx.json": [
        ("sw", "root "), ("sw", "index "), ("sw", "rewrite "), ("sw", "return "), ("sw", "include "),
    ],
    "python.json": [
        ("sw", "from "), ("sw", "for "), ("sw", "with "),
    ],
    "go.json": [
        ("sw", "type "), ("del", "go "),
    ],
    "rust.json": [
        ("sw", "let "), ("sw", "match "),
    ],
    "scala.json": [
        ("sw", "with "),
    ],
    "javascript.json": [
        ("sw", "let "), ("sw", "type "),
    ],
    "csharp.json": [
        ("sw", "using "),
    ],
    "java.json": [
        ("del", "new "), ("sw", "finally"),
    ],
    "groovy.json": [
        ("sw", "error "), ("sw", "input "), ("sw", "echo "), ("del", "git "),
    ],
}


def dump(cfg):
    """按仓库既有格式序列化：2 空格缩进、pattern 对象单行内联、LF、结尾换行。"""
    out = ["{"]
    keys = list(cfg.keys())
    for i, k in enumerate(keys):
        v = cfg[k]
        comma = "," if i < len(keys) - 1 else ""
        if k == "file_extensions":
            out.append('  "%s": [%s]%s' % (k, ", ".join(json.dumps(x, ensure_ascii=False) for x in v), comma))
        elif k == "rules":
            out.append('  "rules": [')
            for ri, rule in enumerate(v):
                rcomma = "," if ri < len(v) - 1 else ""
                out.append("    {")
                rk = list(rule.keys())
                for j, rk2 in enumerate(rk):
                    if rk2 == "patterns":
                        out.append('      "patterns": [')
                        for pi, p in enumerate(rule["patterns"]):
                            pcomma = "," if pi < len(rule["patterns"]) - 1 else ""
                            pt = json.dumps(p["pattern_type"], ensure_ascii=False)
                            pp = json.dumps(p["pattern"], ensure_ascii=False)
                            line = '        {"pattern_type": %s, "pattern": %s' % (pt, pp)
                            if p.get("end_delimiter") is not None:
                                line += ', "end_delimiter": %s' % json.dumps(p["end_delimiter"], ensure_ascii=False)
                            line += "}"
                            out.append(line + pcomma)
                        out.append("      ]")
                    else:
                        out.append('      "%s": %s%s' % (rk2, json.dumps(rule[rk2], ensure_ascii=False), "," if j < len(rk) - 1 else ""))
                out.append("    }%s" % rcomma)
            out.append("  ]%s" % comma)
        else:
            out.append('  "%s": %s%s' % (k, json.dumps(v, ensure_ascii=False), comma))
    out.append("}")
    return "\n".join(out) + "\n"


def main():
    total_del = total_sw = total_mod = 0
    for fname, ops in sorted(OPS.items()):
        path = os.path.join(BASE, fname)
        raw = io.open(path, encoding="utf-8", newline="").read()
        # 格式化器忠实性校验：对「原文」做 dump 必须逐字节还原（容忍既有文件中 '" }' 引号后多余空格的纯空白归一），否则禁止写盘
        if dump(json.loads(raw)) != raw.replace('" }', '"}'):
            print("FATAL: %s 格式化器无法逐字节还原原文" % fname)
            sys.exit(1)
        cfg = json.loads(raw)
        nd = ns = nm = 0
        for rule in cfg.get("rules", []):
            pats = rule.get("patterns", [])
            new = []
            for p in pats:
                pt, pat = p.get("pattern_type"), p.get("pattern")
                dropped = False
                for op in ops:
                    if op[0] == "del" and pt == "contains" and pat == op[1]:
                        dropped = True
                        nd += 1
                        break
                    if op[0] == "sw" and pt == "contains" and pat == op[1]:
                        p = dict(p, pattern_type="starts_with")
                        ns += 1
                        break
                    if op[0] == "mod" and pt == op[1] and pat == op[2]:
                        p = dict(p, pattern_type=op[3], pattern=op[4])
                        nm += 1
                        break
                if not dropped:
                    new.append(p)
            rule["patterns"] = new
        text = dump(cfg)
        # 语义守卫：改写后仍可被 json.loads 解析（防写出残缺 JSON）
        json.loads(text)
        io.open(path, "w", encoding="utf-8", newline="\n").write(text)
        total_del += nd; total_sw += ns; total_mod += nm
        print("%-18s del=%-3d sw=%-3d mod=%-3d" % (fname, nd, ns, nm))
    print("\n合计: 删 %d / 收紧 starts_with %d / 改写 %d" % (total_del, total_sw, total_mod))


if __name__ == "__main__":
    main()
