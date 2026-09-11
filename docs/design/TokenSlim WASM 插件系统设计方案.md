# TokenSlim WASM 插件系统设计方案

## 1. 架构概述

### 1.1 设计目标

- **动态加载**：运行时扫描并加载插件，无需重新编译主程序
- **安全隔离**：WASM 沙箱环境，插件崩溃不影响主程序
- **版本兼容**：插件与主程序独立版本管理
- **跨平台**：一次编译，到处运行
- **热插拔**：支持运行时加载/卸载插件

### 1.2 技术选型

```yaml
运行时：wasmtime v15.0+
WASM 目标：wasm32-wasi
接口定义：WIT (WebAssembly Interface Types)
序列化：serde + bincode
异步支持：wasmtime-async
```

---

## 2. 项目结构

```
TokenSlim/
├── src/
│   ├── main.rs                  # 主程序入口
│   ├── plugin_host.rs           # WASM 主机实现
│   └── plugin_api.rs            # 插件 API 定义
├── plugins/                     # 插件目录
│   ├── plugin-sdk/              # 插件开发 SDK
│   │   ├── src/
│   │   │   └── lib.rs           # SDK 接口
│   │   ├── Cargo.toml
│   │   └── README.md
│   ├── android_gradle_plugin/   # 示例插件 1
│   │   ├── src/
│   │   │   └── lib.rs
│   │   ├── Cargo.toml
│   │   └── plugin.toml          # 插件元数据
│   ├── gcc_log_plugin/          # 示例插件 2
│   │   └── ...
│   └── mod.rs                   # 插件注册表
├── wasmtime/                    # WASM 运行时配置
│   └── config.toml
├── Cargo.toml
└── build.rs                     # 构建脚本
```

---

## 3. 核心实现

### 3.1 插件接口定义 (WIT)

```wit
// plugins/plugin-sdk/wit/plugin.wit

package tokenslim:plugin;

interface plugin-metadata {
    record metadata {
        name: string,
        version: string,
        description: string,
        author: string,
    }
}

interface plugin-interface {
    use plugin-metadata.{metadata};
    
    plugin: interface {
        get-metadata: func() -> metadata;
        process: func(input: string) -> string;
        init: func() -> result<(), string>;
        shutdown: func() -> result<(), string>;
    }
}

world plugin-world {
    export plugin-interface;
}
```

### 3.2 插件 SDK

```rust
// plugins/plugin-sdk/src/lib.rs

use serde::{Deserialize, Serialize};

/// 插件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// 插件 Trait（主机端调用接口）
pub trait Plugin {
    fn get_metadata(&self) -> PluginMetadata;
    fn process(&self, input: &str) -> String;
    fn init(&mut self) -> Result<(), String>;
    fn shutdown(&mut self) -> Result<(), String>;
}

/// 插件宏（简化插件开发）
#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn _plugin_create() -> *mut dyn $crate::Plugin {
            Box::into_raw(Box::new(<$plugin_type>::default()))
        }

        #[no_mangle]
        pub extern "C" fn _plugin_destroy(ptr: *mut dyn $crate::Plugin) {
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    };
}
```

### 3.3 插件主机加载器

