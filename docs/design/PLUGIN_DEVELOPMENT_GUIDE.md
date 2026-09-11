# TokenSlim 插件开发指南

> 状态校准（2026-05-13）  
> 当前插件开发以 `src/plugins/<plugin>_plugin/` 静态插件为主，动态插件仅作为可选包装层。新增或增强插件必须同步完成 samples、showcase、tests、审计快照、case 镜像和冻结状态。  
> 审计总览当前为：VCS `328/328` frozen，non-VCS `484/484` frozen。

## 当前强制流程

1. 在 `samples/<plugin>_plugin/` 添加真实样本，禁止在测试中手写长字符串。
2. 更新 `src/plugins/<plugin>_plugin/showcase.rs`，通常至少 12 个 case；窄插件需说明豁免理由。
3. 更新 `src/plugins/<plugin>_plugin/test.rs`，覆盖 detect、compress、ROI 和关键语义。
4. 在 `methods.rs` 中实现逻辑，压缩入口必须走 `prefer_non_expanding(raw, compacted)`。
5. 如影响 `tokenslim run`，同步更新 `config/plugins/*.route.json` 和 CLI 路由测试。
6. 生成 `target/*_compact_showcase_report.txt`。
7. 执行 `scripts/audit_case_metrics.ps1`，确认 `regressed=0`、`frozen_changed=0`。
8. 使用 `-RequireSemanticGate` 冻结通过的 case。
9. 同步更新 `docs/plans/`、`docs/reports/`、`docs/audit/`、`README.md` 和 `DOCS_ORGANIZATION.md`。

## 📖 概述

本指南介绍如何为 TokenSlim 开发和集成插件，包括静态链接插件和动态加载插件两种方式。

---

## 🏗️ 架构现状

### 已完成的功 (✅)

✅ **plugin-interface crate** - 定义了 `PLUGIN_API_VERSION = 1`  
✅ **动态插件加载器** - `src/core/dynamic_plugin_loader/mod.rs` (功能完整)  
✅ **配置文件支持** - `config/plugins.toml` 支持 static/dynamic_plugins 配置  
✅ **FFI 包装层** - 完整的 FFI 导出和调用机制  
✅ **动态插件加载已接入 CLI** - 支持动态优先 + 静态回退  
✅ **目录冲突收敛** - 核心实现统一保留在 `src/plugins/`，不再双份维护  

### 编译验证

```bash
# 主程序
tokenslim run cargo build -p tokenslim
# ✅ 编译成功 (11.85s)

# 动态库
tokenslim run cargo build -p gcc_log_plugin -p smart_path_plugin\
            -p java_stack_plugin -p python_traceback_plugin
# ✅ 4 个插件全部成功生成 .dll 文件
```

---

## 🎯 插件类型

### 1. 静态插件（推荐用于核心功能）

**特点**：
- 编译进主程序，零加载开销
- 直接访问内部 API
- 适合高频使用的核心插件

**位置**: `src/plugins/`

**示例插件**:
- `smart_path` - 智能路径处理
- `ansi_cleaner` - ANSI 转义码清理
- `json`, `yaml`, `xml_html` - 格式处理
- `gcc_log`, `java_stack`, `nodejs` - 日志处理

### 2. 动态插件（推荐用于扩展功能）

**特点**：
- 运行时从 `plugins/` 目录加载 `.dll`/`.so`/`.dylib`
- 无需重新编译主程序即可添加/更新
- 通过 C ABI 导出函数进行通信

**位置**: `plugins/`

**当前动态插件（按需保留）**:
- ✅ `db_log_plugin`
- ✅ `syslog_plugin`
- ✅ `xcode_log_plugin`
- ⏳ `rust_go_plugin` / `web_log_plugin`（可选动态化）

---

## 🚀 开发静态插件

### 步骤 1: 创建插件模块

在 `src/plugins/` 下创建新目录，例如 `my_plugin/`:

```
src/plugins/my_plugin/
├── mod.rs      # 模块声明
├── types.rs    # 类型定义
├── methods.rs  # Plugin trait 实现
└── test.rs     # 测试
```

### 步骤 2: 定义插件结构

