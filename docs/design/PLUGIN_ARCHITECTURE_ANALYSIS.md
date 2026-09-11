# TokenSlim 插件架构设计原则分析

> 分析时间: 2026-05-11
> 目的: 分析现有插件的分工模式，指导 NDJSON 插件的设计决策

---

## 现有插件关系案例分析

### 案例 1: `vcs_git_plugin` vs `git_diff_plugin`

#### 分工模式

| 插件                | 职责                                                  | 检测模式                                 | 优先级 |
| ------------------- | ----------------------------------------------------- | ---------------------------------------- | ------ |
| **vcs_git_plugin**  | Git 命令输出（status, log, branch, reflog, shortlog） | `git status`, `git log`, `git branch` 等 | 150    |
| **git_diff_plugin** | Git diff 输出（diff, show, patch）                    | `diff --git`, `@@`, `+++`, `---`         | 160    |

#### 为什么分开？

**原因 1: 输出格式完全不同**
```bash
# vcs_git_plugin 处理
$ git status
On branch main
Changes not staged for commit:
  modified:   src/main.rs

# git_diff_plugin 处理
$ git diff
diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,5 @@
-old line
+new line
```

**原因 2: 压缩策略完全不同**
- `vcs_git_plugin`: 语义压缩（提取 branch/commit/author/message）
- `git_diff_plugin`: 结构压缩（压缩 diff header、hunk、路径）

**原因 3: 检测模式不冲突**
- `vcs_git_plugin`: 检测 `git status`, `git log` 等命令
- `git_diff_plugin`: 检测 `diff --git`, `@@` 等 diff 标记

**结论**: ✅ **分开是正确的** - 职责清晰，互不干扰

---

### 案例 2: `json_plugin` vs 假设的 `ndjson_plugin`

#### 对比分析

| 维度         | json_plugin                        | ndjson_plugin                      |
| ------------ | ---------------------------------- | ---------------------------------- |
| **输入格式** | 单个 JSON 对象                     | 每行一个 JSON 对象                 |
| **处理方式** | 递归压缩 JSON 树                   | 逐行解析 + 跨行聚合                |
| **压缩策略** | 数组截断、字符串字典化、Key 字典化 | 事件聚合、语义理解、结果汇总       |
| **检测模式** | `{` 开头，JSON 结构                | 每行都是 `{...}`                   |
| **典型场景** | API 响应、配置文件                 | `go test -json`, `npm test --json` |
| **语义理解** | ❌ 无（通用压缩）                   | ✅ 有（理解 Action/Test/Package）   |

#### 如果合并会怎样？

**方案 A: 在 `json_plugin` 中添加 NDJSON 模式**

```rust
pub struct JsonPlugin {
    pub config: JsonConfig,
    pub ndjson_mode: bool,  // 新增：NDJSON 模式开关
}

impl Plugin for JsonPlugin {
    fn compress(&self, slice: &Slice) -> CompressOutput {
        if self.ndjson_mode {
            // NDJSON 逻辑：逐行解析 + 聚合
            self.compress_ndjson(slice)
        } else {
            // 原有逻辑：单个 JSON 压缩
            self.compress_json(slice)
        }
    }
}
```

**问题**:
1. ❌ **职责混乱** - `json_plugin` 变成了"JSON 万能插件"
2. ❌ **配置复杂** - 需要区分两种模式的配置
3. ❌ **检测冲突** - 如何区分单个 JSON 和 NDJSON？
4. ❌ **代码臃肿** - 一个插件包含两套完全不同的逻辑
5. ❌ **测试困难** - 需要测试两种模式的交互
6. ❌ **违背单一职责原则**

---

### 案例 3: `rust_go_plugin` - 反面教材？

#### 当前状态

`rust_go_plugin` 同时处理：
- Rust 编译错误/警告
- Go panic 栈帧

#### 问题

```rust
pub struct RustGoPlugin {
    pub rust_compile_pattern: Arc<Regex>,  // Rust 相关
    pub go_panic_pattern: Arc<Regex>,      // Go 相关
    pub go_frame_pattern: Arc<Regex>,      // Go 相关
}
```

**是否应该拆分？**

**不拆分的理由** ✅:
1. 两者都是**编译/运行时错误**（职责相似）
2. 压缩策略相似（提取关键信息、压缩栈帧）
3. 代码量小（不会臃肿）
4. 检测模式不冲突

**拆分的理由** ❌:
1. Rust 和 Go 是不同语言（但这不是主要矛盾）
2. 未来可能需要更复杂的 Go 特定逻辑（如 Go test）

**结论**: 当前不拆分是合理的，但如果要添加 Go test 支持，**应该新建插件**。

