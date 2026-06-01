//! Base64 encode / decode — thin wrappers around the `base64` crate.
//!
//! `no_std` compatible (only uses `alloc`).

use alloc::string::String;
use alloc::vec::Vec;

/// Decode a standard base64 string into raw bytes.
///
/// Kept as a public helper for any future command that needs the decoded
/// bytes directly (the upload command now uses [`base64::read::DecoderReader`]
/// to stream-decode). Marked `allow(dead_code)` so the binary still
/// builds cleanly until a second user lands.
#[allow(dead_code)]
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
