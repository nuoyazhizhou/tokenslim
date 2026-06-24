"""重排前后对比截图脚本

生成一张 1440×900 的截图，左侧 BEFORE（交错日志），右侧 AFTER（重排后）
高亮关键变化：按目标分组、路径截断、地址脱敏
"""
from playwright.sync_api import sync_playwright
from pathlib import Path
import html

OUT = Path(r"C:\git_work\TokenSlim\docs\webui-screenshots\reorder-before-after.png")
OUT.parent.mkdir(parents=True, exist_ok=True)

BEFORE = Path(r"C:\git_work\TokenSlim\benchmarks\messy_jbuild.log")
AFTER  = Path(r"C:\git_work\TokenSlim\benchmarks\sorted_jbuild.log")

def fmt(path: Path, head: int = 35) -> str:
    """读取文件前 N 行并 HTML 转义。"""
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()[:head]
    return "\n".join(html.escape(l) for l in lines)

# 预渲染颜色：根据行内容染色
import re
ERR_PAT = re.compile(r"error:|Error |expected primary-expression|\\*\\*\\*")
WARN_PAT = re.compile(r"warning:|unused variable")

def colorize(text: str) -> str:
    """对 error/warning 行加颜色。"""
    out = []
    for line in text.split("\n"):
        if ERR_PAT.search(line):
            out.append(f'<span class="err">{line}</span>')
        elif WARN_PAT.search(line):
            out.append(f'<span class="warn">{line}</span>')
        else:
            out.append(line)
    return "\n".join(out)

# 计算 before 行数
n_before = len(BEFORE.read_text(encoding="utf-8").splitlines())
n_after  = len(AFTER.read_text(encoding="utf-8").splitlines())
b_size = BEFORE.stat().st_size
a_size = AFTER.stat().st_size

HTML = f"""<!doctype html>
<html><head><meta charset="utf-8"><style>
  :root {{
    --bg: #0d1117; --fg: #c9d1d9; --muted: #8b949e; --line: #30363d;
    --err: #f85149; --warn: #d29922; --accent: #58a6ff;
  }}
  html, body {{ margin: 0; padding: 0; background: var(--bg); color: var(--fg);
                font-family: 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Consolas, monospace;
                font-size: 12px; line-height: 1.45; }}
  .header {{ padding: 18px 24px 12px; border-bottom: 1px solid var(--line); background: #161b22; }}
  .header h1 {{ margin: 0; font-size: 18px; font-weight: 600; color: #f0f6fc; }}
  .header .sub {{ color: var(--muted); margin-top: 6px; font-size: 12px; }}
  .header .stats {{ color: var(--accent); margin-top: 6px; font-size: 12px; }}
  .header .stats b {{ color: #f0f6fc; }}
  .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 0; }}
  .pane {{ padding: 14px 18px; border-right: 1px solid var(--line); overflow: hidden; }}
  .pane:last-child {{ border-right: none; background: #0a0e14; }}
  .pane h2 {{ margin: 0 0 8px 0; font-size: 13px; color: #f0f6fc; font-weight: 600;
              display: flex; justify-content: space-between; }}
  .pane h2 .tag {{ font-size: 11px; padding: 1px 8px; border-radius: 3px;
                    background: #21262d; color: var(--muted); font-weight: 400; }}
  .pane h2 .tag.bad {{ background: #4c1f1f; color: #ff7b72; }}
  .pane h2 .tag.good {{ background: #1f4c2e; color: #56d364; }}
  pre {{ margin: 0; white-space: pre; overflow: hidden; color: #c9d1d9; }}
  .err {{ color: var(--err); }}
  .warn {{ color: var(--warn); }}
  .footer {{ padding: 10px 24px; border-top: 1px solid var(--line); background: #161b22;
             color: var(--muted); font-size: 11px; display: flex; justify-content: space-between; }}
  .footer code {{ background: #0a0e14; padding: 1px 6px; border-radius: 3px; color: #f0f6fc; }}
</style></head>
<body>
  <div class="header">
    <h1>TokenSlim — Log Reordering: BEFORE vs AFTER</h1>
    <div class="sub">Parallel build (<code>make -j4</code>) log diff demonstration</div>
    <div class="stats">
      <b>BEFORE</b>: {n_before} lines · {b_size:,} bytes · 4-way interleaved targets<br>
      <b>AFTER </b>: {n_after} lines · {a_size:,} bytes · grouped by build target, paths shortened, addresses masked
    </div>
  </div>
  <div class="grid">
    <div class="pane">
      <h2><span>BEFORE — raw interleaved log</span><span class="tag bad">NON-DETERMINISTIC</span></h2>
      <pre>{colorize(fmt(BEFORE))}</pre>
    </div>
    <div class="pane">
      <h2><span>AFTER — <code>log_reorder --deterministic -n -p</code></span><span class="tag good">GROUPED + NORMALIZED</span></h2>
      <pre>{colorize(fmt(AFTER))}</pre>
    </div>
  </div>
  <div class="footer">
    <span><code>log_reorder -i in.log -o out.log --deterministic -n -p</code></span>
    <span>Two identical builds → byte-identical output · ready for Beyond Compare</span>
  </div>
</body></html>
"""

PAGE = Path(r"C:\git_work\TokenSlim\docs\webui-screenshots\reorder-page.html")
PAGE.write_text(HTML, encoding="utf-8")

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    ctx = browser.new_context(viewport={"width": 1440, "height": 900}, device_scale_factor=2)
    page = ctx.new_page()
    page.goto(f"file:///{PAGE.as_posix()}")
    page.wait_for_load_state("networkidle")
    page.screenshot(path=str(OUT), full_page=True)
    browser.close()

print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
