use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct KalshiClaims {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Kalshi API JWT authenticator
pub struct KalshiAuth {
    api_key_id: String,
    encoding_key: EncodingKey,
    /// Cached token: (token_string, expiry_timestamp_secs)
    token_cache: Mutex<Option<(String, u64)>>,
}

impl KalshiAuth {
    pub fn api_key_id(&self) -> &str {
        &self.api_key_id
    }

    /// Create from RSA private key PEM and API key ID
    pub fn new(api_key_id: String, rsa_private_key_pem: &[u8]) -> Result<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(rsa_private_key_pem)
            .context("Failed to parse RSA private key PEM for Kalshi")?;
        info!(api_key_id = %api_key_id, "Kalshi JWT auth initialized");
        Ok(Self {
            api_key_id,
            encoding_key,
            token_cache: Mutex::new(None),
        })
    }

    /// Return current Unix timestamp in seconds, falling back to 0 on clock error.
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Generate a fresh JWT token
    pub fn generate_token(&self) -> Result<String> {
        let now = Self::now_secs();
        let claims = KalshiClaims {
            sub: self.api_key_id.clone(),
            iat: now - 10,
            exp: now + 300,
        };
        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &self.encoding_key)
            .context("Failed to encode JWT")?;
        Ok(token)
    }

    /// Generate Authorization header value, using a cached token when still valid.
    pub async fn auth_header(&self) -> Result<String> {
        let now = Self::now_secs();
        {
            let cache = self.token_cache.lock().await;
            if let Some((ref cached_token, expiry)) = *cache {
                if expiry > now + 30 {
                    return Ok(format!("Bearer {}", cached_token));
                }
            }
        }

        let token = self.generate_token()?;
        let expiry = now + 290;
        {
            let mut cache = self.token_cache.lock().await;
            *cache = Some((token.clone(), expiry));
        }
        Ok(format!("Bearer {}", token))
    }
}
