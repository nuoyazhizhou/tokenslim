#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFilter {
    Vitest,
    Jest,
    Mocha,
}

impl VariantFilter {
    /// 返回变体过滤器对应的框架名字符串（vitest / jest / mocha）。
    pub fn as_filter_name(&self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::Jest => "jest",
            Self::Mocha => "mocha",
        }
    }
}
