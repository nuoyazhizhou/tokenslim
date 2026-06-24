#[cfg(feature = "experimental")]
pub mod methods;
#[cfg(feature = "experimental")]
pub mod types;

#[cfg(feature = "experimental")]
pub use types::*;

// Dummy implementation when feature is disabled
#[cfg(not(feature = "experimental"))]
pub struct SemanticClassifier;

#[cfg(not(feature = "experimental"))]
impl SemanticClassifier {
    pub fn new() -> Option<Self> {
        None
    }
    pub fn classify(&self, _text: &str) -> Option<String> {
        None
    }
}
