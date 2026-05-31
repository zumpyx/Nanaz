//! HTTP client backed by ureq + rustls (pure Rust TLS — no system OpenSSL needed).
//!
//! Uses a thread-local agent cache for non-proxy connections.
//! Self-signed / invalid certs are accepted (C2 typical).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};

use crate::{Error, Result};

thread_local! {
    static AGENT: RefCell<Option<ureq::Agent>> = RefCell::new(None);
}

// ── TLS verifier that accepts everything ────────────────────

#[derive(Debug)]
struct NoVerification;

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> core::result::Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> core::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> core::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

// ── Agent construction ──────────────────────────────────────

fn danger_tls_config() -> Result<Arc<ClientConfig>> {
    let provider = Arc::new(default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Transport(format!("TLS protocol versions: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerification))
        .with_no_client_auth();

    Ok(Arc::new(config))
}

fn build_agent(proxy: Option<&str>) -> Result<ureq::Agent> {
    let tls_config = danger_tls_config()?;

    let mut builder = ureq::AgentBuilder::new().tls_config(tls_config);

    if let Some(p) = proxy {
        let px = ureq::Proxy::new(p)
            .map_err(|e| Error::Transport(format!("proxy parse failed: {e}")))?;
        builder = builder.proxy(px);
    }

    Ok(builder.build())
}

fn get_agent(proxy: Option<&str>) -> Result<ureq::Agent> {
    if proxy.is_some() {
        return build_agent(proxy);
    }

    AGENT.with(|cell| {
        if let Some(ref agent) = *cell.borrow() {
            return Ok(agent.clone());
        }
        let agent = build_agent(None)?;
        cell.replace(Some(agent.clone()));
        Ok(agent)
    })
}

// ── Public API ──────────────────────────────────────────────

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
