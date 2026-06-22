#[cfg(feature = "candle-core")]
pub mod methods;
#[cfg(feature = "candle-core")]
pub mod types;

#[cfg(feature = "candle-core")]
pub use types::*;

// Dummy implementation when feature is disabled
#[cfg(not(feature = "candle-core"))]
pub struct SemanticClassifier;

#[cfg(not(feature = "candle-core"))]
impl SemanticClassifier {
    pub fn new() -> Option<Self> {
        None
    }
    pub fn classify(&self, _text: &str) -> Option<String> {
        None
    }
}
