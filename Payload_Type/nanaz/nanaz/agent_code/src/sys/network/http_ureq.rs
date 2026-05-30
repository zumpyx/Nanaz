use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{Error, Result};

thread_local! {
    static AGENT: RefCell<Option<ureq::Agent>> = RefCell::new(None);
}

fn build_agent(proxy: Option<&str>) -> Result<ureq::Agent> {
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| Error::Transport(format!("TLS init failed: {}", e)))?;

    let mut builder = ureq::AgentBuilder::new().tls_connector(Arc::new(tls));

    if let Some(p) = proxy {
        let px = ureq::Proxy::new(p)
            .map_err(|e| Error::Transport(format!("proxy parse failed: {e}")))?;
        builder = builder.proxy(px);
    }

    Ok(builder.build())
}

fn get_agent(proxy: Option<&str>) -> Result<ureq::Agent> {
    // Proxy: don't cache, create fresh each time
    if proxy.is_some() {
        return build_agent(proxy);
    }

    // No proxy: use cached agent
    AGENT.with(|cell| {
        if let Some(ref agent) = *cell.borrow() {
            return Ok(agent.clone());
        }
        let agent = build_agent(None)?;
        cell.replace(Some(agent.clone()));
        Ok(agent)
    })
}

pub fn http_request(
    url: &str,
    method: &str,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
    body: Option<&str>,
) -> Result<String> {
    let agent = get_agent(proxy)?;

    match method {
        "GET" => {
            let mut req = agent.get(url);
            if let Some(h) = headers {
                for (k, v) in h {
                    req = req.set(k, v);
                }
            }
            let response = req
                .call()
                .map_err(|e| Error::Transport(format!("GET failed: {}", e)))?;
            Ok(response
                .into_string()
                .map_err(|e| Error::Transport(format!("read response failed: {}", e)))?)
        }
        "POST" => {
            let mut req = agent.post(url);
            if let Some(h) = headers {
                for (k, v) in h {
                    req = req.set(k, v);
                }
            }
            let response = req
                .send_string(body.unwrap_or_default())
                .map_err(|e| Error::Transport(format!("POST failed: {}", e)))?;
            Ok(response
                .into_string()
                .map_err(|e| Error::Transport(format!("read response failed: {}", e)))?)
        }
        _ => Err(Error::Transport("unsupported HTTP method".into())),
    }
}
