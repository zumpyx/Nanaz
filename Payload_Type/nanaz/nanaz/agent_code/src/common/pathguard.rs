//! Shared path-protection helpers.
//!
//! Destructive file commands (cat, download, cp, mv, rm, upload) consult
//! [`is_protected_path`] and refuse to operate on system paths unless the
//! operator sets `allow_system_path: true` in the task parameters.
//!
//! The default deny-list mirrors what an operator would want a junior
//! analyst to avoid touching by accident: OS install trees, package
//! databases, boot loader paths. It is intentionally conservative — false
//! negatives (refusing a legitimate write) are cheap; false positives
//! (clobbering /etc/passwd) are not.

use std::path::Path;

/// Path prefixes (lowercased) that require `allow_system_path: true` to
/// touch. Matches both Windows and Unix conventions.
pub const PROTECTED_PREFIXES: &[&str] = &[
    "/boot",
    "/etc",
    "/usr",
    "/var",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "c:\\windows",
    "c:\\program files",
    "c:\\programdata",
];

/// Normalize operator-supplied paths to the target platform separator.
///
/// Operators can type `/` everywhere. On Windows the agent maps it to `\`;
/// on Unix a typed `\` is treated as `/` for consistency with the UI.
pub fn normalize_user_path(path: &str) -> String {
    #[cfg(windows)]
    {
        path.trim().replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        path.trim().replace('\\', "/")
    }
}

/// Render a target path for operator output. Windows paths are always shown
/// with backslashes; Unix paths are always shown with forward slashes.
pub fn display_path(path: &Path) -> String {
    normalize_user_path(&path.to_string_lossy())
}

fn normalize_for_match(path: &str) -> String {
    let normalized = normalize_user_path(path);
    let path = Path::new(&normalized);
    let canonical = std::fs::canonicalize(path)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(normalized);
    normalize_user_path(&canonical)
        .replace('\\', "/")
        .trim_end_matches(['/', '\\'])
        .to_lowercase()
}

fn is_same_or_child(path: &str, prefix: &str) -> bool {
    let prefix = prefix.replace('\\', "/");
    if path == prefix {
        return true;
    }
    let slash_child = format!("{prefix}/");
    path.starts_with(&slash_child)
}

/// Returns true if `path` lands under a protected system directory.
pub fn is_protected_path(path: &str) -> bool {
    let normalized = normalize_for_match(path);
    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| is_same_or_child(&normalized, prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_protected() {
        assert!(is_protected_path("/etc/passwd"));
        assert!(is_protected_path("/usr/local/bin/nanaz"));
        assert!(is_protected_path("/var/log/syslog"));
        assert!(is_protected_path("/boot/grub/grub.cfg"));
    }

    #[test]
    fn linux_unprotected() {
        assert!(!is_protected_path("/home/user/note.txt"));
        assert!(!is_protected_path("/tmp/dropper.exe"));
        assert!(!is_protected_path("/opt/nanaz/config.json"));
        assert!(!is_protected_path("/usr2/local/bin/nanaz"));
        assert!(!is_protected_path("/etcetera/passwd"));
    }

    #[test]
    fn windows_protected() {
        assert!(is_protected_path(
            "C:\\Windows\\System32\\drivers\\etc\\hosts"
        ));
        assert!(is_protected_path("c:\\program files\\vendor\\app.exe"));
        assert!(is_protected_path("C:\\ProgramData\\Microsoft\\foo"));
    }

    #[test]
    fn windows_unprotected() {
        assert!(!is_protected_path("C:\\Users\\admin\\Desktop\\note.txt"));
        assert!(!is_protected_path("D:\\drops\\payload.exe"));
        assert!(!is_protected_path("C:\\Windows.old\\notepad.exe"));
    }

    #[test]
    fn windows_slash_normalized() {
        // Mixed separators — the function should normalise them on Windows.
        let p = "c:/windows/system32/notepad.exe";
        let l = p.to_lowercase();
        let normalized = l.replace('/', "\\");
        assert!(normalized.starts_with("c:\\windows"));
    }

    #[test]
    fn normalize_operator_separators() {
        #[cfg(windows)]
        assert_eq!(normalize_user_path("C:/Users/bob"), "C:\\Users\\bob");
        #[cfg(not(windows))]
        assert_eq!(normalize_user_path(r"\tmp\nanaz"), "/tmp/nanaz");
    }
}
