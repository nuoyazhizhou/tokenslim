# TokenSlim 未来规划 (Future Plan)

> 最后更新: 2026-05-13
> 当前版本: v3（P1-P4 全部完成）

---

## 已完成功能（不再列入未来规划）

以下功能曾在早期规划中列为"未来"，现已全部实现：

| 功能 | 状态 | 位置 |
|------|------|------|
| 多线程并发压缩 | ✅ v6.2 重构完成 | `src/core/compression_pipeline/` |
| 硬超时隔离 | ✅ SafeExecutor | `src/core/safe_executor/` |
| JSON 脱水插件 | ✅ | `src/plugins/json_plugin/` |
| YAML 脱水插件 | ✅ | `src/plugins/yaml_plugin/` |
| XML/HTML 脱水插件 | ✅ | `src/plugins/xml_html_plugin/` |
| VSCode 扩展 | ✅ | `vscode-extension/` |
| JetBrains 插件 | ✅ | `jetbrains-plugin/` |
| Chrome 扩展 | ✅ | `chrome-extension/` |
| REST API Sidecar | ✅ axum 6 端点 | `src/bin/tokenslim-server.rs` |
| Python SDK | ✅ | `sdk/python/` |
| Node.js SDK | ✅ | `sdk/nodejs/` |
| Java SDK | ✅ | `sdk/java/` |

---

## 一、核心引擎优化 (Core Engine)

### 1.1 基于本地 Embedding 模型的智能识别
- **现状**: `ContentAnalyzer` 使用纯正则提取特征
- **目标**: 集成 `candle` 或 `onnxruntime`，使用小参数量模型（如 `all-MiniLM`）将切片转换为向量，识别未见过的全新格式错误日志
- **优先级**: P3（低优，需评估 ROI）

### 1.2 跨切片上下文去重
- **现状**: `DedupEngine` 执行行级/局部帧级去重
- **目标**: 基于滑动窗口的全局跨切片长距离去重（如日志首尾出现相同长配置串）
- **优先级**: P3（低优）

### 1.3 SIMD 进一步优化
- **现状**: 已使用 SSE4.2/AVX2 加速
- **目标**: AVX-512 支持，进一步加速字符串匹配
- **优先级**: P3（低优，硬件覆盖率有限）
- **跟踪**: `docs/tasks/CONSOLIDATED_TASKS.md`

---

## 二、高级插件扩展 (Advanced Plugins)

### 2.1 编程语言 AST 插件
- **目标**: 通过树结构分析，去掉样板代码、注释、重复引用，提取"逻辑骨架"
- **候选语言**: C++ / JavaScript / Python
- **优先级**: P3（低优，需评估实际压缩收益）

### 2.2 LLM 自动化编写插件
- **目标**: 遇到无法识别的日志格式时，自动让大模型即时生成 Rust 匹配正则并热加载
- **技术挑战**: 代码生成安全性、热加载稳定性
- **优先级**: P3（低优，实验性功能）

---

## 三、Server 模式高级功能

### 3.1 Web Dashboard
- **目标**: 可视化统计面板，展示压缩率趋势、Token 节省、过滤器使用情况
- **优先级**: P2（中优）

### 3.2 JWT 认证
- **目标**: 替代当前 Bearer Token，支持多用户/多租户
- **优先级**: P2（中优）

### 3.3 WebSocket 流式压缩
- **目标**: 支持长连接流式压缩，适用于实时日志场景
- **优先级**: P3（低优）

### 3.4 Docker 镜像发布
- **目标**: 提供官方 Docker 镜像，简化部署
- **优先级**: P2（中优）

---

## 四、优先级汇总

| 优先级 | 功能 | 类型 |
|--------|------|------|
| P2 | Web Dashboard | Server 增强 |
| P2 | JWT 认证 | Server 增强 |
| P2 | Docker 镜像 | 部署 |
| P3 | Embedding 智能识别 | 引擎 |
| P3 | 跨切片去重 | 引擎 |
| P3 | SIMD AVX-512 | 引擎 |
| P3 | 编程语言 AST 插件 | 插件 |
| P3 | LLM 自动生成插件 | 插件 |
| P3 | WebSocket 流式压缩 | Server 增强 |