# 多语言 SDK (TokenSlim SDKs)

## 1. 模块职责

提供 Python、Node.js、Java 三种语言的官方 SDK，封装对 TokenSlim Sidecar Server 的 HTTP REST 调用。所有 SDK 均为零外部依赖，仅使用各语言标准库。

## 2. SDK 概览

| SDK | 文件 | 语言 | 最低版本 | 依赖 |
|-----|------|------|----------|------|
| Python | `sdk/python/tokenslim_sdk.py` | Python 3 | 3.6+ | 仅 `urllib`（标准库） |
| Node.js | `sdk/nodejs/tokenslim.js` | JavaScript | Node.js 12+ | 仅 `http`（标准库） |
| Java | `sdk/java/TokenSlimClient.java` | Java | 11+ | 仅 `java.net.http`（标准库） |

## 3. 统一 API 设计

所有 SDK 遵循相同的接口设计：

| 方法 | HTTP | 端点 | 说明 |
|------|------|------|------|
| `isHealthy()` / `health()` | GET | `/health` | 健康检查 |
| `compress(text)` | POST | `/compress` | 压缩文本 |
| `decompress(tokens, dictionary)` | POST | `/decompress` | 解压还原 |

## 4. Python SDK

### 类：TokenSlimClient

```python
from tokenslim_sdk import TokenSlimClient

client = TokenSlimClient(host="127.0.0.1", port=10086)
```

| 方法 | 参数 | 返回值 |
|------|------|--------|
| `is_healthy()` | 无 | `bool` |
| `compress(text)` | `text: str` | `dict`（含 tokens/dictionary/metadata） |
| `decompress(tokens, dictionary)` | `tokens: list, dictionary: dict` | `dict` |

### 实现细节
- 基于 `urllib.request`，零外部依赖
- 自动 JSON 序列化/反序列化
- HTTP 错误时抛出 `Exception` 并附带状态码和响应体

## 5. Node.js SDK

### 类：TokenSlimClient

```javascript
const TokenSlimClient = require('./tokenslim');
const client = new TokenSlimClient('127.0.0.1', 10086);
```

| 方法 | 参数 | 返回值 |
|------|------|--------|
| `isHealthy()` | 无 | `Promise<boolean>` |
| `compress(text)` | `text: string` | `Promise<object>` |
| `decompress(tokens, dictionary)` | `tokens: array, dictionary: object` | `Promise<object>` |

### 实现细节
- 基于 `http` 模块，零外部依赖
- 所有方法返回 Promise（异步）
- 自动处理 JSON 序列化/反序列化
- 支持 CommonJS (`require`) 导入

## 6. Java SDK

### 类：TokenSlimClient

```java
TokenSlimClient client = new TokenSlimClient("127.0.0.1", 10086);
// 或使用默认构造
TokenSlimClient client = new TokenSlimClient();
```

| 方法 | 参数 | 返回值 |
|------|------|--------|
| `health()` | 无 | `CompletableFuture<String>` |
| `compress(text)` | `text: String` | `CompletableFuture<String>` |
| `decompress(tokensJson, dictionaryJson)` | `tokensJson: String, dictionaryJson: String` | `CompletableFuture<String>` |

### 实现细节
- 基于 Java 11 `java.net.http.HttpClient`
- 所有方法返回 `CompletableFuture`（异步）
- 简单的 JSON 字符串拼接（MVP 阶段，未引入 JSON 库）
- 包含 `main()` 方法用于快速测试

## 7. 通信协议

所有 SDK 与 `tokenslim-server` 通过 HTTP REST 通信：

```
默认地址: http://127.0.0.1:10086

GET  /health      → {"status": "UP"}
POST /compress    → {"text": "..."}  → {"tokens": [...], "dictionary": {...}, "metadata": {...}}
POST /decompress  → {"tokens": [...], "dictionary": {...}}  → {"text": "..."}
```

### 鉴权

当服务器设置了 `TOKENSLIM_API_KEY` 环境变量时，所有请求需携带：
```
Authorization: Bearer <YourKey>
```

## 8. 使用示例

### Python
```python
client = TokenSlimClient()
if client.is_healthy():
    result = client.compress("your log text...")
    print(f"Ratio: {result['metadata']['compression_ratio']:.2%}")
```

### Node.js
```javascript
const client = new TokenSlimClient();
const healthy = await client.isHealthy();
if (healthy) {
    const result = await client.compress("your log text...");
    console.log(`Ratio: ${result.metadata.compression_ratio}`);
}
```

### Java
```java
TokenSlimClient client = new TokenSlimClient();
String health = client.health().get();
String result = client.compress("your log text...").get();
System.out.println("Result: " + result);
```

---

*最后更新：2026-05-13（基于源码 `sdk/` 同步）*