# CLI 层分计划

> **父计划**: [CODE_COMMENT_PLAN.md](./CODE_COMMENT_PLAN.md)  
> **执行前必须读取**: CODE_COMMENT_PLAN.md + 本文件  
> **范围**: `src/cli/` 目录  
> **依赖**: utils 层  
> **状态**: 进行中（部分文件已注释，文件级状态以本表勾选为准）

---

## 一、 本层概述

CLI 层是 TokenSlim 的命令行接口层，负责解析命令行参数、调度各个子命令、与用户交互。

**执行顺序（按依赖关系）**:
```
types.rs → common.rs → whitelist.rs → conpty_probe.rs → pty_runner.rs
→ app.rs → commands/ 各子命令 → mod.rs → test.rs
```

---

## 二、 文件清单与任务状态

### 2.1 基础模块

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 1 | `src/cli/types.rs` | ~10 | 待开始 | - | 类型定义 |
| 2 | `src/cli/common.rs` | ~10 | 待开始 | - | 通用函数 |
| 3 | `src/cli/whitelist.rs` | ~5 | 待开始 | - | 白名单逻辑 |
| 4 | `src/cli/conpty_probe.rs` | ~5 | 待开始 | - | ConPTY 探测 |
| 5 | `src/cli/pty_runner.rs` | ~10 | 待开始 | - | PTY 运行器 |

### 2.2 核心模块

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 6 | `src/cli/app.rs` | ~15 | 进行中 | - | CLI 主入口（已部分完成） |
| 7 | `src/cli/mod.rs` | ~5 | 待开始 | - | 模块导出 |

### 2.3 子命令模块（commands/）

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 8 | `src/cli/commands/mod.rs` | ~5 | 待开始 | - | 子命令模块导出 |
| 9 | `src/cli/commands/compress.rs` | ~10 | 待开始 | - | compress 子命令 |
| 10 | `src/cli/commands/decompress.rs` | ~10 | 待开始 | - | decompress 子命令 |
| 11 | `src/cli/commands/run.rs` | ~10 | 待开始 | - | run 子命令 |
| 12 | `src/cli/commands/benchmark.rs` | ~50 | 待开始 | - | benchmark 子命令（**巨型文件 3500+ 行**，需分段处理） |
| 13 | `src/cli/commands/doctor.rs` | ~10 | 待开始 | - | doctor 子命令 |
| 14 | `src/cli/commands/config.rs` | ~10 | 待开始 | - | config 子命令 |
| 15 | `src/cli/commands/repair.rs` | ~10 | 待开始 | - | repair 子命令 |
| 16 | `src/cli/commands/export.rs` | ~10 | 待开始 | - | export 子命令 |
| 17 | `src/cli/commands/serve_static.rs` | ~5 | 待开始 | - | serve_static 子命令 |

### 2.4 测试模块

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 18 | `src/cli/test.rs` | ~10 | 待开始 | - | CLI 测试 |

**小计**: 18 个文件，约 200 个函数

---

## 三、 重点注意事项

### 3.1 benchmark.rs 巨型文件处理策略

`src/cli/commands/benchmark.rs` 超过 3500 行，是本层最大的文件。必须采用分段处理策略：

1. **第一步**：用 Grep 提取所有 `pub fn` / `pub(crate) fn` / `fn` 列表，作为目录
2. **第二步**：按功能模块分组（benchmark 相关 / verify 相关 / 辅助函数）
3. **第三步**：逐个分组处理，每次只读目标函数及其上下文（±20 行）
4. **第四步**：每处理完一组，更新状态，避免上下文丢失

### 3.2 app.rs 已部分完成

`src/cli/app.rs` 已有部分注释，需要：
1. 检查现有注释质量
2. 找出未注释的函数
3. 补全剩余注释

---

## 四、 执行步骤

### 步骤 1：读取总计划 + 本分计划
- [ ] 读取 `docs/plans/CODE_COMMENT_PLAN.md`
- [ ] 读取本文件 `docs/plans/cli_plan.md`

### 步骤 2：基础模块
- [ ] 处理 `src/cli/types.rs`
- [ ] 处理 `src/cli/common.rs`
- [ ] 处理 `src/cli/whitelist.rs`
- [ ] 处理 `src/cli/conpty_probe.rs`
- [ ] 处理 `src/cli/pty_runner.rs`

### 步骤 3：核心模块
- [ ] 补全 `src/cli/app.rs` 剩余注释
- [ ] 处理 `src/cli/mod.rs`

### 步骤 4：子命令模块
- [ ] 处理 `src/cli/commands/mod.rs`
- [ ] 处理 `src/cli/commands/compress.rs`
- [ ] 处理 `src/cli/commands/decompress.rs`
- [ ] 处理 `src/cli/commands/run.rs`
- [ ] 处理 `src/cli/commands/benchmark.rs`（分段处理）
- [ ] 处理 `src/cli/commands/doctor.rs`
- [ ] 处理 `src/cli/commands/config.rs`
- [ ] 处理 `src/cli/commands/repair.rs`
- [ ] 处理 `src/cli/commands/export.rs`
- [ ] 处理 `src/cli/commands/serve_static.rs`

### 步骤 5：测试模块
- [ ] 处理 `src/cli/test.rs`

### 步骤 6：验证
- [ ] 运行 `tokenslim run cargo check` — 确保无编译错误
- [ ] 抽查 5 个文件，检查注释质量

### 步骤 7：收口
- [ ] 更新本文件顶部状态为「已完成」
- [ ] 在总计划中标记本分计划为完成
- [ ] 记录发现的问题到「问题清单」

---

## 五、 问题清单

（执行过程中发现的问题记录在此）

---

## 六、 完成标准

- [ ] 18 个文件全部处理完毕
- [ ] 所有 `pub fn` / `pub(crate) fn` / `fn` 都有中文注释
- [ ] 所有 `struct` / `enum` 都有中文注释
- [ ] `cargo check` 通过
- [ ] 问题清单已记录
