# Phase 3: Crypto Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build encrypted keystore, EIP-712 signing for Polymarket, and JWT/RSA authentication for Kalshi.

**Architecture:** AES-256-GCM encrypted keyfile for wallet private keys. EIP-712 structured data signing via `ethers`. RSA-based JWT generation for Kalshi API auth.

**Tech Stack:** aes-gcm, ethers, jsonwebtoken, rsa, ring, sha2, hex, base64

**Depends on:** Phase 1

---

### Task 1: Encrypted Keystore

**Files:**
- Create: `src/crypto/mod.rs`
- Create: `src/crypto/keystore.rs`

- [ ] **Step 1: Write the keystore module**

`src/crypto/mod.rs`:
```rust
pub mod keystore;
pub mod eip712;
pub mod jwt;
```

`src/crypto/keystore.rs`:
```rust
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::info;

const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 32;
// File format: [salt:32][nonce:12][ciphertext:...]

/// Encrypt a private key and save to file
pub fn encrypt_keyfile(key_bytes: &[u8], passphrase: &str, path: &str) -> Result<()> {
    let mut salt = [0u8; SALT_SIZE];
    rand::thread_rng().fill_bytes(&mut salt);

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let derived_key = derive_key(passphrase, &salt);
    let cipher = Aes256Gcm::new_from_slice(&derived_key)
        .context("Failed to create AES cipher")?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, key_bytes)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut output = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &output)?;
    info!("Encrypted keyfile saved to {}", path);
    Ok(())
}

/// Decrypt a private key from an encrypted file
pub fn decrypt_keyfile(path: &str, passphrase: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(path)
        .with_context(|| format!("Failed to read keyfile: {}", path))?;

    if data.len() < SALT_SIZE + NONCE_SIZE + 16 {
        anyhow::bail!("Keyfile too short to be valid");
    }

    let salt = &data[..SALT_SIZE];
    let nonce_bytes = &data[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
    let ciphertext = &data[SALT_SIZE + NONCE_SIZE..];

    let derived_key = derive_key(passphrase, salt);
    let cipher = Aes256Gcm::new_from_slice(&derived_key)
        .context("Failed to create AES cipher")?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed (wrong passphrase?): {}", e))?;

    info!("Keyfile decrypted successfully from {}", path);
    Ok(plaintext)
}

/// Derive a 256-bit key from passphrase + salt using SHA-256 (simple KDF)
/// For production, consider Argon2 or scrypt, but SHA-256 is adequate here
fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(passphrase.as_bytes());
    hasher.update(salt);
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}
```

- [ ] **Step 2: Commit**

```bash
git add src/crypto/
git commit -m "feat: add AES-256-GCM encrypted keystore for wallet private keys"
```

---

### Task 2: EIP-712 Signing for Polymarket

**Files:**
- Create: `src/crypto/eip712.rs`

- [ ] **Step 1: Write EIP-712 signer**

