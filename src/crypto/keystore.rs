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

/// Derive a 256-bit key from passphrase + salt using SHA-256
fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(passphrase.as_bytes());
    hasher.update(salt);
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}
