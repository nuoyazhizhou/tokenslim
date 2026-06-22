use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JavaStackConfig {
    pub fold_external_frames: bool,
}

pub struct JavaStackPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: JavaStackConfig,
}