```rust
// src/plugin_host.rs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use wasmtime::*;
use wasmtime_wasi::*;

/// 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
    pub config: HashMap<String, String>,
}

/// 已加载的插件
pub struct LoadedPlugin {
    pub instance: Instance,
    pub store: Store<WasiCtx>,
    pub config: PluginConfig,
    pub path: PathBuf,
}

/// WASM 插件主机
pub struct PluginHost {
    engine: Engine,
    linker: Linker<WasiCtx>,
    plugins: HashMap<String, LoadedPlugin>,
}

impl PluginHost {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        
        // 注册 WASI API
        wasmtime_wasi::add_to_linker_async(&mut linker)?;
        
        Ok(Self {
            engine,
            linker,
            plugins: HashMap::new(),
        })
    }

    /// 扫描并加载插件目录
    pub async fn scan_plugins(&mut self, plugins_dir: &Path) -> Result<()> {
        if !plugins_dir.exists() {
            fs::create_dir_all(plugins_dir)?;
            return Ok(());
        }

        for entry in fs::read_dir(plugins_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                match self.load_plugin(&path).await {
                    Ok(_) => println!("✓ 加载插件：{}", path.display()),
                    Err(e) => eprintln!("✗ 加载失败 {:?}: {}", path, e),
                }
            }
        }
        
        Ok(())
    }

    /// 加载单个插件
    pub async fn load_plugin(&mut self, path: &Path) -> Result<()> {
        // 创建 WASI 上下文
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        builder.env("PLUGIN_PATH", path.to_str().unwrap())?;
        
        let wasi = builder.build();
        let mut store = Store::new(&self.engine, wasi);
        
        // 加载 WASM 模块
        let module = Module::from_file(&self.engine, path)
            .with_context(|| format!("加载模块失败：{}", path.display()))?;
        
        // 创建实例
        let instance = self.linker
            .instantiate_async(&mut store, &module)
            .await?;
        
        // 获取插件元数据
        let metadata_fn = instance.get_typed_func::<(), String>(&mut store, "get_metadata")?;
        let metadata = metadata_fn.call_async(&mut store, ()).await?;
        
        // 加载配置
        let config_path = path.with_extension("toml");
        let config = self.load_plugin_config(&config_path)?;
        
        // 初始化插件
        if let Ok(init_fn) = instance.get_typed_func::<(), Result<(), String>>(&mut store, "init") {
            init_fn.call_async(&mut store, ()).await??;
        }
        
        self.plugins.insert(metadata.clone(), LoadedPlugin {
            instance,
            store,
            config,
            path: path.to_path_buf(),
        });
        
        Ok(())
    }

    /// 卸载插件
    pub fn unload_plugin(&mut self, name: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.remove(name) {
            // 调用 shutdown 钩子
            if let Ok(shutdown_fn) = plugin.instance.get_typed_func::<(), Result<(), String>>(&plugin.store, "shutdown") {
                let _ = shutdown_fn.call(plugin.store, ());
            }
            println!("✓ 卸载插件：{}", name);
        }
        Ok(())
    }

    /// 处理输入（调用所有插件）
    pub async fn process_all(&mut self, input: &str) -> Result<String> {
        let mut result = input.to_string();
        
        for (name, plugin) in &mut self.plugins {
            if !plugin.config.enabled {
                continue;
            }
            
            match plugin.process(&result).await {
                Ok(processed) => result = processed,
                Err(e) => eprintln!("插件 {} 处理失败：{}", name, e),
            }
        }
        
        Ok(result)
    }

    /// 调用插件的 process 函数
    async fn process(&mut self, input: &str) -> Result<String> {
        let process_fn = self.instance.get_typed_func::<String, String>(&mut self.store, "process")?;
        let result = process_fn.call_async(&mut self.store, input.to_string()).await?;
        Ok(result)
    }

    fn load_plugin_config(&self, path: &Path) -> Result<PluginConfig> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: PluginConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(PluginConfig {
                name: "unknown".to_string(),
                enabled: true,
                priority: 100,
                config: HashMap::new(),
            })
        }
    }

    /// 列出已加载的插件
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }
}
```

### 3.4 示例插件实现

```rust
// plugins/android_gradle_plugin/src/lib.rs

use plugin_sdk::{Plugin, PluginMetadata, register_plugin};

#[derive(Default)]
pub struct AndroidGradlePlugin {
    metadata: PluginMetadata,
}

impl Plugin for AndroidGradlePlugin {
    fn get_metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "android-gradle".to_string(),
            version: "0.1.0".to_string(),
            description: "Android Gradle 日志脱水插件".to_string(),
            author: "TokenSlim Team".to_string(),
        }
    }

    fn process(&self, input: &str) -> String {
        // 实现 Android Gradle 日志脱水逻辑
        let result = input
            .lines()
            .filter(|line| !line.contains("BUILD SUCCESSFUL"))
            .map(|line| {
                // 移除时间戳和路径信息
                line.replace(&std::env::current_dir().unwrap().to_string_lossy(), "$PROJECT")
            })
            .collect::<Vec<_>>()
            .join("\n");
        
        result
    }

    fn init(&mut self) -> Result<(), String> {
        println!("Android Gradle 插件初始化");
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        println!("Android Gradle 插件关闭");
        Ok(())
    }
}

// 注册插件
register_plugin!(AndroidGradlePlugin);
```

