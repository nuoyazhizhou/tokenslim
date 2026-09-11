# Yaml Plugin 模块设计

## 职责
专门用于处理大型 YAML 结构文本的压缩。特别针对 Kubernetes 的部署配置、Docker Compose 文件或者 OpenAPI 规范。这些文件通常具有大量的同质化 Key，并且依靠空白缩进来维持层级。

## 策略
- 对文本进行 `serde_yaml::from_str` 解析测试，或者利用正则侦测。
- 与 JSON 类似，解析成 `Value` 后，我们在输出的时候保留缩进层级（或转化为极简的表示），并将高频的 YAML Key 转化为短 Token `$YK1`。
- YAML 结构对空格极其敏感，压缩方案：将 YAML 转换成最小化 JSON 发送到大模型？不，模型可能期待 YAML 格式以输出 YAML。所以我们必须输出脱水后的 YAML 字符串。

## 设计
- **detect**: 利用 `serde_yaml` 判断 Slice 是否为有效 YAML 且具有足够深度。
- **compress**: 使用与 `JSON` 插件相同的 AST 字典化递归，将 Key 替换为 `$Mxxx`，最后使用 `serde_yaml::to_string` 输出紧凑带 Token 的 YAML。
