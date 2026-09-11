# 插件名称：WebpackVitePlugin

## 1. 模块概述
针对前端开发环境设计的日志压缩插件。处理 Webpack 资产列表、Vite 依赖分析及 HMR 日志。

## 2. 功能点清单
### 2.1 资产路径优化 (Asset Path Optimization)
- **功能描述**: 将 `[name].[contenthash].js` 生成的巨量离散文件名提取为模式规则，存储在字典中。

### 2.2 依赖树折叠
- **功能描述**: 压缩构建失败时输出的海量依赖关系树（Dependency Tree），保留错误路径。
