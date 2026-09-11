# 插件名称：UnityUnrealPlugin

## 1. 模块概述
针对游戏引擎（Unity & Unreal Engine）设计，处理海量资源（Assets）加载日志、着色器编译日志及序列化引用。

## 2. 功能点清单
### 2.1 游戏资源路径分层字典
- **功能描述**: 精准识别 `Assets/`, `Packages/` (Unity) 和 `/Game/Content/` (Unreal) 路径。
- **策略**: 采用分段字典化，如 `Assets/Textures/Environmental/` 被拆分为多级 Token。

### 2.2 构建报告精简
- **功能描述**: 处理庞大的 Build Report，仅保留总评分、耗时及显著异常。
