use crate::{NError, NResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn http_request(url: &str, method: &str, body: Option<&[u8]>) -> NResult<String> {
    match method {
        "GET" => {
            let response = minreq::get(url).send().unwrap();
            let response = response.as_str().unwrap();
            Ok(response.to_string())
        }
        "POST" => {
            let response = minreq::post(url)
                .with_body(body.unwrap_or_default())
                .send()
                .unwrap();
            let body = response.as_str().unwrap();
            Ok(body.to_string())
        }
        _ => Err(NError::CheckError),
    }
}

// 🌟 完善后的测试模块
#[cfg(test)]
mod tests {
    use super::*;

    /// 🟢 测试场景 1：验证标准的 GET 请求是否能正常拿到响应
    #[test]
    fn test_http_get_request() {
        // 使用公网标准的 HTTP 探测靶场（使用 http 避免未开启 https-rustls 特性时报错）
        let url = "http://httpbin.org/get";

        let result = http_request(url, "GET", None);

        assert!(result.is_ok(), "GET 请求应当执行成功");
        let body = result.unwrap();

        // 验证返回的 JSON 字典中是否包含 httpbin 的特征，确保数据确实是控端/服务端回传的
        assert!(
            body.contains("httpbin.org"),
            "响应体中应包含正确的 Host 路由特征"
        );
    }

    /// 🔵 测试场景 2：验证 POST 请求是否能正确把 Body 载荷顶给控端
    #[test]
    fn test_http_post_request() {
        let url = "http://httpbin.org/post";
        let test_payload = b"nanaz_agent_test_data";

        let result = http_request(url, "POST", Some(test_payload));

        assert!(result.is_ok(), "POST 请求应当执行成功");
        let body = result.unwrap();

        // httpbin/post 会把接收到的 data 原封不动回显在 JSON 的 "data" 字段里
        assert!(
            body.contains("nanaz_agent_test_data"),
            "服务端应当正确接收并回显 POST Body 载荷"
        );
    }

    /// 🔴 测试场景 3：边界防御测试，验证不支持的 Method 是否会触发错误状态熔断
    #[test]
    fn test_invalid_http_method() {
        let url = "http://httpbin.org/put";

        // 投递 Agent 不支持的 PUT 方法
        let result = http_request(url, "PUT", None);

        assert!(result.is_err(), "遇到不支持的 HTTP 方法时应当返回错误");

        // 深度校验返回的错误类型是否为你定义的 CheckError
        if let Err(err) = result {
            assert!(
                matches!(err, NError::CheckError),
                "应当精准匹配 NError::CheckError 熔断枚举"
            );
        }
    }
}
