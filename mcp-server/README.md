# @tokenslim/mcp-server

> Give every MCP-aware AI Agent (Claude Code, Cursor, Windsurf, Qoder, OpenCode, etc.) direct access to TokenSlim's compression and decompression capabilities — through the standard Model Context Protocol.

**中文快速指南见 [末尾章节](#中文快速指南)。**

---

## Table of Contents

- [What is TokenSlim MCP Server?](#what-is-tokenslim-mcp-server)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Available Tools](#available-tools)
- [Available Resources](#available-resources)
- [Agent Configuration Examples](#agent-configuration-examples)
  - [Claude Code / Claude Desktop](#claude-code--claude-desktop)
  - [Cursor](#cursor)
  - [Windsurf](#windsurf)
  - [Qoder](#qoder)
  - [OpenCode](#opencode)
  - [Generic stdio client](#generic-stdio-client)
- [Development](#development)
- [Project Structure](#project-structure)
- [FAQ](#faq)
- [中文快速指南](#中文快速指南)

---

## What is TokenSlim MCP Server?

TokenSlim MCP Server is a lightweight [Model Context Protocol](https://modelcontextprotocol.io) server that wraps the TokenSlim CLI engine. It lets any MCP-compatible AI agent:

- **Compress** arbitrary text or local files before feeding them into an LLM context window.
- **Decompress** TokenSlim output back to the original text.
- **Smart-compress** with a threshold — skip compression when the gain is too small to justify the overhead.
- **Query session stats** to track cumulative compression savings.
- **Browse configuration and plugins** as MCP resources.

This is particularly useful when your agent reads large build logs, CI output, cloud traces, or git diffs and you want to fit more signal into less context.

---

## Prerequisites

| Requirement | Version |
|---|---|
| Node.js | ≥ 18 |
| TokenSlim CLI | Any recent version; auto-detected on startup |

The server will locate `tokenslim` / `tokenslim.exe` on your `PATH` (or via the `TOKENSLIM_CLI_PATH` environment variable). If it cannot find the binary, the server exits with a diagnostic message.

---

## Installation

### From npm (recommended once published)

```bash
# Global install — puts tokenslim-mcp-server on PATH
npm install -g @tokenslim/mcp-server
```

### From source (development)

```bash
cd mcp-server
npm install
npm run build
```

### Run directly via npx (no install needed)

```bash
npx -y @tokenslim/mcp-server
```

---

## Available Tools

### `compress`

Compress any text. Returns a TokenSlim `CompressionOutput` JSON that can later be restored with `decompress`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `text` | `string` | ✅ | The raw text to compress |
| `mode` | `"fast"` \| `"balanced"` \| `"max"` | — | Compression preset; `"max"` maps to the CLI `ai` preset for maximum token savings |
| `plugins` | `string[]` | — | Hint for plugin selection (currently the CLI auto-routes) |

**Returns:** `{ compressed, stats: { original_size, compressed_size, compression_ratio } }`

---

### `decompress`

Restore the original text from a TokenSlim compressed JSON.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `text` | `string` | ✅ | TokenSlim `CompressionOutput` JSON string |

**Returns:** the original plain text.

---

### `compress_file`

Compress a local file by path. Useful when an agent wants to compress a log file on disk without reading it first.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `path` | `string` | ✅ | Absolute or CWD-relative path to the file |
| `mode` | `"fast"` \| `"balanced"` \| `"max"` | — | Compression preset |

**Returns:** same shape as `compress`, with an additional `source_file` field in `stats`.

---

### `smart_compress`

Intelligently decide whether to compress: if the achieved compression ratio is below `threshold`, return the original text unchanged to avoid unnecessary overhead.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `text` | `string` | ✅ | The raw text to evaluate |
| `threshold` | `number` (0–1) | — | Minimum ratio to trigger compression (default `0.3`, i.e. ≥ 30% savings) |

**Returns:** `{ decision: "compressed" | "skipped", achieved_ratio, compressed, reason }`

---

### `stats`

Return cumulative compression statistics for the current MCP session (no parameters).

**Returns:** `{ total_runs, total_original_bytes, total_compressed_bytes, total_saved_bytes, average_ratio }`

---

## Available Resources

| URI | Name | MIME Type | Description |
|---|---|---|---|
| `tokenslim://config` | TokenSlim Config | `application/json` | CLI version, environment info, active project configuration |
| `tokenslim://plugins` | TokenSlim Plugins | `application/json` | List of all available plugins with names and descriptions |

---

## Agent Configuration Examples

All configurations use the **stdio transport** — the agent spawns the server as a subprocess and exchanges JSON-RPC messages over stdin/stdout.

> **Tip:** Replace `/path/to/mcp-server/dist/index.js` with the real absolute path on your machine. On Windows use forward slashes or escaped backslashes: `C:/path/to/...` or `C:\\path\\to\\...`.

### Claude Code / Claude Desktop

**File:** `~/.claude/claude_desktop_config.json` (macOS/Linux) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows).
Some setups use a per-project `.mcp.json` in the repo root.

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

After publishing to npm, you can use `npx` instead (no local build needed):

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "npx",
      "args": ["-y", "@tokenslim/mcp-server"]
    }
  }
}
```

---

### Cursor

**File:** `.cursor/mcp.json` in the project root (per-project) or `~/.cursor/mcp.json` (global).

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

With npx:

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "npx",
      "args": ["-y", "@tokenslim/mcp-server"]
    }
  }
}
```

---

### Windsurf

**File:** `~/.codeium/windsurf/mcp_config.json`

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

---

### Qoder

**File (per-project):** `.qoder/mcp_servers.json` in the project root.
**File (global):** `~/.qoder/mcp_servers.json`

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

---

### OpenCode

**File:** `.opencode/mcp.json` in the project root.

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

---

### Generic stdio client

Any MCP client that supports stdio transport can use the same shape:

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

Or via npx (no local install, always latest version):

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "npx",
      "args": ["-y", "@tokenslim/mcp-server"]
    }
  }
}
```

---

## Environment Variables

| Variable | Description |
|---|---|
| `TOKENSLIM_CLI_PATH` | Override auto-detection and point directly to the `tokenslim` binary |
| `RUST_LOG` | Passed through to the CLI (`info`, `debug`, `warn`, `error`) |

---

## Development

```bash
# Install deps
npm install

# Type-check and build
npm run build

# Run in dev mode (tsx, no build step)
npm run dev

# Run the built output
npm start
```

Diagnostic messages are written to **stderr** so they don't pollute the MCP stdio stream.

---

## Project Structure

```
mcp-server/
├── package.json            # @tokenslim/mcp-server
├── tsconfig.json
├── mcp_servers.example.json  # Ready-to-copy agent config template
├── README.md
├── src/
│   ├── index.ts            # Entry: register tools/resources, start stdio server
│   ├── tools/
│   │   ├── compress.ts         # compress tool
│   │   ├── compress-file.ts    # compress_file tool
│   │   ├── decompress.ts       # decompress tool
│   │   ├── smart-compress.ts   # smart_compress tool
│   │   └── stats.ts            # stats tool
│   ├── resources/
│   │   ├── config.ts           # tokenslim://config resource
│   │   └── plugins.ts          # tokenslim://plugins resource
│   └── utils/
│       ├── cli.ts              # CLI binary detection & subprocess wrapper
│       ├── session.ts          # In-memory session statistics tracker
│       └── types.ts            # Shared types
└── dist/                   # Compiled output (npm run build)
```

---

## FAQ

**Q: The server exits immediately with "CLI not found". What do I do?**

Make sure `tokenslim` is on your `PATH` (run `tokenslim --version` in a terminal to verify). If it's installed in a non-standard location, set `TOKENSLIM_CLI_PATH` before starting the server:

```bash
TOKENSLIM_CLI_PATH=/opt/tokenslim/bin/tokenslim node dist/index.js
```

Or add the env var to your agent config:

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"],
      "env": {
        "TOKENSLIM_CLI_PATH": "/opt/tokenslim/bin/tokenslim"
      }
    }
  }
}
```

**Q: Does the server call a remote API?**

No. All compression runs locally via the TokenSlim CLI subprocess. No data leaves your machine.

**Q: Can I use this without installing TokenSlim globally?**

Yes. Install `tokenslim` as a project-local npm dependency (`npm install tokenslim`) — the server will find the binary inside `node_modules/.bin/`.

---

## 中文快速指南

### 简介

TokenSlim MCP Server 是一个轻量级 [Model Context Protocol](https://modelcontextprotocol.io) 服务器，封装了 TokenSlim CLI 引擎。它让任何支持 MCP 的 AI Agent（Claude Code、Cursor、Windsurf、Qoder 等）能够直接调用 TokenSlim 的压缩与解压能力，在处理大型构建日志、CI 输出、云日志、git diff 等场景下，将更多信号装入更少的上下文 Token。

### 安装

```bash
# 从源码构建
cd mcp-server
npm install
npm run build

# 或全局安装（发布后）
npm install -g @tokenslim/mcp-server
```

### 5 个可用工具

| 工具 | 功能 |
|---|---|
| `compress` | 压缩任意文本，返回可还原的 JSON |
| `decompress` | 将压缩结果还原为原始文本 |
| `compress_file` | 按路径压缩本地文件 |
| `smart_compress` | 智能判断：压缩率达标才压缩，否则返回原文 |
| `stats` | 查询当前会话的累计压缩统计 |

### 2 个可用资源

| URI | 说明 |
|---|---|
| `tokenslim://config` | CLI 版本、环境信息、项目配置 |
| `tokenslim://plugins` | 所有可用插件的名称与描述 |

### 各 Agent 配置示例

#### Claude Code / Claude Desktop

**文件：** `~/.claude/claude_desktop_config.json` 或项目根目录 `.mcp.json`

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

#### Cursor

**文件：** `.cursor/mcp.json`（项目级）或 `~/.cursor/mcp.json`（全局）

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

#### Windsurf

**文件：** `~/.codeium/windsurf/mcp_config.json`

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

#### Qoder

**文件（项目级）：** `.qoder/mcp_servers.json`
**文件（全局）：** `~/.qoder/mcp_servers.json`

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"]
    }
  }
}
```

#### npx 方式（发布后，无需本地安装）

```json
{
  "mcpServers": {
    "tokenslim": {
      "command": "npx",
      "args": ["-y", "@tokenslim/mcp-server"]
    }
  }
}
```

### 配置模板

项目中附带了一个可直接复制的模板文件：[`mcp_servers.example.json`](./mcp_servers.example.json)，只需将路径替换为你的本地路径即可。

### 环境变量

| 变量 | 说明 |
|---|---|
| `TOKENSLIM_CLI_PATH` | 绕过自动检测，直接指定 `tokenslim` 二进制路径 |
| `RUST_LOG` | 传递给 CLI 的日志级别（`info`、`debug`、`warn`、`error`）|
