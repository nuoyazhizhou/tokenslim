# TokenSlim SDK 使用文档

> 目标：把 LLM 拿到的长输出在送进 prompt 前压缩 50%–95%。

TokenSlim 提供三套语言 SDK，调用同一个 HTTP API。底层是 Rust 写的 sidecar server。

| 语言 | 包 | 状态 | 协议 |
|---|---|---|---|
| Node.js / TypeScript | `tokenslim` (npm) | ✅ 已发布 0.1.0 | REST/JSON |
| Python | `tokenslim_sdk.py` (单文件) | ✅ 可用 | REST/JSON |
| Java 11+ | `TokenSlimClient` (单文件) | ✅ 可用 | REST/JSON |

服务端点统一：`http://<host>:10086`（默认 `127.0.0.1`）

---

## 1. 前置条件

**先起 server**：

```bash
# 任选一种
tokenslim serve --port 10086
# 或
docker run -d -p 10086:10086 ghcr.io/nuoyazhizhou/tokenslim
```

**确认服务在线**：

```bash
curl http://127.0.0.1:10086/health
# {"status":"UP","version":"0.1.0","plugin_count":55}
```

---

## 2. Node.js / TypeScript（推荐）

### 安装

```bash
npm install tokenslim
```

> 完整包位置：[packages/sdk-nodejs/](../../packages/sdk-nodejs/)

### 最小例子

```ts
import { TokenSlimClient } from 'tokenslim';

const client = new TokenSlimClient();     // default: 127.0.0.1:10086
if (await client.isHealthy()) {
    const r = await client.compress(longLog, { preset: 'ai' });
    console.log(`${r.original_tokens} → ${r.compressed_tokens}`);
}
```

### 完整接口

```ts
import {
    TokenSlimClient,
    TokenSlimError,
    type CompressResponse,
} from 'tokenslim';

const client = new TokenSlimClient({
    host: '127.0.0.1',
    port: 10086,
    timeoutMs: 30_000,
    headers: { 'X-Trace-Id': traceId },
});

// 健康检查
const up: boolean = await client.isHealthy();

// 压缩
const r: CompressResponse = await client.compress(text, {
    preset: 'ai',                    // 'ai' | 'balanced' | 'lossless'
    plugin_hint: 'vcs_git_plugin',   // 可选，强制某个插件
});

// 解压
const back: string = await client.decompress(r.compressed, r.dictionary ?? {});

// 元数据
const info = await client.describe();
// { version, plugin_count, families }
```

### 错误处理

```ts
import { TokenSlimError } from 'tokenslim';

try {
    await client.compress(text);
} catch (e) {
    if (e instanceof TokenSlimError) {
        console.error(`HTTP ${e.statusCode}: ${e.message}`);
    }
}
```

### 实战：喂给 OpenAI

```ts
import OpenAI from 'openai';
import { TokenSlimClient } from 'tokenslim';

const slim = new TokenSlimClient();
const openai = new OpenAI();

const rawGitLog = await $`git log --oneline -n 500`.then(r => r.stdout);
const r = await slim.compress(rawGitLog, { plugin_hint: 'vcs_git_plugin' });

const resp = await openai.chat.completions.create({
    model: 'gpt-4o-mini',
    messages: [{
        role: 'user',
        content: `以下是压缩后的 git log（节省 ${(100 - r.ratio * 100).toFixed(1)}% token）：\n\n${r.compressed}`,
    }],
});
```

---

## 3. Python

### 安装

```bash
# 临时方式（推荐先用这个验证）
curl -O https://raw.githubusercontent.com/nuoyazhizhou/tokenslim/main/sdk/python/tokenslim_sdk.py
```

或待 PyPI 上线后：

```bash
pip install tokenslim
```

### 最小例子

```python
from tokenslim_sdk import TokenSlimClient

c = TokenSlimClient()  # default: 127.0.0.1:10086
if c.is_healthy():
    r = c.compress(long_log)
    print(f"{r['original_tokens']} → {r['compressed_tokens']}")
```

### 完整接口

