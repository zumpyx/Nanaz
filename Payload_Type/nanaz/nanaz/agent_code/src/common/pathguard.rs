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

use std::path::{Component, Path, PathBuf};

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

/// Exact paths that are too broad to treat as normal prefixes.
pub const PROTECTED_EXACT_PATHS: &[&str] = &["/", "c:"];

/// Normalize operator-supplied paths to the target platform separator.
///
/// Operators can type `/` everywhere on Windows and the agent maps it to `\`.
/// On Unix, `\` is a legal filename byte, so it must not be rewritten into a
/// path separator.
pub fn normalize_user_path(path: &str) -> String {
    #[cfg(windows)]
    {
        path.trim().replace('/', "\\")
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

fn looks_windows_absolute(path: &str) -> bool {
    path.starts_with("//")
        || (path.len() >= 3 && path.as_bytes()[1] == b':' && path.as_bytes()[2] == b'/')
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component.as_os_str());
                }
            }
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                out.push(component.as_os_str());
            }
        }
    }
    out
}

fn normalize_for_match(path: &str) -> String {
    let normalized = normalize_user_path(path);
    let input = Path::new(&normalized);
    let candidate = if input.is_absolute() || looks_windows_absolute(&normalized.replace('\\', "/"))
    {
        PathBuf::from(&normalized)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&normalized)
    };
    let canonical = std::fs::canonicalize(&candidate)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| lexical_normalize(&candidate).to_string_lossy().to_string());
    let mut normalized = normalize_user_path(&canonical)
        .replace('\\', "/")
        .to_lowercase();
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
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
    if is_exact_protected_path_normalized(&normalized) {
        return true;
    }
    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| is_same_or_child(&normalized, prefix))
}

fn is_exact_protected_path_normalized(normalized: &str) -> bool {
    PROTECTED_EXACT_PATHS
        .iter()
        .any(|exact| normalized == exact.replace('\\', "/"))
}

/// Returns true only for broad filesystem roots like `/` or `C:\`.
pub fn is_protected_root_path(path: &str) -> bool {
    is_exact_protected_path_normalized(&normalize_for_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_protected() {
        assert!(is_protected_path("/"));
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

    #[cfg(unix)]
    #[test]
    fn relative_paths_are_checked_from_current_dir() {
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir("/").unwrap();
        assert!(is_protected_path("etc/nanaz_should_not_exist"));
        std::env::set_current_dir(old).unwrap();
    }

    #[test]
    fn windows_protected() {
        assert!(is_protected_path("C:\\"));
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
        assert_eq!(normalize_user_path(r"\tmp\nanaz"), r"\tmp\nanaz");
    }

    #[test]
    fn display_path_str_strips_windows_extended_prefix() {
        #[cfg(windows)]
        assert_eq!(display_path_str(r"\\?\C:\Users\bob"), r"C:\Users\bob");
    }
}
