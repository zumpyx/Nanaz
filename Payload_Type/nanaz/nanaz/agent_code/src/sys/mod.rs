pub mod encoding;
pub mod metadata;
pub mod network;

/// Build a temp file path from a filename, sanitising path separators.
#[cfg(windows)]
pub fn temp_path(name: &str) -> String {
    let safe = std::path::Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let tmp = std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into());
    format!("{}\\{}", tmp.trim_end_matches('\\'), safe)
}

#[cfg(unix)]
pub fn temp_path(name: &str) -> String {
    let safe = std::path::Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    format!("/tmp/{safe}")
}
