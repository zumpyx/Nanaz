// src/config.rs
use crate::c2::C2Profile;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub payload_uuid: uuid::Uuid,
    pub c2_profiles: Vec<C2Profile>,
}

impl Config {
    pub fn load_json(config: &str) -> Self {
        serde_json::from_str(config).expect("config.json: invalid JSON")
    }
}

#[test]
fn test_load_config() {
    const RAW_JSON: &str = include_str!("../config.json");
    let config = Config::load_json(RAW_JSON);
    println!("{:?}", config);
}
