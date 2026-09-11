# 插件名称：TemplateDrivenPlugin

## 1. 模块概述
`TemplateDrivenPlugin` 是 TokenSlim 的“自适应”核心插件。它不依赖硬编码的正则，而是通过外部 JSON 配置文件加载一系列“模板规则”。该插件通常与 `log_miner` 工具配合使用，实现针对特定项目、特定格式日志的精准压缩。

## 2. 数据结构
- `TemplateRule`: 包含 `pattern`（人类可读模板，含 `<*>`）和 `regex_str`（对应的捕获正则）。
- `TemplateDrivenConfig`: 规则列表。
- `TemplateDrivenPlugin`: 运行时结构，持有关联的正则表达式。

## 3. 功能点清单
### 3.1 模板转正则 (Regex Generation)
- **功能描述**: 将包含 `<*>` 占位符的简洁字符串转换为能够捕获变量的正则表达式。
- **函数签名**: `pub fn build_regex_from_template(template: &[String]) -> String`
- **动态特性**: 自动识别十六进制哈希、数字、版本号等常见模式。

### 3.2 动态配置加载 (Hot-Loading Support)
- **功能描述**: 支持从 `config/template_driven_rules.json` 动态加载规则，无需重编译引擎。
- **调用者**: CLI 启动过程、ID 插件设置。

### 3.3 变量提取与字典化 (Variable Tokenization)
- **功能描述**: 匹配成功后，将正则捕获组提取出来并存入宏字典 (`$M`)。
- **压缩比优势**: 仅传输模板 ID 和变量 Token，极大节省 Token。

## 4. 与其它模块的交互
- **输入**: 接收 `TextSlicer` 产生的 `Slice`。
- **输出**: 将生成的 `DictRef` 提交给 `DictionaryEngine`。

## 5. 待办与注意事项
- 需优化正则生成的安全性，防止“正则回溯攻击 (ReDoS)”。
- 支持 AI 自动纠错功能的集成。
