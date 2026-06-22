#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantConfig {
    pub name: String,
    pub detect: VariantDetect,
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantDetect {
    File { exists: String },
    ArgsPattern { pattern: String },
    OutputPattern { pattern: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFilter {
    Vitest,
    Jest,
    Mocha,
}

impl VariantFilter {
    pub fn as_filter_name(&self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::Jest => "jest",
            Self::Mocha => "mocha",
        }
    }
}
