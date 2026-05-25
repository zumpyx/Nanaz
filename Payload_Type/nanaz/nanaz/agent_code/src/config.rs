// src/config.rs
use crate::c2::C2Profile;
use serde::Deserialize;

// 💡 现代 Rust 的标准基操：完美吞入编译期生成的常量
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub payload_uuid: uuid::Uuid,
    pub c2_profiles: Vec<C2Profile>,
}

impl Config {
    pub fn load_json(config: &str) -> Self {
        serde_json::from_str(config).unwrap()
    }
    pub fn get_payload_uuid(&self) -> uuid::Uuid {
        self.payload_uuid.clone()
    }
    pub fn get_c2_profiles(&self) -> &[C2Profile] {
        &self.c2_profiles
    }
}

#[test]
fn test_load_config() {
    const RAW_JSON: &str = include_str!("../config.json");
    let config = Config::load_json(RAW_JSON);
    println!("{:?}", config);
}
