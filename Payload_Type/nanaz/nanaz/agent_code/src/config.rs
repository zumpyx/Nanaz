// src/config.rs
use crate::c2::C2Profile;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub payload_uuid: uuid::Uuid,
    pub c2_profiles: Vec<C2Profile>,
}

#[derive(Debug)]
pub enum ConfigError {
    Parse(String),
    Empty { reason: &'static str },
    Unsupported { reason: &'static str },
    InvalidPsk { reason: String },
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigError::Parse(e) => write!(f, "embedded config.json invalid: {e}"),
            ConfigError::Empty { reason } => write!(f, "embedded config.json unusable: {reason}"),
            ConfigError::Unsupported { reason } => {
                write!(
                    f,
                    "embedded config.json uses unsupported settings: {reason}"
                )
            }
            ConfigError::InvalidPsk { reason } => {
                write!(f, "embedded config.json has invalid PSK: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Parse embedded config JSON. Returns an error rather than silently
    /// substituting a nil UUID / empty profile list — a malformed config would
    /// otherwise let the binary "start" and immediately exit, which is
    /// impossible to distinguish from a successful no-op tasking.
    pub fn load_json(config: &str) -> Result<Self, ConfigError> {
        let parsed: Self =
            serde_json::from_str(config).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if parsed.payload_uuid.is_nil() {
            return Err(ConfigError::Empty {
                reason: "payload_uuid is nil",
            });
        }
        if parsed.c2_profiles.is_empty() {
            return Err(ConfigError::Empty {
                reason: "no c2_profiles configured",
            });
        }
        if parsed.c2_profiles.len() != 1 {
            return Err(ConfigError::Unsupported {
                reason: "exactly one http C2 profile is supported",
            });
        }
        // PSK validation. Mythic generates a 32-byte base64 AES key per
        // payload; rejecting malformed keys at load time means a typo
        // surfaces as "refusing to start" instead of a confusing AES error
        // deep in the C2 layer.
        for profile in &parsed.c2_profiles {
            profile.validate_psk()?;
        }
        Ok(parsed)
    }
}

#[test]
fn test_load_config() {
    const RAW_JSON: &str = include_str!("../config.json");
    // A nil-UUID, empty-profile, or redacted-PSK config (the placeholder)
    // will fail the validation checks; we only assert the function does
    // not panic on the whatever-the-developer-has-checked-in JSON.
    let _ = Config::load_json(RAW_JSON);
}

#[test]
fn rejects_multiple_c2_profiles() {
    let raw = r#"{
        "payload_uuid": "11111111-1111-4111-8111-111111111111",
        "c2_profiles": [
            {
                "http": {
                    "aes_psk": null,
                    "callback_host": "http://127.0.0.1",
                    "callback_interval": 10,
                    "callback_jitter": 0,
                    "callback_port": 80,
                    "encrypted_exchange_check": false,
                    "get_uri": "index",
                    "headers": {},
                    "killdate": "",
                    "post_uri": "data",
                    "proxy_host": "",
                    "proxy_pass": "",
                    "proxy_port": "",
                    "proxy_user": "",
                    "query_path_name": "q",
                    "external_ip_check": false
                }
            },
            {
                "http": {
                    "aes_psk": null,
                    "callback_host": "http://127.0.0.2",
                    "callback_interval": 10,
                    "callback_jitter": 0,
                    "callback_port": 80,
                    "encrypted_exchange_check": false,
                    "get_uri": "index",
                    "headers": {},
                    "killdate": "",
                    "post_uri": "data",
                    "proxy_host": "",
                    "proxy_pass": "",
                    "proxy_port": "",
                    "proxy_user": "",
                    "query_path_name": "q",
                    "external_ip_check": false
                }
            }
        ]
    }"#;

    assert!(matches!(
        Config::load_json(raw),
        Err(ConfigError::Unsupported { .. })
    ));
}