### 3.5 插件配置文件

```toml
# plugins/android_gradle_plugin/plugin.toml

name = "android-gradle"
enabled = true
priority = 100

[config]
log_level = "info"
max_lines = 1000
strip_timestamps = true
strip_paths = true
```

### 3.6 主程序入口

```rust
// src/main.rs

mod plugin_host;

use anyhow::Result;
use plugin_host::PluginHost;
use std::path::Path;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 TokenSlim 启动...");
    
    // 创建插件主机
    let mut host = PluginHost::new()?;
    
    // 扫描并加载插件
    let plugins_dir = Path::new("plugins");
    host.scan_plugins(plugins_dir).await?;
    
    println!("已加载插件：{:?}", host.list_plugins());
    
    // 示例：处理输入
    let input = r#"
        BUILD SUCCESSFUL in 5s
        /home/user/project/app/build.gradle
        Task :app:compileDebugJavaWithJavac
    "#;
    
    let result = host.process_all(input).await?;
    println!("处理结果:\n{}", result);
    
    Ok(())
}
```

---

## 4. 构建与部署

### 4.1 Cargo.toml 配置

```toml
# 主程序 Cargo.toml
[package]
name = "tokenslim"
version = "0.1.0"
edition = "2021"

[dependencies]
wasmtime = "15.0"
wasmtime-wasi = "15.0"
anyhow = "1.0"
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

# 插件 SDK Cargo.toml
[package]
name = "plugin-sdk"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["rlib"]

[dependencies]
serde = { version = "1.0", features = ["derive"] }
```

### 4.2 构建脚本

```rust
// build.rs

use std::process::Command;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=plugins/");
    
    // 构建所有插件
    let plugins_dir = Path::new("plugins");
    
    for entry in std::fs::read_dir(plugins_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.join("Cargo.toml").exists() {
            let plugin_name = path.file_name().unwrap().to_str().unwrap();
            
            println!("cargo:warning=构建插件：{}", plugin_name);
            
            // 编译为 WASM
            let status = Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("--target")
                .arg("wasm32-wasi")
                .current_dir(&path)
                .status()
                .unwrap();
            
            if !status.success() {
                panic!("插件 {} 构建失败", plugin_name);
            }
            
            // 复制 WASM 文件到输出目录
            let wasm_path = path.join("target/wasm32-wasi/release")
                .join(format!("{}.wasm", plugin_name.replace("-", "_")));
            
            let output_path = Path::new("target/plugins")
                .join(format!("{}.wasm", plugin_name));
            
            std::fs::create_dir_all(output_path.parent().unwrap()).unwrap();
            std::fs::copy(&wasm_path, &output_path).unwrap();
        }
    }
}
```

### 4.3 一键构建脚本

```bash
#!/bin/bash
# build_plugins.sh

set -e

echo "🔨 构建所有插件..."

# 添加 WASM 目标
rustup target add wasm32-wasi

# 构建插件
for plugin in plugins/*/; do
    if [ -f "$plugin/Cargo.toml" ]; then
        plugin_name=$(basename "$plugin")
        echo "构建插件：$plugin_name"
        
        cd "$plugin"
        cargo build --release --target wasm32-wasi
        
        # 复制 WASM 文件
        cp target/wasm32-wasi/release/${plugin_name//-/_}.wasm ../../target/plugins/
        cd ../..
    fi
done

echo "✅ 插件构建完成"
```

---

