# TokenSlim Workspace 诊断生态支持矩阵 (Ecosystem Support Matrix)

> 目标：为 LLM 提供精准的代码生成和排障上下文，防止出现“在 React 项目里写 Vue 语法”、“在 Windows 上生成 `grep` 命令”、“不知道是 iOS 还是 Android 导致编译命令猜错”等问题。

---

## 1. 当前已支持 (Currently Supported) ✅

目前的 `workspace` 诊断已具备跨平台的基本骨架，覆盖了主流的通用开发场景。

### 1.1 操作系统与终端 (OS & Shell)
*   **OS**: Windows, macOS, Linux (及各类发行版，提取内核与版本)
*   **Shell**: PowerShell, CMD, Bash, Zsh, Fish
*   **编码**: Codepage (Windows), UTF-8 链路探测

### 1.2 开发语言与构建工具 (Languages & Toolchains)
*   **Rust**: `Cargo.toml` -> `cargo build/test`
*   **Java**: `pom.xml`, `build.gradle` -> `mvn`, `./gradlew`
*   **C# / .NET**: `*.sln`, `*.csproj` -> `dotnet build/test`
*   **Node.js**: `package.json` -> `npm run build/test` (泛 JS/TS)
*   **Python**: `pyproject.toml`, `requirements.txt`, `setup.py` -> `pip`, `pytest`
*   **C/C++**: `CMakeLists.txt`, `Makefile` -> `cmake`, `make`
*   **Go**: `go.mod` -> `go build/test`
*   **Ruby**: `Gemfile` -> `bundle exec`
*   **PHP**: `composer.json` -> `composer`
*   **Dart**: `pubspec.yaml` -> `dart pub`

### 1.3 编辑器与 IDE 生态 (IDE Ecosystem)
*   **VSCode**: `.vscode/`
*   **IntelliJ / IDEA**: `.idea/`, `*.iml`
*   **Visual Studio**: `.vs/`, `*.sln`

---

## 2. 缺失的全局生态全景 (The Comprehensive Missing Picture) ❌

为了做到真正的全栈覆盖，我们需要针对目前尚未细分的生态进行支持。以下是主要操作系统、计算机语言、框架、编译器、SDK、平台和 IDE 的全景清单及 TokenSlim 的缺失状态：

### 2.1 平台与操作系统 (Platforms & OS)
*   **Mobile / 移动端**:
    *   *缺失*: **iOS** (需要识别 `.xcodeproj`, `Info.plist`), **Android** (需要识别 `AndroidManifest.xml`, `app/build.gradle`)
*   **嵌入式与 RTOS**:
    *   *缺失*: Arduino, Raspberry Pi, FreeRTOS (需要识别 `platformio.ini`, 专用 Makefile)
*   **云原生与容器 (Cloud & Containers)**:
    *   *已支持 (Phase 3)*: 探测 `docker-compose.yml` (`docker-compose-present`)

### 2.2 计算机语言 (Languages)
*   *已支持*: Rust, Java, C#, Node(JS/TS), Python, C/C++, Go, Ruby, PHP, Dart, **Swift**, **Kotlin**
*   *主要缺失*:
    *   **Scala** (与 Java 区分)
    *   **Elixir** (`mix.exs`)
    *   **Haskell** (`stack.yaml`, `*.cabal`)
    *   **Lua** (`Rockspec`)
    *   **Zig** (`build.zig`)

### 2.3 核心框架 (Frameworks)
*   **前端生态 (Phase 1 已加入细分)**:
    *   **React 体系**: Next.js (`next.config.js`)
    *   **Vue 体系**: Nuxt.js (`nuxt.config.js`), Vite/Vue (`vite.config.js`)
    *   **Svelte**: SvelteKit (`svelte.config.js`)
    *   **Angular**: `angular.json`
*   **后端生态 (Phase 1 已加入细分)**:
    *   **Java**: Spring Boot (`pom.xml` 内含 `spring-boot`)
    *   **Python**: Django (`manage.py`)
    *   **PHP**: Laravel (`artisan`)
    *   **Ruby**: Ruby on Rails (`bin/rails`)
*   **跨平台/原生 UI (Cross-Platform/Native UI)**:
    *   *已支持 (Phase 2)*: Flutter (`pubspec.yaml` 包含 sdk: flutter)
    *   *缺失*: React Native, MAUI/Xamarin, Tauri

### 2.4 主要编译器与底层链 (Compilers & Low-level Toolchains)
*   *缺失探测*:
    *   **GCC / G++**
    *   **Clang / LLVM**
    *   **MSVC** (Windows 底层编译)
    *   **Ninja** (构建系统)
    *   **Bazel** (C++ 现代构建)

### 2.5 核心 SDK 与运行时 (SDKs & Runtimes)
*   *已支持探测*: Node.js, Python, JDK, Rustc
*   *缺失探测*:
    *   **Android SDK / NDK** (`ANDROID_HOME`, `local.properties`)
    *   **Xcode Command Line Tools** (`xcode-select -p`)
    *   **.NET SDK** (细分版本号)
    *   **Bun / Deno** (替代 Node.js)

### 2.6 编辑器与 IDE (IDEs)
*   *已支持*: VSCode, IDEA, Visual Studio
*   *缺失*:
    *   **Xcode**: `.xcworkspace`
    *   **Android Studio**: 特征同 IDEA，但结合 Android 结构
    *   **Eclipse**: `.project`, `.classpath`
    *   **Neovim / Vim**: `init.lua`, `.nvim/` (虽然较少存留在项目中)
    *   **AI Native IDEs**: Cursor (`.cursor/`, `.cursorrules`), Trae, Windsurf

---

## 3. 为什么这些重要？(Why this matters for LLM?)

如果只告诉 LLM `proj: node`：
- LLM 会盲目使用 `npm`，即使用户仓库里有 `pnpm-lock.yaml`。
- LLM 会在 Next.js (React) 项目里给出 Vue 的组件写法。

如果只告诉 LLM `proj: java`：
- LLM 不知道这是普通的 Java 库，还是 Android App (需要使用 `android.util.Log` 而不是 `System.out.println`)。

通过补齐这份全景矩阵的检测（Phase 1/2/3 **均已在本次迭代完成**），TokenSlim 成为了真正“智能”的上下文拦截器，为 Agent 提供真正免配置、零错误的运行时语境。
