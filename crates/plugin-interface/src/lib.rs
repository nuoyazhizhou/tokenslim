//! Stable FFI contract shared by TokenSlim and dynamic plugins.

/// Dynamic plugin ABI version.
///
/// Bump this when a breaking change is made to exported FFI symbols
/// or memory/data layout assumptions.
pub const PLUGIN_API_VERSION: u32 = 1;
