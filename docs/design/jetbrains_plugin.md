# JetBrains 插件 (JetBrains Plugin)

## 1. 模块职责

TokenSlim JetBrains 插件为 IntelliJ IDEA、WebStorm、PyCharm 等 JetBrains IDE 提供一键压缩能力。通过 HTTP 与本地 `tokenslim-server` Sidecar 服务通信。

## 2. 技术栈

| 项目 | 值 |
|------|-----|
| 语言 | Kotlin |
| 构建工具 | Gradle |
| 通信协议 | HTTP REST（127.0.0.1:10086） |
| 最低 Java 版本 | 11 |

## 3. 核心架构

```
JetBrains Plugin (Kotlin)
  │
  ├── actions/
  │   ├── CompressAction.kt       # 压缩操作（AnAction）
  │   └── RestartServerAction.kt  # 重启服务器操作
  │
  ├── TokenSlimServerManager.kt   # 服务器生命周期管理
  └── TokenSlimClient.kt          # HTTP 客户端封装
```

## 4. 核心类

### CompressAction (AnAction)
继承 `AnAction`，注册到 IDE 菜单。执行逻辑：
1. 获取当前编辑器选中的文本（如有选中），否则获取整个文档
2. 调用 `TokenSlimServerManager.ensureServerRunning()` 确保服务器在线
3. 通过 `TokenSlimClient.compress()` 发送压缩请求
4. 将返回的 JSON 结果在 `LightVirtualFile` 中展示
5. 通过 `NotificationGroupManager` 显示成功/失败通知

### RestartServerAction (AnAction)
重启 Sidecar 服务的快捷操作。

### TokenSlimServerManager (Object/Singleton)
管理 `tokenslim-server` 进程生命周期：
- `ensureServerRunning(project)`: 先通过 `/health` 检查，未运行则启动
- `startServer(project)`: 在项目根目录 `target/release/` 下查找二进制并启动
- `stopServer()`: 销毁进程

### TokenSlimClient
HTTP 客户端封装，基于 Java 11 `HttpClient`：
- `checkHealth()`: GET `/health`，返回 `CompletableFuture<Boolean>`
- `compress(text)`: POST `/compress`，返回 `CompletableFuture<String>`

## 5. 数据流

```
用户触发 CompressAction
  │
  ▼
获取选中文本 / 全文
  │
  ▼
TokenSlimServerManager.ensureServerRunning()
  │
  ▼
TokenSlimClient.compress(text)
  │
  ▼
POST /compress {"text": "..."}
  │
  ▼
LightVirtualFile 展示 JSON 结果
  │
  ▼
NotificationGroupManager 通知
```

## 6. 构建与安装

```bash
cd jetbrains-plugin
./gradlew buildPlugin
# 将生成的 .zip 拖入 IDE 安装
```

---

*最后更新：2026-05-13（基于源码 `jetbrains-plugin/src/` 同步）*