---

## 插件设计原则总结

### 原则 1: 单一职责原则 (Single Responsibility Principle)

**定义**: 一个插件只负责一种输出格式或一类场景

**示例**:
- ✅ `vcs_git_plugin` - Git 命令输出
- ✅ `git_diff_plugin` - Git diff 输出
- ✅ `json_plugin` - 单个 JSON 对象
- ❌ `json_plugin` + NDJSON - 两种格式混合

---

### 原则 2: 输出格式决定插件边界

**规则**: 如果输出格式完全不同，应该分开

**判断标准**:
| 对比维度 | 相同 → 合并 | 不同 → 分开       |
| -------- | ----------- | ----------------- |
| 行结构   | 都是单行    | 单行 vs 多行      |
| 解析方式 | 都是正则    | 正则 vs JSON 解析 |
| 压缩策略 | 都是过滤    | 过滤 vs 聚合      |
| 语义理解 | 都不需要    | 通用 vs 特定工具  |

**示例**:
- `git status` vs `git diff` - 格式不同 → 分开 ✅
- 单个 JSON vs NDJSON - 格式不同 → 分开 ✅
- Rust 错误 vs Go panic - 格式相似 → 合并 ✅

---

### 原则 3: 压缩策略决定插件边界

**规则**: 如果压缩策略完全不同，应该分开

**压缩策略分类**:
1. **结构压缩** - 压缩格式本身（如 JSON 数组截断）
2. **语义压缩** - 提取关键信息（如 Git commit 信息）
3. **聚合压缩** - 跨行聚合（如 NDJSON 事件聚合）
4. **过滤压缩** - 保留/丢弃行（如 static_rule_plugin）

**示例**:
- `json_plugin` (结构压缩) vs `ndjson_plugin` (聚合压缩) → 分开 ✅
- `vcs_git_plugin` (语义压缩) vs `git_diff_plugin` (结构压缩) → 分开 ✅

---

### 原则 4: 检测模式决定插件优先级

**规则**: 如果检测模式冲突，优先级高的先执行

**示例**:
```rust
// git_diff_plugin (优先级 160) 先于 vcs_git_plugin (优先级 150)
// 因为 git diff 输出可能包含 "git" 关键字，但应该由 git_diff_plugin 处理
```

**NDJSON 场景**:
- `json_plugin` 检测单个 JSON 对象
- `ndjson_plugin` 检测每行都是 JSON 对象
- 需要设置合适的优先级，避免冲突

---

### 原则 5: 代码复杂度决定是否拆分

**规则**: 如果合并后代码复杂度显著增加，应该拆分

**判断标准**:
- 单个插件 > 500 行 → 考虑拆分
- 需要多个模式开关 → 考虑拆分
- 测试用例 > 50 个 → 考虑拆分

---

## NDJSON 插件决策分析

### 方案对比

| 方案                           | 优点             | 缺点                           | 评分   |
| ------------------------------ | ---------------- | ------------------------------ | ------ |
| **A. 扩展 json_plugin**        | 复用现有代码     | 职责混乱、代码臃肿、检测冲突   | ❌ 2/10 |
| **B. 扩展 rust_go_plugin**     | 与 Go 相关       | 职责不匹配（测试 vs 编译错误） | ❌ 1/10 |
| **C. 新建 ndjson_plugin**      | 职责清晰、易维护 | 需要新建代码                   | ✅ 9/10 |
| **D. 使用 static_rule_plugin** | 无需新代码       | 功能受限、压缩效果差           | ⚠️ 4/10 |

---

### 详细分析

#### 方案 A: 扩展 `json_plugin` ❌ **不推荐**

**违背的原则**:
- ❌ 单一职责原则 - 一个插件处理两种格式
- ❌ 输出格式原则 - 单个 JSON vs 多行 NDJSON
- ❌ 压缩策略原则 - 结构压缩 vs 聚合压缩

**代码示例**:
```rust
// 不好的设计
pub struct JsonPlugin {
    pub config: JsonConfig,
    pub ndjson_config: Option<NdjsonConfig>,  // 混乱
}

impl Plugin for JsonPlugin {
    fn detect(&self, slice: &Slice) -> Option<f32> {
        // 如何区分单个 JSON 和 NDJSON？
        if self.is_ndjson(slice) {
            // NDJSON 检测逻辑
        } else {
            // JSON 检测逻辑
        }
    }
    
    fn compress(&self, slice: &Slice) -> CompressOutput {
        if self.ndjson_config.is_some() {
            // NDJSON 压缩逻辑（200+ 行）
        } else {
            // JSON 压缩逻辑（300+ 行）
        }
    }
}
```

