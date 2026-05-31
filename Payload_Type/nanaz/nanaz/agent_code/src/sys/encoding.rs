//! System encoding detection for child-process output.
//!
//! On Windows, command output is typically in the system's ANSI code page
//! (e.g. GBK/CP936 for Chinese, Shift-JIS/CP932 for Japanese), not UTF-8.
//! `decode_output()` tries UTF-8 first and falls back to the system locale
//! encoding when replacement characters are detected.

/// Decode raw bytes from a child process into a String.
///
/// Strategy:
/// 1. Try strict UTF-8 (if it passes, return immediately — no allocation waste)
/// 2. Try `from_utf8_lossy` — if no replacement chars, return
/// 3. On Windows: detect system ANSI code page via `GetACP()`, decode with `encoding_rs`
/// 4. Fallback: return the lossy UTF-8 result
pub fn decode_output(bytes: &[u8]) -> String {
    // Fast path: valid UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // Lossy path — check if it produced replacement chars
    let lossy = String::from_utf8_lossy(bytes);
    if !lossy.contains('\u{FFFD}') {
        return lossy.to_string();
    }

    // Lossy produced replacements — try system encoding
    #[cfg(windows)]
    {
        if let Some(decoded) = decode_with_acp(bytes) {
            if !decoded.is_empty() {
                return decoded;
            }
        }
    }

    lossy.to_string()
}

/// Decode bytes using the Windows ANSI code page (`GetACP`).
#[cfg(windows)]
fn decode_with_acp(bytes: &[u8]) -> Option<String> {
    let acp = unsafe { windows_acp::GetACP() };
    // Map common code pages to encoding_rs labels
    let label = code_page_label(acp)?;
    let (decoded, _encoding, had_errors) = encoding_rs::Encoding::for_label(label.as_bytes())?.decode(bytes);
    if had_errors {
        // Fall back to replacement-based decode (returns Option<Cow<str>>)
        encoding_rs::Encoding::for_label(label.as_bytes())?
            .decode_without_bom_handling_and_without_replacement(bytes)
            .map(|c| c.into_owned())
    } else {
        Some(decoded.into_owned())
    }
}

/// Map a Windows code page number to an encoding_rs label.
#[cfg(windows)]
fn code_page_label(cp: u32) -> Option<&'static str> {
    match cp {
        936 => Some("gbk"),        // Chinese Simplified
        950 => Some("big5"),       // Chinese Traditional
        932 => Some("shift_jis"),  // Japanese
        949 => Some("euc-kr"),     // Korean
        1250 => Some("windows-1250"), // Central/Eastern Europe
        1251 => Some("windows-1251"), // Cyrillic
        1252 => Some("windows-1252"), // Western European
        1253 => Some("windows-1253"), // Greek
        1254 => Some("windows-1254"), // Turkish
        1255 => Some("windows-1255"), // Hebrew
        1256 => Some("windows-1256"), // Arabic
        1257 => Some("windows-1257"), // Baltic
        1258 => Some("windows-1258"), // Vietnamese
        874 => Some("windows-874"),   // Thai
        _ => None,
    }
}

/// Windows FFI — GetACP.
#[cfg(windows)]
mod windows_acp {
    unsafe extern "system" {
        pub fn GetACP() -> u32;
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8_clean() {
        let input = "hello world 你好".as_bytes();
        let result = decode_output(input);
        assert_eq!(result, "hello world 你好");
    }

    #[test]
    fn test_decode_utf8_lossy() {
        // Invalid UTF-8 bytes
        let input = b"hello \xc0\xc1 world";
        let result = decode_output(input);
        // Should contain replacement chars (or be decoded via system encoding)
        assert!(!result.is_empty());
    }

    #[test]
    fn test_decode_empty() {
        let result = decode_output(b"");
        assert!(result.is_empty());
    }
}