## 5. 高级特性

### 5.1 插件热重载

```rust
// src/hot_reload.rs

use notify::{Event, RecursiveMode, Watcher};
use std::sync::mpsc::channel;
use std::time::Duration;

pub struct HotReloader {
    watcher: Watcher,
    plugins_dir: PathBuf,
}

impl HotReloader {
    pub fn new(plugins_dir: PathBuf) -> Result<Self> {
        let (tx, rx) = channel();
        
        let mut watcher = Watcher::new(tx, Duration::from_secs(2))?;
        watcher.watch(&plugins_dir, RecursiveMode::Recursive)?;
        
        Ok(Self { watcher, plugins_dir })
    }

    pub async fn watch(&mut self, host: &mut PluginHost) -> Result<()> {
        loop {
            match self.rx.recv() {
                Ok(Event { path, .. }) => {
                    if let Some(path) = path {
                        if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                            println!("检测到插件变更：{}", path.display());
                            
                            // 卸载旧插件
                            let name = path.file_stem().unwrap().to_str().unwrap();
                            let _ = host.unload_plugin(name);
                            
                            // 加载新插件
                            let _ = host.load_plugin(&path).await;
                        }
                    }
                }
                Err(e) => eprintln!("监控错误：{:?}", e),
            }
        }
    }
}
```

### 5.2 插件间通信

```rust
// src/plugin_bus.rs

use tokio::sync::broadcast;

pub struct PluginMessageBus {
    tx: broadcast::Sender<Message>,
}

impl PluginMessageBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.tx.subscribe()
    }

    pub fn broadcast(&self, msg: Message) {
        let _ = self.tx.send(msg);
    }
}

pub enum Message {
    Log(String),
    Metric { name: String, value: f64 },
    Event { name: String, data: serde_json::Value },
}
```

---

## 6. 测试示例

```rust
// tests/plugin_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_plugin_load() {
        let mut host = PluginHost::new().unwrap();
        host.load_plugin("target/plugins/android_gradle.wasm")
            .await
            .unwrap();
        
        assert!(host.list_plugins().contains(&"android-gradle"));
    }

    #[tokio::test]
    async fn test_plugin_process() {
        let mut host = PluginHost::new().unwrap();
        host.load_plugin("target/plugins/android_gradle.wasm")
            .await
            .unwrap();
        
        let input = "BUILD SUCCESSFUL in 5s";
        let result = host.process_all(input).await.unwrap();
        
        assert!(!result.contains("BUILD SUCCESSFUL"));
    }
}
```

---

## 7. 性能优化建议

### 7.1 资源限制

```rust
// 限制插件内存使用
let mut store = Store::new(&engine, wasi);
store.limiter(|_| {
    struct Limiter;
    impl ResourceLimiter for Limiter {
        fn memory_growing(&mut self, current: usize, desired: usize, _maximum: Option<usize>) -> bool {
            desired <= 64 * 1024 * 1024 // 64MB
        }
    }
});
```

### 7.2 实例池化

```rust
use wasmtime::PoolingAllocationStrategy;

let mut config = Config::new();
config.allocation_strategy(PoolingAllocationStrategy::Reuse);
```

---

## 8. 总结

### 优势
✅ 安全隔离（插件崩溃不影响主程序）  
✅ 跨平台（WASM 字节码）  
✅ 语言无关（可用 Rust/C++/AssemblyScript 编写插件）  
✅ 版本兼容（ABI 稳定）  
✅ 热插拔（支持运行时加载/卸载）

### 劣势
⚠️ 性能开销（约 10-20%）  
⚠️ 实现复杂度较高  
⚠️ WASM 模块体积较大

### 适用场景
- 需要第三方插件扩展
- 多租户隔离需求
- 跨平台部署需求
- 安全性要求高的场景

---

**下一步行动：**

1. 创建 `plugin-sdk` crate
2. 实现第一个示例插件
3. 编写构建脚本
4. 集成到 TokenSlim 主程序
5. 编写测试用例