**问题**:
1. `detect` 方法需要两套逻辑
2. `compress` 方法需要两套逻辑
3. 配置结构混乱
4. 测试用例混合
5. 未来难以维护

---

#### 方案 B: 扩展 `rust_go_plugin` ❌ **不推荐**

**违背的原则**:
- ❌ 单一职责原则 - 编译错误 vs 测试输出
- ❌ 输出格式原则 - 文本栈帧 vs JSON 事件
- ❌ 压缩策略原则 - 栈帧压缩 vs 事件聚合

**问题**:
1. `rust_go_plugin` 专注于编译/运行时错误
2. Go test 输出是测试结果，不是错误
3. JSON 格式与栈帧格式完全不同
4. 会使插件职责不清晰

---

#### 方案 C: 新建 `ndjson_plugin` ✅ **强烈推荐**

**符合的原则**:
- ✅ 单一职责原则 - 只处理 NDJSON 格式
- ✅ 输出格式原则 - 专门处理逐行 JSON
- ✅ 压缩策略原则 - 专注于事件聚合
- ✅ 代码复杂度原则 - 独立模块，易维护

**代码示例**:
```rust
// 好的设计
pub struct NdjsonPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: NdjsonConfig,
}

impl Plugin for NdjsonPlugin {
    fn detect(&self, slice: &Slice) -> Option<f32> {
        // 专注于 NDJSON 检测
        // 1. 每行都是 JSON 对象
        // 2. 包含特定字段（如 Action, Test, Package）
    }
    
    fn compress(&self, slice: &Slice) -> CompressOutput {
        // 专注于 NDJSON 压缩
        // 1. 逐行解析 JSON
        // 2. 按 Package 分组
        // 3. 按 Test 聚合事件
        // 4. 生成紧凑摘要
    }
}
```

**优点**:
1. 职责清晰 - 只处理 NDJSON
2. 代码独立 - 不影响其他插件
3. 易于测试 - 测试用例独立
4. 易于维护 - 修改不影响其他插件
5. 易于扩展 - 可以支持其他 NDJSON 工具

---

#### 方案 D: 使用 `static_rule_plugin` ⚠️ **临时方案**

**适用场景**: 快速原型、临时需求

**配置示例**:
```toml
[[sections]]
name = "go_test_results"
enter = "^\\{.*\"Action\":\"(pass|fail)\""
keep = "^\\{.*\"Action\":\"(pass|fail)\""
```

**局限性**:
- ❌ 无法解析 JSON 字段
- ❌ 无法跨行聚合
- ❌ 无法生成摘要
- ❌ 压缩效果差（30-40% vs 60-70%）

---

## 最终建议

### 推荐方案: **新建 `ndjson_plugin`** ✅

**理由**:
1. **符合所有设计原则** - 单一职责、格式分离、策略独立
2. **参考现有模式** - 类似 `vcs_git_plugin` vs `git_diff_plugin` 的分工
3. **代码质量高** - 职责清晰、易维护、易测试
4. **扩展性强** - 可以支持其他 NDJSON 工具
5. **符合 TokenSlim 哲学** - 一个场景一个专用插件

### 实现路径

```
src/plugins/ndjson_plugin/
├── mod.rs          # 插件入口
├── types.rs        # NdjsonPlugin, NdjsonConfig, GoTestEvent 等类型
├── methods.rs      # detect() 和 compress() 实现
├── parser.rs       # 逐行 JSON 解析
├── aggregator.rs   # 事件聚合（按 Package/Test）
├── tests.rs        # 单元测试
└── showcase.rs     # 压缩效果展示
```

### 与现有插件的关系

```
json_plugin (优先级 140)
  ↓ 处理单个 JSON 对象
  
ndjson_plugin (优先级 145)
  ↓ 处理逐行 JSON 对象
  
rust_go_plugin (优先级 130)
  ↓ 处理编译错误和 panic
  
static_rule_plugin (优先级 100)
  ↓ 通用正则过滤
```

---

## 总结

**核心结论**:
- ✅ **新建插件** 是正确的选择
- ❌ **扩展现有插件** 会导致职责混乱
- ✅ 参考 `vcs_git_plugin` vs `git_diff_plugin` 的分工模式
- ✅ 遵循"一个场景一个专用插件"的设计哲学

**设计原则**:
1. 单一职责原则
2. 输出格式决定边界
3. 压缩策略决定边界
4. 检测模式决定优先级
5. 代码复杂度决定拆分

**行动建议**:
1. 新建 `src/plugins/ndjson_plugin/`
2. 参考 `json_plugin` 的结构
3. 实现 Go test -json 场景
4. 预留扩展接口（支持其他 NDJSON 工具）
