mod detector;
mod router;
mod types;

pub use router::{resolve_npm_test_variant, resolve_variant};
pub use types::{VariantConfig, VariantDetect, VariantFilter};
