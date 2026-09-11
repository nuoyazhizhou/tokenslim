/// Java/Kotlin/Android 异常堆栈压缩插件主体：去重重复堆栈、截断深层帧、折叠 Suppressed、保留异常类名字面量。
pub struct JavaStackPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
