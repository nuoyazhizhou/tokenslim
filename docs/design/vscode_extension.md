# VS Code 扩展 (VS Code Extension)

## 1. 模块职责

TokenSlim VS Code 扩展以 REST API 模式运行，通过 HTTP 与本地 `tokenslim-server` Sidecar 服务通信，提供一键压缩当前文件或选中文本的能力。

## 2. 技术栈

| 项目 | 值 |
|------|-----|
| 语言 | TypeScript |
| 最低 VS Code 版本 | 1.80.0 |
| 通信协议 | HTTP REST（127.0.0.1:10086） |
| 入口文件 | `src/extension.ts` |

## 3. 核心架构

```
VS Code Extension (TypeScript)
  │
  ├── extension.ts          # 激活入口，注册命令
  │   ├── activate()        # 注册 3 个命令 + 启动服务器检查
  │   ├── deactivate()      # 清理服务器进程
  │   ├── ensureServerRunning()  # 健康检查，按需启动
  │   ├── startServer()     # 启动 tokenslim-server 子进程
  │   ├── makeRequest()     # HTTP 请求封装
  │   └── compressAndShow() # 压缩并展示结果
  │
  └── package.json          # 扩展清单
```

## 4. 注册命令

| 命令 ID | 标题 | 功能 |
|---------|------|------|
| `tokenslim.compressCurrentFile` | TokenSlim: Compress Current File | 压缩当前活动编辑器的全部内容 |
| `tokenslim.compressSelection` | TokenSlim: Compress Selection | 压缩当前选中的文本 |
| `tokenslim.restartServer` | TokenSlim: Restart Server | 重启 Sidecar 服务 |

## 5. 核心函数

### activate(context)
扩展激活入口。注册 3 个命令到 `context.subscriptions`，并调用 `ensureServerRunning()` 检查服务器状态。

### ensureServerRunning(context)
向 `/health` 端点发送 GET 请求。如果服务器未运行，自动调用 `startServer()` 启动。

### startServer(context)
在 `target/release/tokenslim-server.exe` 路径查找二进制文件，以 detached 模式启动子进程。启动后等待 1 秒验证 `/health` 端点。

### makeRequest(method, path, body?)
封装 Node.js `http.request`，返回 Promise。支持 GET/POST，自动处理 JSON 序列化/反序列化。

### compressAndShow(text)
发送 POST `/compress` 请求，将返回的 JSON 结果在新编辑器标签页中展示（`ViewColumn.Beside`），并显示压缩率通知。

### deactivate()
扩展停用时终止 `tokenslim-server` 子进程。

## 6. 数据流

```
用户触发命令
  │
  ▼
compressCurrentFile / compressSelection
  │
  ▼
ensureServerRunning() → /health 检查
  │ (如未运行)
  ▼
startServer() → spawn tokenslim-server.exe
  │
  ▼
compressAndShow(text)
  │
  ▼
POST /compress {"text": "..."}
  │
  ▼
展示 JSON 结果 + 压缩率通知
```

## 7. 安装与调试

```bash
cd vscode-extension
npm install
# 按 F5 启动调试窗口
```

## 8. 依赖

- `@types/vscode`: ^1.80.0
- `@types/node`: ^20.0.0
- `typescript`: ^5.0.0
- 无运行时 npm 依赖（仅使用 Node.js 内置 `http`、`child_process`、`fs`、`path`）

---

*最后更新：2026-05-13（基于源码 `vscode-extension/src/extension.ts` 同步）*