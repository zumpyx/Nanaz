// src/sys/http/n_ureq.rs
use crate::{NError, NResult};
use std::sync::Arc;

pub fn http_request(url: &str, method: &str, body: Option<&[u8]>) -> NResult<String> {
    // 1. 🌟 创建一个底层的原生 TLS 建立器，并在其中强行无视证书
    let mut tls_builder = native_tls::TlsConnector::builder();
    tls_builder.danger_accept_invalid_certs(true); // 关掉证书合法性校验
    tls_builder.danger_accept_invalid_hostnames(true); // 关掉域名匹配校验

    let tls_connector = tls_builder
        .build()
        .map_err(|e| NError::Custom(format!("TLS 建立器初始化失败: {}", e)))?;

    // 2. 🌟 把这个放荡不羁的 TLS 建立器，通过 Arc 注入给 ureq
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls_connector))
        .build();

    // 3. 正常的业务分发
    match method {
        "GET" => {
            let response = agent
                .get(url)
                .call()
                .map_err(|e| NError::Custom(format!("GET 失败: {}", e)))?;

            let response_str = response
                .into_string()
                .map_err(|e| NError::Custom(format!("读取响应失败: {}", e)))?;

            Ok(response_str)
        }
        "POST" => {
            let payload = body.unwrap_or_default();

            let response = agent
                .post(url)
                .send_bytes(payload)
                .map_err(|e| NError::Custom(format!("POST 失败: {}", e)))?;

            let response_str = response
                .into_string()
                .map_err(|e| NError::Custom(format!("读取响应失败: {}", e)))?;

            Ok(response_str)
        }
        _ => Err(NError::CheckError),
    }
}
