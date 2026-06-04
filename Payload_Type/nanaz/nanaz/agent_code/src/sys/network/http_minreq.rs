//! HTTP client backed by minreq.
//!
//! The public helpers keep the old network module API so C2 and `wget` do not
//! need to know which HTTP client is compiled in.

use std::collections::HashMap;
use std::io::{Read, Write};

use crate::{Error, Result};

const REQUEST_TIMEOUT_SECS: u64 = 60;
const MAX_HEADERS_SIZE: usize = 64 * 1024;
const MAX_STATUS_LINE_LEN: usize = 8 * 1024;

fn map_minreq_error(context: &str, err: minreq::Error) -> Error {
    Error::Transport(format!("{context}: {err}"))
}

fn apply_common_options(
    mut req: minreq::Request,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
) -> Result<minreq::Request> {
    if let Some(h) = headers {
        for (k, v) in h {
            req = req.with_header(k, v);
        }
    }

    if let Some(proxy_url) = proxy.filter(|p| !p.trim().is_empty()) {
        let proxy =
            minreq::Proxy::new(proxy_url).map_err(|e| map_minreq_error("proxy parse failed", e))?;
        req = req.with_proxy(proxy);
    }

    Ok(req
        .with_timeout(REQUEST_TIMEOUT_SECS)
        .with_max_headers_size(MAX_HEADERS_SIZE)
        .with_max_status_line_length(MAX_STATUS_LINE_LEN))
}

fn ensure_success(method: &str, url: &str, status_code: i32, reason: &str) -> Result<()> {
    if (200..300).contains(&status_code) {
        return Ok(());
    }
    Err(Error::Transport(format!(
        "{method} {url} returned HTTP {status_code} {reason}"
    )))
}

#[allow(clippy::too_many_arguments)]
pub fn http_request(
    url: &str,
    method: &str,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
    body: Option<&str>,
) -> Result<String> {
    let req = match method {
        "GET" => minreq::get(url),
        "POST" => minreq::post(url).with_body(body.unwrap_or_default()),
        _ => return Err(Error::Transport("unsupported HTTP method".into())),
    };
    let response = apply_common_options(req, headers, proxy)?
        .send()
        .map_err(|e| map_minreq_error(&format!("{method} failed"), e))?;
    ensure_success(method, url, response.status_code, &response.reason_phrase)?;
    response
        .as_str()
        .map(str::to_owned)
        .map_err(|e| map_minreq_error("read response failed", e))
}

/// Stream a GET response body into a writer, refusing to read past `max_bytes`.
#[allow(clippy::too_many_arguments)]
pub fn http_get_to_writer<W: Write>(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
    max_bytes: u64,
    writer: &mut W,
) -> Result<u64> {
    let mut response = apply_common_options(minreq::get(url), headers, proxy)?
        .send_lazy()
        .map_err(|e| map_minreq_error("GET failed", e))?;
    ensure_success("GET", url, response.status_code, &response.reason_phrase)?;

    if let Some(len) = response.headers.get("content-length")
        && let Ok(n) = len.parse::<u64>()
        && n > max_bytes
    {
        return Err(Error::Transport(format!(
            "Content-Length {n} exceeds cap {max_bytes}"
        )));
    }

    let mut buf = [0u8; 16 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|e| Error::Transport(format!("read body failed: {e}")))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_bytes {
            return Err(Error::Transport(format!(
                "body exceeds cap {max_bytes} (read at least {total} bytes)"
            )));
        }
        writer
            .write_all(&buf[..n])
            .map_err(|e| Error::Transport(format!("write failed: {e}")))?;
    }
    Ok(total)
}
