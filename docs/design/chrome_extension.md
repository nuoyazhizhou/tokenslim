# Chrome 浏览器扩展 (Chrome Extension)

## 1. 模块职责

TokenSlim Chrome 扩展自动检测网页中的 TokenSlim 压缩 JSON，提供一键还原按钮。当用户在 ChatGPT、Claude Web 等 AI 界面中粘贴 TokenSlim 压缩输出时，扩展可将其还原为可读的原始日志。

## 2. 技术栈

| 项目 | 值 |
|------|-----|
| 语言 | TypeScript |
| 运行环境 | Chrome/Edge 浏览器（Manifest V3） |
| 核心文件 | `src/content.ts`、`src/rehydrator.ts` |

## 3. 核心架构

```
Chrome Extension (TypeScript)
  │
  ├── content.ts          # 内容脚本，DOM 注入与检测
  │   ├── init()          # MutationObserver 监听 DOM 变化
  │   ├── processNode()   # 扫描 <pre>/<code> 块
  │   └── injectRestoreButton()  # 注入还原按钮
  │
  └── rehydrator.ts       # 纯前端解压引擎
      └── TSRehydrator    # TokenSlim JSON → 原始文本
```

## 4. 核心函数

### content.ts

#### init()
创建 `MutationObserver` 监听 `document.body` 的 DOM 变化（`childList: true, subtree: true`）。页面加载时立即调用 `processNode(document.body)`。

#### processNode(root)
扫描所有 `<pre>` 和 `<code>` 元素：
1. 跳过已处理的元素（`data-tokenslim-processed` 标记）
2. 检查文本是否包含 `"tokens"` 和 `"dictionary"` 关键字
3. 尝试 JSON.parse，验证是否为 TokenSlim 压缩格式
4. 匹配成功则调用 `injectRestoreButton()`

#### injectRestoreButton(target, payload)
在目标元素旁注入还原按钮：
1. 标记元素为已处理
2. 创建 "TokenSlim: Restore Logs" 按钮
3. 点击时调用 `TSRehydrator.rehydrate(payload)` 还原文本
4. 还原后移除按钮，添加 "✓ Restored by TokenSlim" 标记

### rehydrator.ts

#### TSRehydrator.rehydrate(payload)
纯前端 TokenSlim 解压引擎，无需服务器：
1. 遍历 `payload.tokens` 数组
2. 字符串 token → 直接拼接
3. `DictRef` token → 递归查字典解析（`$P`/`$D`/`$M`/`$F`/`$PK`/`$FL`）
4. `Repeat` token → 重复拼接
5. 最后调用 `restoreMarkers()` 还原语义标记

#### resolveRecursive(token, payload, depth)
递归解析字典引用，支持嵌套别名（如 `$P7=$P3/subdir`）。最大递归深度 10 层，防止无限循环。

#### restoreMarkers(text)
还原压缩标记：
- `$PL` → `[Pipeline]`
- `$GCC` → `gcc:`
- `$XC|PROBE|xN` → `[Xcode Probe xN]`
- `$XC|AGG|TYPE|xN` → `[TYPE xN]`
- Android 资源警告聚合还原

## 5. 数据流

```
页面加载 / DOM 变化
  │
  ▼
MutationObserver → processNode()
  │
  ▼
扫描 <pre>/<code> → 检测 TokenSlim JSON
  │
  ▼
injectRestoreButton() → 注入按钮
  │
  ▼
用户点击按钮
  │
  ▼
TSRehydrator.rehydrate(payload)
  │
  ├── 遍历 tokens → 拼接文本
  ├── 递归解析字典引用
  └── restoreMarkers() 还原标记
  │
  ▼
替换原始内容 + 显示还原标记
```

## 6. 安装

```bash
# Chrome
打开 chrome://extensions/ → 开启"开发者模式"
→ "加载已解压的扩展程序" → 选择 chrome-extension/dist 目录
```

---

*最后更新：2026-05-13（基于源码 `chrome-extension/src/` 同步）*