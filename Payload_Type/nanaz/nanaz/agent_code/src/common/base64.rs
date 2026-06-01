//! Base64 encode / decode — thin wrappers around the `base64` crate.
//!
//! `no_std` compatible (only uses `alloc`).

use alloc::string::String;
use alloc::vec::Vec;

/// Decode a standard base64 string into raw bytes.
pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| alloc::format!("base64 decode failed: {e}"))
}

/// Encode raw bytes as a standard base64 string.
#[allow(dead_code)] // only used by upload.rs tests
pub fn encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}
