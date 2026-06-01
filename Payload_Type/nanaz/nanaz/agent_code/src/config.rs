// src/config.rs
use crate::c2::C2Profile;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub payload_uuid: uuid::Uuid,
    pub c2_profiles: Vec<C2Profile>,
}

impl Config {
    /// Parse embedded config JSON. Falls back to an all-zeros UUID and empty
    /// profile list on parse failure so the binary still starts (and surfaces a
    /// clean error via the C2 layer) rather than panicking before main.
    pub fn load_json(config: &str) -> Self {
        match serde_json::from_str(config) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[config] embedded config.json invalid: {e}");
                Self {
                    payload_uuid: uuid::Uuid::nil(),
                    c2_profiles: Vec::new(),
                }
            }
        }
    }
}

#[test]
fn test_load_config() {
    const RAW_JSON: &str = include_str!("../config.json");
    let _ = Config::load_json(RAW_JSON);
}
