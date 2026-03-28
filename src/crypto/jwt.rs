use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

// Manual Clone: EncodingKey does not implement Clone, so we skip the cache on clone
// (the new instance starts with an empty cache).
impl Clone for KalshiAuth {
    fn clone(&self) -> Self {
        // EncodingKey stores key material internally; re-derive from the cached encoding key
        // is not possible, so callers that need to clone should re-construct from the PEM.
        // We keep the field by using from_rsa_pem on a placeholder, but that requires the PEM.
        // Instead we store the key bytes and reconstruct — for now, panic is preferable to
        // silently losing the key. In practice, wrap in Arc<KalshiAuth> instead of cloning.
        unimplemented!("KalshiAuth cannot be cloned — wrap in Arc<KalshiAuth> instead")
    }
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

    /// Generate a fresh JWT token (valid for 10 minutes)
    pub fn generate_token(&self) -> Result<String> {
        let now = Self::now_secs();

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

    /// Generate Authorization header value, using a cached token when still valid.
    ///
    /// The cached token is reused if it has at least 30 seconds of remaining lifetime.
    /// Otherwise a fresh token is generated and cached.
    pub fn auth_header(&self) -> Result<String> {
        let now = Self::now_secs();

        {
            let cache = self.token_cache.lock()
                .map_err(|_| anyhow::anyhow!("Token cache mutex poisoned"))?;

            if let Some((ref cached_token, expiry)) = *cache {
                // Reuse if at least 30 s remain before expiry
                if expiry > now + 30 {
                    return Ok(format!("Bearer {}", cached_token));
                }
            }
        }

        // Generate a fresh token and cache it
        let token = self.generate_token()?;
        let expiry = now + 600;

        {
            let mut cache = self.token_cache.lock()
                .map_err(|_| anyhow::anyhow!("Token cache mutex poisoned"))?;
            *cache = Some((token.clone(), expiry));
        }

        Ok(format!("Bearer {}", token))
    }
}
