# TokenSlim 5-Minute Quickstart

> 目标：5 分钟内把 TokenSlim 跑起来，并把一段长日志压给 LLM。

## 0. 它在做什么（一句话）

TokenSlim 把冗长的 VCS / 构建 / 测试 / 数据库日志压缩成更短的等价摘要，**省 50%–95% token**，且保留决策相关行（错误、警告、状态变更、关键路径）。

---

## 1. 装 CLI（30 秒）

### npm（任意平台，**推荐** — 一行把 CLI + server + 60+ 插件配置全部装好）

```bash
# 装到当前项目
npm install tokenslim

# 或者全局装，让 `tokenslim` / `tokenslim-server` 命令直接可用
npm install -g tokenslim
```

`tokenslim` 包内置 6 个平台二进制作为 `optionalDependencies`：

- `@tokenslim/cli-binary-linux-x64-gnu` / `linux-arm64-gnu`
- `@tokenslim/cli-binary-darwin-x64` / `darwin-arm64`
- `@tokenslim/cli-binary-windows-x64` / `windows-arm64`

npm/pnpm/yarn 会自动选与本机匹配的那个，下载 `tokenslim` + `tokenslim-server` 二进制 + 60+ 插件配置（`config/plugins/*.toml`）到 `node_modules/`。然后 `bin/tokenslim.js` 这个 Node 包装器会把调用转给真实二进制。

如果网络不通导致平台包没装上，会自动 fallback 到 GitHub Releases 下载；如果仍然失败，会打 warning 但不报错（SDK 仍可作为 REST 客户端使用）。

### macOS / Linux（脚本）

```bash
curl -fsSL https://raw.githubusercontent.com/nuoyazhizhou/tokenslim/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/nuoyazhizhou/tokenslim/main/install.ps1 | iex
```

### 用 cargo（任意平台）

```bash
cargo install tokenslim
cargo install tokenslim-server
```

验证：

```bash
tokenslim --version
# tokenslim 0.1.0
```

> 💡 `tokenslim --help` 查看所有子命令；`tokenslim plugins list` 列出 60+ 插件；`tokenslim explain-plugin --explain-command "你的命令"` 解释路由选择。

---

## 2. 试一行（10 秒）

把任何长输出丢给 `tokenslim run`，它会自动识别日志类型并选插件：

```bash
git log --oneline -n 200 | tokenslim run
```

输出会变成结构化摘要，行数减少 80% 左右，但 commit 哈希、文件路径、错误、警告全部保留。

---

## 3. 起服务（1 分钟）

很多工具（VSCode 扩展、Chrome 扩展、Node.js SDK）需要 HTTP 服务。

```bash
tokenslim serve --port 10086
```

输出：

```
TokenSlim server listening on http://127.0.0.1:10086
Health: GET /health
Compress: POST /compress
Decompress: POST /decompress
```

健康检查：

```bash
curl http://127.0.0.1:10086/health
# {"status":"UP","version":"0.1.0","plugin_count":55}
```

---

## 4. SDK 调用（2 分钟）

### Node.js / TypeScript

```bash
npm install tokenslim
```

```ts
import { TokenSlimClient } from 'tokenslim';

const c = new TokenSlimClient();
const r = await c.compress(longLog);
console.log(`${r.original_tokens} → ${r.compressed_tokens} tokens`);
```

### Python

```bash
pip install tokenslim   # 暂无 PyPI 包，先用 sdk/python/tokenslim_sdk.py
```

```python
from tokenslim_sdk import TokenSlimClient

c = TokenSlimClient()
r = c.compress(long_log)
print(f"{r['original_tokens']} → {r['compressed_tokens']} tokens")
```

### Java

见 [`docs/guides/SDK_USAGE.md`](./SDK_USAGE.md)。

---

## 5. 接 LLM 实战（30 秒）

最小可工作示例 —— 把 LLM 拿到的 token 喂给 TokenSlim 压缩再回传：

```ts
import OpenAI from 'openai';
import { TokenSlimClient } from 'tokenslim';

const slim = new TokenSlimClient();
const openai = new OpenAI();

// 1) 你有一段 5000 字的 git log
const raw = await runShell('git log --oneline -n 500');

// 2) 压缩
const r = await slim.compress(raw);
console.log(`saved ${100 - r.ratio * 100}% tokens`);

// 3) 拼进 prompt 发给 LLM
const resp = await openai.chat.completions.create({
    model: 'gpt-4o-mini',
    messages: [{ role: 'user', content: `Summarize:\n${r.compressed}` }],
});

// 4) 不需要时再解压
const back = await slim.decompress(r.compressed, r.dictionary ?? {});
```

---

## 6. 常用命令速查

| 任务 | 命令 |
|---|---|
| 包装任何命令并压缩输出 | `tokenslim run <cmd>` |
| 起 HTTP 服务 | `tokenslim serve --port 10086` |
| 健康检查 | `curl localhost:10086/health` |
| 看所有插件 | `tokenslim plugins` |
| 看某个插件怎么压 | `tokenslim explain-plugin vcs_git_plugin` |
| 跑自带的测试样本 | `tokenslim run pytest tests/` |
| 看压缩为什么这样 | `tokenslim run --explain-route -- git status` |

---

## 7. 常见问题

**Q: 压缩有损吗？**
A: 可调。`--preset ai` 激进（90%+ 节省），`--preset balanced` 默认（70%），`--preset lossless` 全保。也可以 `tokenslim decompress` 把压缩串还原。

**Q: 怎么知道它选对了插件？**
A: `tokenslim run --explain-route -- git status` 会打印路由决策。

**Q: 跟直接 gzip 区别？**
A: gzip 节省 70% 但**不能解压回可读文本**（gzip 是字节流），LLM 读不懂。TokenSlim 保留可读性 + 字典化可还原 + 删噪声留信号。

---

## 8. 下一步

- [SDK 使用文档（Node / Python / Java）](./SDK_USAGE.md)
- [完整使用手册](./USER_GUIDE.md)
- [插件注册表](../../plugins_registry.md)
