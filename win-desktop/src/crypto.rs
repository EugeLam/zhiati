use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::{Aead, generic_array::GenericArray};
use base64::{Engine, engine::general_purpose::STANDARD};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use rand::Rng;
use std::fs;
use std::path::PathBuf;
use crate::config::config_dir;

const SALT_FILENAME: &str = "crypto_salt.bin";
const PBKDF2_ITERATIONS: u32 = 100_000;
const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;

fn salt_path() -> PathBuf {
    config_dir().join(SALT_FILENAME)
}

fn get_or_create_salt() -> Vec<u8> {
    let path = salt_path();
    if path.exists() {
        if let Ok(salt) = fs::read(&path) {
            if salt.len() == 16 {
                return salt;
            }
        }
    }
    let mut salt = vec![0u8; 16];
    rand::thread_rng().fill(&mut salt[..]);
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::write(&path, &salt);
    salt
}

fn derive_machine_key() -> [u8; KEY_SIZE] {
    let salt = get_or_create_salt();
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string());
    let password = format!("zhiati-local-{}", hostname);
    let mut key = [0u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        &salt,
        PBKDF2_ITERATIONS,
        &mut key,
    );
    key
}

pub fn encrypt_password(password: &str) -> String {
    let key = derive_machine_key();
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
    let nonce_bytes: [u8; NONCE_SIZE] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, password.as_bytes().as_ref())
        .expect("Encryption failed");
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    STANDARD.encode(&combined)
}

pub fn decrypt_password(encrypted_b64: &str) -> Result<String, String> {
    let key = derive_machine_key();
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
    let combined = STANDARD.decode(encrypted_b64)
        .map_err(|e| format!("解密失败: base64 解码错误: {}", e))?;
    if combined.len() < NONCE_SIZE + 1 {
        return Err("解密失败: 密文太短".to_string());
    }
    let nonce = Nonce::from_slice(&combined[..NONCE_SIZE]);
    let ciphertext = &combined[NONCE_SIZE..];
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| format!("解密失败: 密码错误或密钥变更: {}", e))?;
    String::from_utf8(plaintext)
        .map_err(|e| format!("解密失败: UTF-8 解码错误: {}", e))
}