```rust
// src/plugins/my_plugin/types.rs
use std::sync::Arc;
use regex::Regex;
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::Slice;
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::dedup_engine::DedupEngine;
use crate::core::compression::{CompressResult, Token};

pub struct MyPlugin {
   name: &'static str,
   priority: u8,
    // 其他字段...
}
```

### 步骤 3: 实现 Plugin Trait

```rust
// src/plugins/my_plugin/methods.rs
impl MyPlugin {
   pub fn new() -> Self {
        Self {
           name: "my_plugin",
           priority: 100,
            // ...
        }
    }
}

impl Default for MyPlugin {
   fn default() -> Self {
        Self::new()
    }
}

impl Plugin for MyPlugin {
   fn name(&self) -> &'static str {
        self.name
    }

   fn priority(&self) -> u8 {
        self.priority
    }

   fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        // 检测逻辑
       let text = slice.text.as_ref();
        if text.contains("my_pattern") {
            Some(0.8)
        } else {
            None
        }
    }

   fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
    ) -> CompressResult {
        // 压缩逻辑
       CompressResult {
            tokens: vec![Token::Text(slice.text.to_string())],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }

   fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        // 还原逻辑
       compressed.to_string()
    }

   fn next_plugins(&self) -> Vec<&'static str> {
        vec![] // 或指定后续插件
    }
}
```

### 步骤 4: 注册插件

在 `src/cli/methods.rs` 的 `get_plugins()` 函数中添加：

```rust
use crate::plugins::my_plugin::MyPlugin;

fn get_plugins() -> Vec<Box<dyn Plugin>> {
   let mut plugins = Vec::new();
    
    // ... 其他插件
    
    plugins.push(Box::new(MyPlugin::new()) as Box<dyn Plugin>);
    
    plugins
}
```

---

## 🔌 开发动态插件

### 步骤 1: 创建独立 crate

在 `plugins/` 下创建新目录：

```
plugins/my_plugin/
├── Cargo.toml
└── src/
    └── lib.rs
```

### 步骤 2: 配置 Cargo.toml

```toml
[package]
name = "my_plugin"
version= "0.1.0"
edition= "2021"

[lib]
crate-type = ["cdylib"]  # 编译为动态库
name = "my_plugin"

[dependencies]
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
log.workspace = true
plugin-interface.workspace = true

# 引用主项目获取 Plugin trait 和类型
tokenslim = { path = "../..", default-features = false }
```

### 步骤 3: 实现插件并导出 FFI 函数

