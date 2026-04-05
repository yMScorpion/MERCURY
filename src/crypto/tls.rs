//! TLS connector helpers shared across feed and execution modules.
//! Centralises native-TLS configuration (min protocol, optional cert pinning).

use anyhow::{Context, Result};
use native_tls::TlsConnector;

const PINNED_CERT_PATH: &str = "/opt/mercury/keys/pinned_certs.pem";

/// Build a `native_tls::TlsConnector` with TLSv1.2 minimum and optional cert
/// pinning from `PINNED_CERT_PATH`.
pub fn build_tls_connector() -> Result<TlsConnector> {
    let mut builder = native_tls::TlsConnector::builder();
    builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
    if let Ok(cert_pem) = std::fs::read(PINNED_CERT_PATH) {
        if let Ok(cert) = native_tls::Certificate::from_pem(&cert_pem) {
            builder.add_root_certificate(cert);
        }
    }
    builder.build().context("Failed to build TLS connector")
}

/// Build a `reqwest::Client` with cert pinning when available.
pub fn build_reqwest_client() -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Ok(cert_pem) = std::fs::read(PINNED_CERT_PATH) {
        if let Ok(cert) = reqwest::Certificate::from_pem(&cert_pem) {
            builder = builder.add_root_certificate(cert);
        }
    }
    builder.build().context("Failed to build reqwest client")
}
