# TokenSlim Context for Windows

此包安装并启动本机 `tokenslim-server`，供已集成 TokenSlim Context Adapter 的 CLIProxyAPI 与 cc-switch 调用。它不会自行拦截模型流量，也不会上传审计或请求正文。

## 安装

在解压后的目录中执行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install-tokenslim-context.ps1 -Start
```

该命令将 `tokenslim-server.exe` 复制到 `%LOCALAPPDATA%\TokenSlim\bin`，在用户环境变量中写入本机 `TOKENSLIM_SERVER_URL`、`TOKENSLIM_HOST` 和 `TOKENSLIM_PORT`，并启动 `127.0.0.1:8765` 上的本地服务。

在线请求改写默认保持关闭。只有在已安装包含 TokenSlim Context Adapter 的 CLIProxyAPI 或 cc-switch 版本后，才可明确启用：

```powershell
.\install-tokenslim-context.ps1 -Start -EnableTransformation
```

重新启动 CLIProxyAPI 或 cc-switch 后，适配器会仅将经过协议确认的工具结果发送给本机 Server 的 `/compress` 端点。系统提示词、开发者提示词、普通用户文本、认证头、工具 schema 和工具参数不会被改写或作为压缩候选传输。

## 环境变量

| 变量 | 默认值 | 作用 |
|---|---:|---|
| `TOKENSLIM_TRANSFORM_ENABLED` | 未设置 / false | 显式允许在线工具结果改写 |
| `TOKENSLIM_SERVER_URL` | `http://127.0.0.1:8765` | 本地 Server 地址；适配器默认拒绝远程地址 |
| `TOKENSLIM_SERVER_TIMEOUT_MS` | `1500` | 本地压缩调用超时，范围 `100–10000` 毫秒 |
| `TOKENSLIM_TRANSFORM_MIN_BYTES` | `4096` | 工具结果达到该字节数才尝试压缩 |
| `TOKENSLIM_AUDIT_JSONL` | 未设置 | CLIProxyAPI 的可选无正文审计文件 |
| `TOKENSLIM_CC_SWITCH_AUDIT_JSONL` | 未设置 | cc-switch 的可选无正文审计文件 |

若 Server 未运行、超时、拒绝请求、返回无效 JSON，或压缩后文本没有严格变短，宿主适配器会保留原始工具结果并继续请求。这是 fail-open 行为。

## 宿主启动

- CLIProxyAPI：运行 `other\CLIProxyAPI\start-tokenslim-observe-only.ps1`。若上述转换变量已显式启用，启动入口将同时加载在线转换器；若未启用，保持既有 observe-only 行为。
- cc-switch：运行 `other\cc-switch\start-tokenslim-observe-only.ps1`。若上述转换变量已显式启用，转发器会在审计原请求后尝试改写已确认的工具结果；其余内容保持不变。

当前包只适用于已包含本轮 TokenSlim Context 代码的 CLIProxyAPI 与 cc-switch 构建版本。正式发布时应将适配器随各自宿主版本发放，并将本包作为 companion runtime 提供。
