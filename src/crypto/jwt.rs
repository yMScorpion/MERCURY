use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rsa::{pkcs8::DecodePrivateKey, pss::BlindedSigningKey, RsaPrivateKey};
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use sha2::Sha256;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::rngs::OsRng;
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

/// Kalshi API authenticator
pub struct KalshiAuth {
    api_key_id: String,
    encoding_key: EncodingKey,
    signing_key: BlindedSigningKey<Sha256>,
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
            .context("Failed to parse RSA private key PEM for Kalshi JWT")?;
            
        let pem_str = std::str::from_utf8(rsa_private_key_pem).context("Invalid UTF-8 in Kalshi private key PEM")?;
        let private_key = RsaPrivateKey::from_pkcs8_pem(pem_str)
            .or_else(|_| rsa::pkcs1::DecodeRsaPrivateKey::from_pkcs1_pem(pem_str))
            .context("Failed to parse RSA private key PEM for Kalshi (tried PKCS#8 and PKCS#1)")?;
        let signing_key = BlindedSigningKey::<Sha256>::new(private_key);

        info!(api_key_id = %api_key_id, "Kalshi auth initialized");
        Ok(Self {
            api_key_id,
            encoding_key,
            signing_key,
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
    
    pub fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    /// Generate a fresh JWT token
    pub fn generate_token(&self) -> Result<String> {
        let now = Self::now_secs();
        let claims = KalshiClaims {
            sub: self.api_key_id.clone(),
            iat: now.saturating_sub(10),
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

    /// Generate Kalshi REST API authentication headers using RSA-PSS (v2 API).
    /// Returns the three required headers for all authenticated REST endpoints.
    pub fn generate_rest_headers(&self, method: &str, path: &str) -> Result<Vec<(String, String)>> {
        let timestamp = Self::now_millis().to_string();
        // Strip query parameters before signing (per Kalshi docs)
        let path_without_query = path.split('?').next().unwrap_or(path);
        let message = format!("{}{}{}", timestamp, method, path_without_query);
        
        let signature = self.signing_key.sign_with_rng(&mut OsRng, message.as_bytes());
        let signature_b64 = STANDARD.encode(signature.to_bytes());
        
        Ok(vec![
            ("KALSHI-ACCESS-KEY".to_string(), self.api_key_id.clone()),
            ("KALSHI-ACCESS-SIGNATURE".to_string(), signature_b64),
            ("KALSHI-ACCESS-TIMESTAMP".to_string(), timestamp),
        ])
    }
    
    /// Generate Kalshi WebSockets Headers
    pub fn generate_ws_headers(&self) -> Result<Vec<(String, String)>> {
        let timestamp = Self::now_millis().to_string();
        let method = "GET";
        let path = "/trade-api/ws/v2";
        let message = format!("{}{}{}", timestamp, method, path);
        
        let signature = self.signing_key.sign_with_rng(&mut OsRng, message.as_bytes());
        let signature_b64 = STANDARD.encode(signature.to_bytes());
        
        Ok(vec![
            ("KALSHI-ACCESS-KEY".to_string(), self.api_key_id.clone()),
            ("KALSHI-ACCESS-SIGNATURE".to_string(), signature_b64),
            ("KALSHI-ACCESS-TIMESTAMP".to_string(), timestamp),
        ])
    }
}
