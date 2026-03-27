use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct KalshiClaims {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Kalshi API JWT authenticator
#[derive(Clone)]
pub struct KalshiAuth {
    api_key_id: String,
    encoding_key: EncodingKey,
}

impl KalshiAuth {
    /// Create from RSA private key PEM and API key ID
    pub fn new(api_key_id: String, rsa_private_key_pem: &[u8]) -> Result<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(rsa_private_key_pem)
            .context("Failed to parse RSA private key PEM for Kalshi")?;

        info!(api_key_id = %api_key_id, "Kalshi JWT auth initialized");
        Ok(Self {
            api_key_id,
            encoding_key,
        })
    }

    /// Generate a fresh JWT token (valid for 10 minutes)
    pub fn generate_token(&self) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = KalshiClaims {
            sub: self.api_key_id.clone(),
            iat: now,
            exp: now + 600,
        };

        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &self.encoding_key)
            .context("Failed to encode JWT")?;

        Ok(token)
    }

    /// Generate Authorization header value
    pub fn auth_header(&self) -> Result<String> {
        let token = self.generate_token()?;
        Ok(format!("Bearer {}", token))
    }
}
