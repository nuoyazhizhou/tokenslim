# Bin 工具分计划

> **父计划**: [CODE_COMMENT_PLAN.md](./CODE_COMMENT_PLAN.md)  
> **执行前必须读取**: CODE_COMMENT_PLAN.md + 本文件  
> **范围**: `src/bin/` 目录  
> **依赖**: utils 层 + core 层 + plugins 层  
> **状态**: 进行中（部分文件已注释，文件级状态以本表勾选为准）

---

## 一、 本层概述

Bin 层是 TokenSlim 的独立二进制工具，包含服务器、日志挖掘、基准测试等辅助工具。

**执行顺序**：
```
log_miner.rs → log_reorder.rs → pipeline_bench.rs
→ tree_dict_experiment.rs → tokenslim-server.rs
```

---

## 二、 文件清单与任务状态

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 1 | `src/bin/log_miner.rs` | ~15 | 待开始 | - | 日志挖掘工具 |
| 2 | `src/bin/log_reorder.rs` | ~10 | 待开始 | - | 日志重排序工具 |
| 3 | `src/bin/pipeline_bench.rs` | ~15 | 待开始 | - | 流水线基准测试 |
| 4 | `src/bin/tree_dict_experiment.rs` | ~10 | 待开始 | - | 树字典实验 |
| 5 | `src/bin/tokenslim-server.rs` | ~20 | 待开始 | - | HTTP 服务器 |

**小计**: 5 个文件，约 70 个函数

---

## 三、 执行步骤

### 步骤 1：读取总计划 + 本分计划
- [ ] 读取 `docs/plans/CODE_COMMENT_PLAN.md`
- [ ] 读取本文件 `docs/plans/bin_plan.md`

### 步骤 2：逐个文件处理
- [ ] 处理 `src/bin/log_miner.rs`
- [ ] 处理 `src/bin/log_reorder.rs`
- [ ] 处理 `src/bin/pipeline_bench.rs`
- [ ] 处理 `src/bin/tree_dict_experiment.rs`
- [ ] 处理 `src/bin/tokenslim-server.rs`

### 步骤 3：验证
- [ ] 运行 `tokenslim run cargo check` — 确保无编译错误
- [ ] 抽查 2 个文件，检查注释质量

### 步骤 4：收口
- [ ] 更新本文件顶部状态为「已完成」
- [ ] 在总计划中标记本分计划为完成
- [ ] 记录发现的问题到「问题清单」

---

## 四、 问题清单

（执行过程中发现的问题记录在此）

---

## 五、 完成标准

- [ ] 5 个文件全部处理完毕
- [ ] 所有 `pub fn` / `fn` 都有中文注释
- [ ] 所有 `struct` / `enum` 都有中文注释
- [ ] `cargo check` 通过
- [ ] 问题清单已记录
