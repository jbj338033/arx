use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

use crate::error::Error;

pub fn load_master_key(path: &str) -> Result<Vec<u8>, Error> {
    if let Ok(key) = std::env::var("ARX_MASTER_KEY") {
        return hex::decode(key)
            .map_err(|e| Error::Internal(format!("invalid ARX_MASTER_KEY hex: {e}")));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Internal(format!("failed to read master key from {path}: {e}")))?;
    let trimmed = String::from_utf8_lossy(&bytes);
    let trimmed = trimmed.trim();
    hex::decode(trimmed).map_err(|e| Error::Internal(format!("invalid master key hex: {e}")))
}

pub fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|e| Error::Internal(format!("invalid encryption key: {e}")))?;
    let less_safe = LessSafeKey::new(unbound);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| Error::Internal(format!("rng failed: {e}")))?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.to_vec();
    less_safe
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| Error::Internal(format!("encryption failed: {e}")))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    Ok(result)
}

pub fn decrypt(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
    if ciphertext.len() < 12 {
        return Err(Error::Internal("ciphertext too short".into()));
    }

    let (nonce_bytes, encrypted) = ciphertext.split_at(12);
    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(nonce_bytes);

    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|e| Error::Internal(format!("invalid decryption key: {e}")))?;
    let less_safe = LessSafeKey::new(unbound);

    let nonce = Nonce::assume_unique_for_key(nonce_arr);
    let mut in_out = encrypted.to_vec();
    let plaintext = less_safe
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| Error::Internal(format!("decryption failed: {e}")))?;

    Ok(plaintext.to_vec())
}
