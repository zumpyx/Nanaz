//! Shared path-protection helpers.
//!
//! File commands use these helpers for path normalization and display.

use std::path::Path;

/// Normalize operator-supplied paths to the target platform separator.
///
/// Operators can type `/` everywhere on Windows and the agent maps it to `\`.
/// On Unix, `\` is a legal filename byte, so it must not be rewritten into a
/// path separator.
pub fn normalize_user_path(path: &str) -> String {
    #[cfg(windows)]
    {
        let normalized = path.trim().replace('/', "\\");
        if normalized.len() == 2 && normalized.as_bytes()[1] == b':' {
            format!("{normalized}\\")
        } else {
            normalized
        }
    }
    #[cfg(not(windows))]
    {
        path.trim().to_string()
    }
}

/// Render a target path for operator output. Windows paths are always shown
/// with backslashes; Unix paths are always shown with forward slashes.
pub fn display_path(path: &Path) -> String {
    display_path_str(&path.to_string_lossy())
}

/// Render an already-stringified target path for operator output.
pub fn display_path_str(path: &str) -> String {
    let normalized = normalize_user_path(path);
    #[cfg(windows)]
    {
        normalized
            .strip_prefix(r"\\?\")
            .unwrap_or(&normalized)
            .to_string()
    }
    #[cfg(not(windows))]
    {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_operator_separators() {
        #[cfg(windows)]
        assert_eq!(normalize_user_path("C:/Users/bob"), "C:\\Users\\bob");
        #[cfg(not(windows))]
        assert_eq!(normalize_user_path(r"\tmp\nanaz"), r"\tmp\nanaz");
    }

    #[test]
    fn normalize_windows_drive_root() {
        #[cfg(windows)]
        {
            assert_eq!(normalize_user_path("C:"), r"C:\");
            assert_eq!(normalize_user_path("C:/"), r"C:\");
        }
        #[cfg(not(windows))]
        assert_eq!(normalize_user_path("C:"), "C:");
    }

    #[test]
    fn display_path_str_strips_windows_extended_prefix() {
        #[cfg(windows)]
        assert_eq!(display_path_str(r"\\?\C:\Users\bob"), r"C:\Users\bob");
    }
}
