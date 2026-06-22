//! Protobuf 插件类型定义。

pub struct ProtobufPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
