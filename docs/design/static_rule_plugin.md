# StaticRule Plugin 模块设计

## 1. 模块目标
`static_rule_plugin` 用于把“简单插件能力”从 Rust 代码下沉为 TOML 配置，降低新增工具日志支持门槛。

典型场景：
- 构建失败列表提取
- CI 统计汇总（如 `N passed` 累加）
- 有明确 enter/exit 边界的块状日志

## 2. 配置模型（TOML）

核心结构：
- `output_template`：输出模板，支持 `{body}` 与聚合变量占位符。
- `[[sections]]`：规则分段。
  - `name`
  - `enter`
  - `exit`（可选）
  - `keep[]` / `drop[]`
  - `[[sections.aggregates]]`
    - `name`
    - `kind = count|sum`
    - `pattern`（可选）

示例：
```toml
output_template = "SUMMARY failed={failed_count}\n{body}"

[[sections]]
name = "failed_tests"
enter = "^FAILED tests"
exit = "^===="
keep = ["^FAILED", "^ERROR"]

[[sections.aggregates]]
name = "failed_count"
kind = "count"
pattern = "^FAILED"
```

## 3. 执行语义

### 3.1 Section 状态机
- 初始 `inactive`
- 命中 `enter` -> `active`
- 命中 `exit` -> `inactive`
- `active` 状态内执行 keep/drop 与 aggregate

### 3.2 聚合器
- `count`：满足 pattern（或未配置 pattern）计数 +1
- `sum`：从 pattern 第 1 捕获组提取数字并累加

### 3.3 输出
- 若存在 `output_template`，替换 `{body}` 与 `{agg_name}` 占位符
- 否则输出 `$SR|k=v` + body（有内容时换行追加）

## 4. 与主链路的关系
- 插件实现 `Plugin` trait，可由 `PluginDispatcher` 常规调度。
- CLI 提供独立验证流：
  - `--verify-rule`
  - `--verify-fixture`
  - `--verify-expected`
- 支持单文件与目录批量 fixture 比对。

## 5. 测试与样例
- 代码内置单测覆盖：
  - TOML 解析
  - enter/exit
  - count/sum 聚合
- 仓库示例 fixture：
  - `tests/fixtures/static_rule/sample_rule.toml`
  - `tests/fixtures/static_rule/sample_fixture.log`
  - `tests/fixtures/static_rule/sample_expected.txt`

## 6. 当前边界（MVP）
- 不提供脚本逃生舱（如 Lua）
- 正则表达能力受限于 `regex` crate
- 聚合仅支持 count/sum 两类

后续可扩展方向：
- avg/min/max
- 分组聚合
- 更丰富模板函数