```rust
// plugins/my_plugin/src/lib.rs
use regex::Regex;
use std::ffi::{c_void, c_char, CStr, CString};
use tokenslim::core::plugin_dispatcher::{CompressResult, Plugin};
use tokenslim::core::text_slicer::Slice;
use tokenslim::core::dictionary_engine::{Dictionary, DictionaryEngine};
use tokenslim::core::dedup_engine::DedupEngine;

/// 我的插件
pub struct MyPlugin {
   name: &'static str,
   priority: u8,
}

impl MyPlugin {
   pub fn new() -> Self {
        Self {
           name: "my_plugin",
           priority: 100,
        }
    }
}

impl Default for MyPlugin {
   fn default() -> Self {
        Self::new()
    }
}

impl Plugin for MyPlugin {
   fn name(&self) -> &'static str { self.name }
   fn priority(&self) -> u8 { self.priority }
    
   fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        Some(0.8)
    }
    
   fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
    ) -> CompressResult {
       CompressResult {
            tokens: vec![],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }
    
   fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
       compressed.to_string()
    }
}

// ============================================================================
// FFI 导出函数 - 必须全部实现
// ============================================================================

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut c_void {
   let plugin = MyPlugin::new();
   let boxed = Box::new(plugin);
    Box::into_raw(boxed) as *mut _
}

#[no_mangle]
pub extern "C" fn destroy_plugin(ptr: *mut c_void) {
    unsafe {
       let _ = Box::from_raw(ptr as *mut MyPlugin);
    }
}

#[no_mangle]
pub extern "C" fn plugin_interface_version() -> u32 {
    plugin_interface::PLUGIN_API_VERSION
}

#[no_mangle]
pub extern "C" fn plugin_name(ptr: *const c_void) -> *mut c_char {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
       let name = plugin.name();
        CString::new(name).unwrap().into_raw()
    }
}

#[no_mangle]
pub extern "C" fn plugin_priority(ptr: *const c_void) -> u8 {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
        plugin.priority()
    }
}

#[no_mangle]
pub extern "C" fn plugin_detect(ptr: *const c_void, slice_ptr: *const FFISlice) -> f32 {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
       let ffi_slice = &*slice_ptr;
       let slice = ffi_slice.to_slice();
        
        match plugin.detect(&slice) {
            Some(confidence) => confidence,
            None => -1.0,
        }
    }
}

#[no_mangle]
pub extern "C" fn plugin_compress(
    ptr: *const c_void,
    slice_ptr: *const FFISlice,
    dict_ptr: *mut DictionaryEngine,
    dedup_ptr: *mut DedupEngine,
) -> *mut CompressResult {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
       let ffi_slice = &*slice_ptr;
       let slice = ffi_slice.to_slice();
       let dict_engine = &mut *dict_ptr;
       let dedup_engine = &mut *dedup_ptr;
        
       let result = plugin.compress(&slice, dict_engine, dedup_engine);
       let boxed = Box::new(result);
        Box::into_raw(boxed)
    }
}

#[no_mangle]
pub extern "C" fn plugin_decompress(
    ptr: *const c_void,
   compressed_ptr: *const c_char,
    dict_ptr: *const Dictionary,
) -> *mut c_char {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
       let compressed = CStr::from_ptr(compressed_ptr).to_str().unwrap_or("");
       let dict = &*dict_ptr;
        
       let result = plugin.decompress(compressed, dict);
        CString::new(result).unwrap().into_raw()
    }
}

#[no_mangle]
pub extern "C" fn plugin_next_plugins(ptr: *const c_void) -> *mut c_char {
    unsafe {
       let plugin = &*(ptr as *const MyPlugin);
       let next = plugin.next_plugins();
       let json= serde_json::to_string(&next).unwrap_or_else(|_| "[]".to_string());
        CString::new(json).unwrap().into_raw()
    }
}

#[no_mangle]
pub extern "C" fn free_c_string(ptr: *mut c_char) {
    unsafe {
        if !ptr.is_null() {
           let _ = CString::from_raw(ptr);
        }
    }
}

#[no_mangle]
pub extern "C" fn free_compress_result(ptr: *mut CompressResult) {
    unsafe {
        if !ptr.is_null() {
           let _ = Box::from_raw(ptr as *mut CompressResult);
        }
    }
}

// ============================================================================
// FFI 辅助类型
// ============================================================================

#[repr(C)]
pub struct FFISlice {
   pub text_ptr: *const c_char,
   pub text_len: usize,
   pub offset: usize,
   pub line_number: usize,
}

impl FFISlice {
   pub fn to_slice(&self) -> Slice<'static> {
       let text = unsafe {
            std::slice::from_raw_parts(self.text_ptr as *const u8, self.text_len)
        };
       let text_str = std::str::from_utf8(text).unwrap_or("");
       let leaked: &'static str = Box::leak(text_str.to_string().into_boxed_str());
        
        Slice {
            text: leaked,
            offset: self.offset,
            line_number: self.line_number,
            label: None,
        }
    }
}
```

### 步骤 4: 编译和部署

```bash
# 编译动态库
tokenslim run cargo build -p my_plugin --release

# Windows: 生成 target/release/my_plugin.dll
# Linux: 生成 target/release/libmy_plugin.so
# macOS: 生成 target/release/libmy_plugin.dylib

# 复制到 plugins 目录
copy target\release\my_plugin.dll plugins\
```

### 步骤 5: 配置启用

编辑 `config/plugins.toml`:

```toml
[plugins]
dynamic_plugins = [
    { name = "my_plugin", file = "my_plugin.dll", enabled = true }
]
```

---

## 📝 FFI 导出检查清单

开发动态插件时，必须导出以下函数：

