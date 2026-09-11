//! Protobuf 插件类型定义。

/// Protobuf 编译输出（protoc / .proto）压缩插件，识别编译警告与错误行并提取关键诊断。
pub struct ProtobufPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
