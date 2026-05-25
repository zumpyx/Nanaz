use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ReqCheckin {
    pub action: String,
    pub uuid: uuid::Uuid,
    pub ips: Vec<String>,
    pub os: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub pid: Option<u32>,
    pub architecture: Option<String>,
    pub domain: Option<String>,
    pub integrity_level: Option<u32>,
    pub external_ip: Option<String>,
    pub encryption_key: Option<String>,
    pub decryption_key: Option<String>,
    pub process_name: Option<String>,
}

impl ReqCheckin {
    pub fn get_checkin_info(uuid: uuid::Uuid) -> Self {
        Self {
            action: "checkin".to_string(),
            uuid,
            ips: crate::sys::get_internal_ips(),
            os: crate::sys::get_os(),
            user: crate::sys::get_user(),
            host: crate::sys::get_hostname(),
            pid: crate::sys::get_pid(),
            architecture: crate::sys::get_arch(),
            domain: crate::sys::get_domain(),
            integrity_level: crate::sys::get_integrity_level(),
            external_ip: crate::sys::get_external_ip(),
            encryption_key: None,
            decryption_key: None,
            process_name: crate::sys::get_process_name(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RespCheckin {
    pub action: String,
    pub id: uuid::Uuid,
    pub status: String,
}

pub struct ReqStagingRSA {
    pub action: String,
    pub pub_key: String,
    pub session_id: String,
}

pub struct RespStagingRSA {
    pub action: String,
    pub uuid: uuid::Uuid,
    pub session_key: String,
    pub session_id: String,
}

// # Custom EKE
//
//