- [ ] `create_plugin()` - 创建插件实例
- [ ] `destroy_plugin(ptr)` - 销毁插件实例
- [ ] `plugin_interface_version()` - API 版本检查
- [ ] `plugin_name(ptr)` - 获取插件名称
- [ ] `plugin_priority(ptr)` - 获取优先级
- [ ] `plugin_detect(ptr, slice_ptr)` - 检测内容
- [ ] `plugin_compress(ptr, slice_ptr, dict_ptr, dedup_ptr)` - 压缩内容
- [ ] `plugin_decompress(ptr, compressed_ptr, dict_ptr)` - 还原内容
- [ ] `plugin_next_plugins(ptr)` - 获取后续插件
- [ ] `free_c_string(ptr)` - 释放 C 字符串
- [ ] `free_compress_result(ptr)` - 释放压缩结果

---

## ⚠️ 常见问题

### Q1: 编译错误 "type annotations needed"

**解决**: 给 `libloading` 的 `get` 方法添加显式类型参数：

```rust
let create_fn: libloading::Symbol<CreatePluginFn> = unsafe {
    library.get::<CreatePluginFn>(b"create_plugin")?
}.into_raw();
```

### Q2: 生命周期错误

**解决**: 使用 `Box::leak()` 将 String 转换为 `'static` 引用：

```rust
let leaked: &'static str = Box::leak(my_string.into_boxed_str());
```

### Q3: 内存泄漏

**确保**: 
- 所有 `CString::into_raw()` 都有对应的 `free_c_string()` 调用
- 所有 `Box::into_raw()` 都有对应的 `destroy_plugin()` 调用

---

## ✅ 当前目录策略

### 1. `src/plugins/`（单一事实来源）
**作用**: 存放插件完整业务逻辑（detect/compress/decompress）。  
**要求**: 新功能优先在这里实现，避免多份实现分叉。

### 2. `plugins/`（动态包装层）
**作用**: 仅保留需要运行时动态加载的插件包装 crate。  
**要求**: 尽量薄封装，不复制复杂业务逻辑。

---

## 📚 编译验证

所有迁移的插件都需要通过编译验证：

```bash
# 主程序编译
tokenslim run cargo build -p tokenslim
# ✅ 编译成功 (11.85s)

# 动态库编译（示例）
tokenslim run cargo build -p db_log_plugin -p syslog_plugin -p xcode_log_plugin
# ✅ 生成对应 .dll/.so/.dylib 文件
```

生成的动态库位于 `target/debug/` 目录。

---

## 🔧 剩余可选任务

如需继续迁移其他插件，可参考以下列表：

### 待动态化插件（按需）
- android_gradle_plugin
- ansi_cleaner_plugin
- json_plugin
- yaml_plugin
- xml_html_plugin
- smart_code_plugin
- node_error_plugin
- nodejs_plugin

以上插件当前保留静态实现即可；仅在有热更新或独立发布需求时再添加动态包装。

---

## 💡 总结

本指南涵盖了 TokenSlim 插件开发的完整流程。当前推荐路线是静态插件优先，动态插件只在需要热更新或独立发布时使用：

1. **静态插件开发** - 适合核心高频功能
2. **动态插件开发** - 适合扩展功能，支持热插拔
3. **FFI 导出规范** -11 个必需函数的完整清单
4. **参考实现** - 4 个已成功迁移的插件示例

按照本指南开发的插件必须通过测试、showcase 和审计冻结后，才算真正集成到 TokenSlim 系统中。
   - 按照 gcc_log_plugin 模板迁移其他插件
   - 验证功能一致性

3. **完善 FFI 包装**
   - 测试所有 FFI 调用路径
   - 添加错误处理和日志记录

4. **编写测试**
   - 单元测试：FFI 导出函数
   - 集成测试：动态插件加载
   - 性能测试：静态 vs 动态

---

## 📚 参考资源

- `../../src/plugins/gcc_log_plugin/methods.rs` - gcc_log_plugin 完整实现
- `../../src/core/dynamic_plugin_loader/mod.rs` - DynamicPluginLoader 实现
- `../../config/plugins.toml` - 插件配置文件示例
- `DYNAMIC_PLUGIN_SYSTEM.md` - 动态插件系统设计

---

*最后更新：2026-05-13*