```python
client = TokenSlimClient(host="127.0.0.1", port=10086)

# 健康检查
ok = client.is_healthy()

# 压缩
result = client.compress(text)
# 返回 dict: { tokens, dictionary, metadata }

# 解压
restored = client.decompress(result['tokens'], result['dictionary'])
```

### 实战：包装 pytest 输出

```python
import subprocess
from tokenslim_sdk import TokenSlimClient

slim = TokenSlimClient()
pytest_out = subprocess.check_output(["pytest", "-v", "tests/"], text=True)
r = slim.compress(pytest_out)

# 喂给 LLM
prompt = f"""以下 pytest 输出已被压缩（节省 {100 - r['metadata']['compression_ratio']*100:.1f}%）：
{r['tokens']}

请告诉我哪些测试失败、为什么失败。"""
```

---

## 4. Java

### 安装

复制 [`sdk/java/TokenSlimClient.java`](../../sdk/java/TokenSlimClient.java) 到你的项目，包名 `com.tokenslim.sdk`。需要 Java 11+。

### 最小例子

```java
import com.tokenslim.sdk.TokenSlimClient;

TokenSlimClient client = new TokenSlimClient();
String health = client.health().get();
System.out.println(health);

String log = "2024-12-26T07:47:22.609Z [ERROR] Failed to connect to database";
String result = client.compress(log).get();
System.out.println(result);
```

### 完整接口

```java
// 自定义 host/port
TokenSlimClient client = new TokenSlimClient("127.0.0.1", 10086);

// 健康检查 — CompletableFuture<String>
CompletableFuture<String> health = client.health();

// 压缩
CompletableFuture<String> compressed = client.compress(text);

// 解压（注意 tokens 和 dictionary 已经是 JSON 字符串）
CompletableFuture<String> restored = client.decompress(tokensJson, dictJson);
```

### Spring Boot 集成示例

```java
@Service
public class LogCompressionService {
    private final TokenSlimClient client = new TokenSlimClient();

    public String compressForLlm(String rawLog) throws Exception {
        if (rawLog.length() < 1024) return rawLog;   // 太短不压
        return client.compress(rawLog).get();
    }
}
```

---

## 5. 通用 REST API（其它语言）

所有 SDK 都是这个 HTTP 协议的薄封装。

### `GET /health`

```bash
curl http://127.0.0.1:10086/health
```

```json
{"status":"UP","version":"0.1.0","plugin_count":55}
```

### `POST /compress`

```bash
curl -X POST http://127.0.0.1:10086/compress \
  -H "Content-Type: application/json" \
  -d '{"text":"<your long log here>","preset":"ai"}'
```

```json
{
    "compressed": "[GRADLE] tasks=5, FAILURE, testDebugUnitTest, Run ./gradlew\n$GRADLE/P1",
    "original_tokens": 1234,
    "compressed_tokens": 87,
    "ratio": 0.0705,
    "plugin_used": "android_gradle_plugin",
    "dictionary": {"$GRADLE/P1": "/home/user/projects/myapp/build/"}
}
```

### `POST /decompress`

```bash
curl -X POST http://127.0.0.1:10086/decompress \
  -H "Content-Type: application/json" \
  -d '{"compressed":"<compressed string>","dictionary":{...}}'
```

### `GET /describe`

```bash
curl http://127.0.0.1:10086/describe
```

```json
{
    "version": "0.1.0",
    "plugin_count": 55,
    "families": ["vcs", "build", "trace", "data", "shell", "utility"]
}
```

---

## 6. 调试小技巧

| 想做的事 | 命令 / 方式 |
|---|---|
| 看 server 选了哪个插件 | 服务端开 `RUST_LOG=debug` 重启 |
| 强制走某个插件 | `client.compress(text, { plugin_hint: 'vcs_git_plugin' })` |
| 看压缩前后的 diff | 服务端开 `RUST_LOG=tokenslim=trace` |
| 跑自带测试样本 | `tokenslim run pytest samples/pytest_plugin/` |

---

## 7. 下一步

- [5 分钟 quickstart](./QUICKSTART.md)
- [完整使用手册](./USER_GUIDE.md)
- [插件注册表](../../plugins_registry.md)
