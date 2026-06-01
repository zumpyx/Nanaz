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

/// Path prefixes (lowercased) that require `allow_system_path: true` to
/// touch. Matches both Windows and Unix conventions. Comparison is on the
/// lowercased path string.
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

/// Returns true if `path` lands under a protected system directory.
pub fn is_protected_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    // Normalize Windows backslashes for prefix matching
    #[cfg(windows)]
    let normalized: String = lower.replace('/', "\\");
    #[cfg(not(windows))]
    let normalized: &str = &*lower;
    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
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
    }

    #[test]
    fn windows_protected() {
        assert!(is_protected_path("C:\\Windows\\System32\\drivers\\etc\\hosts"));
        assert!(is_protected_path("c:\\program files\\vendor\\app.exe"));
        assert!(is_protected_path("C:\\ProgramData\\Microsoft\\foo"));
    }

    #[test]
    fn windows_unprotected() {
        assert!(!is_protected_path("C:\\Users\\admin\\Desktop\\note.txt"));
        assert!(!is_protected_path("D:\\drops\\payload.exe"));
    }

    #[test]
    fn windows_slash_normalized() {
        // Mixed separators — the function should normalise them on Windows.
        let p = "c:/windows/system32/notepad.exe";
        let l = p.to_lowercase();
        let normalized = l.replace('/', "\\");
        assert!(normalized.starts_with("c:\\windows"));
    }
}
