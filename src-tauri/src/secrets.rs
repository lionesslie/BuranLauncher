use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;

pub struct SecretBox {
    key: [u8; 32],
}

impl SecretBox {
    pub fn load_or_create(path: PathBuf) -> Self {
        let key = fs::read(&path)
            .ok()
            .filter(|bytes| bytes.len() == 32)
            .map(|bytes| {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                key
            })
            .unwrap_or_else(|| {
                let mut key = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, key);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
                }
                key
            });
        SecretBox { key }
    }

    pub fn encrypt(&self, plaintext: &str) -> String {
        let cipher = Aes256Gcm::new_from_slice(&self.key).expect("AES anahtarı geçersiz");
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        match cipher.encrypt(Nonce::from_slice(nonce.as_slice()), plaintext.as_bytes()) {
            Ok(ciphertext) => {
                let mut combined = Vec::with_capacity(12 + ciphertext.len());
                combined.extend_from_slice(nonce.as_slice());
                combined.extend_from_slice(&ciphertext);
                B64.encode(combined)
            }
            Err(_) => String::new(),
        }
    }

    pub fn decrypt(&self, encoded: &str) -> Option<String> {
        let bytes = B64.decode(encoded).ok()?;
        if bytes.len() < 13 {
            return None;
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.key).ok()?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .ok()?;
        String::from_utf8(plaintext).ok()
    }
}