`src/crypto/eip712.rs`:
```rust
use anyhow::{Context, Result};
use ethers::core::types::{Address, U256};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::transaction::eip712::{EIP712Domain, Eip712};
use std::str::FromStr;
use tracing::info;

/// Polymarket CLOB order signer
#[derive(Clone)]
pub struct PolymarketSigner {
    wallet: LocalWallet,
    chain_id: u64,
}

impl PolymarketSigner {
    /// Create from raw private key bytes
    pub fn new(private_key_bytes: &[u8], chain_id: u64) -> Result<Self> {
        let wallet = LocalWallet::from_bytes(private_key_bytes)
            .context("Failed to create wallet from private key")?
            .with_chain_id(chain_id);

        info!(address = %wallet.address(), chain_id, "Polymarket signer initialized");
        Ok(Self { wallet, chain_id })
    }

    /// Create from hex-encoded private key string
    pub fn from_hex(hex_key: &str, chain_id: u64) -> Result<Self> {
        let key = hex_key.strip_prefix("0x").unwrap_or(hex_key);
        let bytes = hex::decode(key).context("Invalid hex private key")?;
        Self::new(&bytes, chain_id)
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    /// Sign a Polymarket CLOB order
    /// Returns the signature as a hex string
    pub async fn sign_order(
        &self,
        salt: U256,
        maker: Address,
        signer: Address,
        taker: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        expiration: U256,
        nonce: U256,
        fee_rate_bps: U256,
        side: u8, // 0 = BUY, 1 = SELL
        signing_scheme: u8,
    ) -> Result<String> {
        // Polymarket uses a custom EIP-712 typed data structure
        // The domain and types are defined by the CTF Exchange contract
        let order_hash = self.compute_order_hash(
            salt, maker, signer, taker, token_id,
            maker_amount, taker_amount, expiration,
            nonce, fee_rate_bps, side, signing_scheme,
        )?;

        let signature = self.wallet
            .sign_hash(ethers::types::H256::from(order_hash))
            .context("Failed to sign order hash")?;

        Ok(format!("0x{}", signature))
    }

    fn compute_order_hash(
        &self,
        salt: U256,
        maker: Address,
        signer: Address,
        taker: Address,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        expiration: U256,
        nonce: U256,
        fee_rate_bps: U256,
        side: u8,
        signing_scheme: u8,
    ) -> Result<[u8; 32]> {
        use ethers::abi::{encode, Token};
        use sha2::{Digest, Sha256};
        use ethers::utils::keccak256;

        // ORDER_TYPEHASH
        let order_typehash = keccak256(
            b"Order(uint256 salt,address maker,address signer,address taker,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint256 expiration,uint256 nonce,uint256 feeRateBps,uint8 side,uint8 signingScheme)"
        );

        let encoded = encode(&[
            Token::FixedBytes(order_typehash.to_vec()),
            Token::Uint(salt),
            Token::Address(maker),
            Token::Address(signer),
            Token::Address(taker),
            Token::Uint(token_id),
            Token::Uint(maker_amount),
            Token::Uint(taker_amount),
            Token::Uint(expiration),
            Token::Uint(nonce),
            Token::Uint(fee_rate_bps),
            Token::Uint(U256::from(side)),
            Token::Uint(U256::from(signing_scheme)),
        ]);

        let struct_hash = keccak256(&encoded);

        // Domain separator for Polymarket CTF Exchange on Polygon
        let domain_separator = self.compute_domain_separator();

        // EIP-712 hash: keccak256("\x19\x01" || domainSeparator || structHash)
        let mut eip712_msg = Vec::with_capacity(66);
        eip712_msg.extend_from_slice(&[0x19, 0x01]);
        eip712_msg.extend_from_slice(&domain_separator);
        eip712_msg.extend_from_slice(&struct_hash);

        Ok(keccak256(&eip712_msg))
    }

    fn compute_domain_separator(&self) -> [u8; 32] {
        use ethers::abi::{encode, Token};
        use ethers::utils::keccak256;

        let domain_typehash = keccak256(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
        );

        // Polymarket CTF Exchange contract on Polygon
        let verifying_contract = Address::from_str(
            "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E"
        ).unwrap();

        let encoded = encode(&[
            Token::FixedBytes(domain_typehash.to_vec()),
            Token::FixedBytes(keccak256(b"Polymarket CTF Exchange").to_vec()),
            Token::FixedBytes(keccak256(b"1").to_vec()),
            Token::Uint(U256::from(self.chain_id)),
            Token::Address(verifying_contract),
        ]);

        keccak256(&encoded)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/crypto/eip712.rs
git commit -m "feat: add EIP-712 order signing for Polymarket CTF Exchange"
```

---

### Task 3: JWT Authentication for Kalshi

**Files:**
- Create: `src/crypto/jwt.rs`

- [ ] **Step 1: Write JWT/RSA auth module**

`src/crypto/jwt.rs`:
```rust
use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct KalshiClaims {
    sub: String,      // API key ID
    iat: u64,         // Issued at
    exp: u64,         // Expiration
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
            exp: now + 600, // 10 minutes
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
```

- [ ] **Step 2: Update main.rs module declarations**

Add to `src/main.rs`:
```rust
mod crypto;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/crypto/ src/main.rs
git commit -m "feat: add Kalshi JWT/RSA authentication module"
```
