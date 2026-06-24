# TokenSlim Server (REST API 契约)

## 1. 模块职责
`tokenslim-server` 基于 `axum` 提供 TokenSlim 的 Sidecar/远程服务能力，用于 HTTP 接入压缩、解压缩、指标查询与配置热重载。

## 2. 鉴权与基础行为
- 认证：支持 Bearer Token（环境变量 `TOKENSLIM_API_KEY`）。
- CORS：已启用。
- 响应编码：UTF-8 JSON（指标端点除外，`/metrics` 为 Prometheus 文本）。
- 运行参数：`TOKENSLIM_HOST` / `TOKENSLIM_PORT`。

## 3. 路由契约（当前 12 个端点）

### 3.1 健康与指标
- `GET /health`
  - 用途：健康检查与运行状态探针。
- `GET /metrics`
  - 用途：Prometheus 指标抓取。
- `GET /metrics/detail`
  - 用途：结构化指标明细（JSON）。

### 3.2 统计查询
- `GET /stats/aggregate`
  - 用途：累计节省统计聚合。
- `GET /stats/daily`
  - 用途：按天统计明细。
- `GET /stats/by-filter`
  - 用途：按过滤器维度统计。

### 3.3 核心能力
- `POST /compress`
  - 用途：文本压缩（支持 AI Export、重排等参数）。
- `POST /compress/stream`
  - 用途：Server-Sent Events 流式压缩，进度实时推送，避免大输入阻塞。
- `POST /decompress`
  - 用途：压缩结果回放/解压。
- `POST /reload`
  - 用途：重新加载配置（热更新）。

### 3.4 插件元数据
- `GET /plugins`
  - 用途：列出全部已注册插件（id、版本、压缩/解压端点、是否启用）。

### 3.5 实时流
- `GET /ws/tail`
  - 用途：WebSocket 长连接，订阅日志/文件变更流，前端 tail 模式后端。

## 4. 核心结构与处理器
- 状态结构：`AppState`
- 请求结构：`CompressRequest`, `DecompressRequest`
- 关键处理器：
  - `compress_handler()` — `POST /compress`
  - `compress_stream_handler()` — `POST /compress/stream`（SSE）
  - `decompress_handler()` — `POST /decompress`
  - `metrics_handler()` — `GET /metrics`（Prometheus 文本）
  - `metrics_detail_handler()` — `GET /metrics/detail`（JSON）
  - `stats_aggregate_handler()` / `stats_daily_handler()` / `stats_by_filter_handler()` — 统计三件套
  - `reload_config_handler()` — `POST /reload`
  - `plugins_handler()` — `GET /plugins`
  - `tail_ws_handler()` — `GET /ws/tail`（WebSocket upgrade）
  - `static_handler()` — SPA fallback（rust-embed `webui/`）

## 5. 变更约束
- 新增/修改端点时，必须同步更新：
  1. 本文档 `docs/design/server.md`
  2. `README.md` Sidecar 示例
  3. `docs/reports/IMPLEMENTATION_STATUS.md` 与 `docs/plans/FEATURE_ROADMAP.md` 的端点数量/清单
