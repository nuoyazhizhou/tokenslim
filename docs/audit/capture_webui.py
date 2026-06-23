"""TokenSlim WebUI 截图脚本 — 抓首页、压缩结果、英文界面、设置"""
from playwright.sync_api import sync_playwright
import os, time, urllib.request, json

OUT = r"C:\git_work\TokenSlim-publish2\docs\webui-screenshots"
os.makedirs(OUT, exist_ok=True)

# 准备示例日志（git log 输出）作为压缩输入
SAMPLE_LOG = """commit 9bc73ff7fd1a8b8e8c5e6a8b2b3c4d5e6f7a8b9c (HEAD -> main)
Author: nuoyazhizhou <nuoyazhizhou@example.com>
Date:   Tue Jun 23 18:08:00 2026 +0800

    feat(publish): integrate Web UI, SSE streaming and WebSocket tail

    Mirror the upstream master changes for the v0.3.2 release:
    - Add ServeDir fallback serving webui/index.html when TOKENSLIM_WEBUI_DIR
      is set (default: repo webui/)
    - Add POST /compress/stream (SSE progress) and GET /ws/tail
      (live log streaming over WebSocket)
    - Add GET /plugins endpoint listing all registered plugins
    - Add 3 new i18n keys: server_webui_enabled, server_webui_disabled,
      server_endpoint_plugins
    - Rename case_050_aliyun_csv_multiline -> case_052

diff --git a/Cargo.toml b/Cargo.toml
index 1234567..abcdef0 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -10,6 +10,7 @@
 axum = { version = "0.8", features = ["ws"] }
+axum = { version = "0.8.8", features = ["ws"] }
 tokio = { version = "1.36", features = ["full"] }
+notify = "7.0"
"""

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    ctx = browser.new_context(viewport={"width": 1440, "height": 900},
                              device_scale_factor=2)
    page = ctx.new_page()

    # 1. 简体中文首页
    page.goto("http://127.0.0.1:10086/")
    page.wait_for_load_state("networkidle")
    page.wait_for_timeout(500)
    page.screenshot(path=os.path.join(OUT, "01-home-zh.png"), full_page=True)
    print("OK 01-home-zh.png")

    # 2. 切换到英文 + 触发压缩
    page.evaluate("document.documentElement.lang = 'en'")
    # 通过 localStorage 切换语言
    page.evaluate("localStorage.setItem('tokenslim.lang', 'en')")
    page.reload()
    page.wait_for_load_state("networkidle")
    page.wait_for_timeout(500)

    # 填入示例日志
    page.locator("#input-text").fill(SAMPLE_LOG)
    page.wait_for_timeout(200)
    page.locator("#btn-compress").click()
    # 等压缩完成（出现 output-stats 文本）
    page.wait_for_selector("#output-stats:not(:empty)", timeout=10_000)
    page.wait_for_timeout(800)
    page.screenshot(path=os.path.join(OUT, "02-compress-en.png"), full_page=True)
    print("OK 02-compress-en.png")

    # 3. 切换到 diff 视图
    page.locator("#btn-toggle-view").click()
    page.wait_for_timeout(500)
    page.screenshot(path=os.path.join(OUT, "03-diff-view.png"), full_page=True)
    print("OK 03-diff-view.png")

    # 4. 触发 AI Export
    page.locator("#opt-ai-export").check()
    page.locator("#btn-compress").click()
    page.wait_for_timeout(1500)
    page.locator("#btn-toggle-view").click()  # 切回 json
    page.locator("#btn-toggle-view").click()  # 切到 ai
    page.wait_for_timeout(500)
    page.screenshot(path=os.path.join(OUT, "04-ai-export.png"), full_page=True)
    print("OK 04-ai-export.png")

    # 5. 移动端宽度（响应式）
    page.set_viewport_size({"width": 768, "height": 1024})
    page.wait_for_timeout(300)
    page.screenshot(path=os.path.join(OUT, "05-tablet.png"), full_page=True)
    print("OK 05-tablet.png")

    browser.close()

print("ALL DONE")
