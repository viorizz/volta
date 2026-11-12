//! Chiffrement des jetons Tesla au repos (AES-256-GCM).
//!
//! Protège les jetons dans la base et dans ses sauvegardes : un dump seul ne
//! suffit pas à les lire. La clé vit dans /opt/volta/.env, sur le même serveur :
//! cela ne protège donc pas contre une prise de contrôle complète du VPS.

use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::{anyhow, ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;

const PREFIX: &str = "v1:";

pub struct TokenCipher(Aes256Gcm);

impl TokenCipher {
    /// Clé de 32 octets encodée en base64 (`openssl rand -base64 32`).
    pub fn from_base64(key: &str) -> anyhow::Result<Self> {
        let bytes = STANDARD.decode(key.trim()).context("TOKEN_ENCRYPTION_KEY n'est pas du base64")?;
        ensure!(bytes.len() == 32, "TOKEN_ENCRYPTION_KEY doit faire 32 octets (openssl rand -base64 32)");
        Ok(Self(Aes256Gcm::new_from_slice(&bytes).map_err(|e| anyhow!("{e}"))?))
    }

    /// `aad` lie le chiffré à son usage (ex. "access_token") : on ne peut pas
    /// intervertir deux colonnes sans que le déchiffrement échoue.
    pub fn encrypt(&self, plain: &str, aad: &str) -> anyhow::Result<String> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .0
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plain.as_bytes(), aad: aad.as_bytes() })
            .map_err(|_| anyhow!("échec du chiffrement"))?;
        let mut out = nonce.to_vec();
        out.extend(ct);
        Ok(format!("{PREFIX}{}", STANDARD.encode(out)))
    }

    pub fn decrypt(&self, stored: &str, aad: &str) -> anyhow::Result<String> {
        let b64 = stored.strip_prefix(PREFIX).ok_or_else(|| anyhow!("jeton non chiffré"))?;
        let raw = STANDARD.decode(b64).context("jeton chiffré illisible")?;
        ensure!(raw.len() > 12, "jeton chiffré trop court");
        let (nonce, ct) = raw.split_at(12);
        let plain = self
            .0
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: aad.as_bytes() })
            .map_err(|_| anyhow!("déchiffrement impossible : clé TOKEN_ENCRYPTION_KEY changée ? Reconnecte le compte Tesla"))?;
        Ok(String::from_utf8(plain)?)
    }

    pub fn is_encrypted(stored: &str) -> bool {
        stored.starts_with(PREFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> TokenCipher {
        TokenCipher::from_base64(&STANDARD.encode([7u8; 32])).unwrap()
    }

    #[test]
    fn roundtrip_and_random_nonce() {
        let c = cipher();
        let a = c.encrypt("secret-token", "access_token").unwrap();
        let b = c.encrypt("secret-token", "access_token").unwrap();
        assert_ne!(a, b);
        assert!(!a.contains("secret-token"));
        assert_eq!(c.decrypt(&a, "access_token").unwrap(), "secret-token");
    }

    #[test]
    fn wrong_context_tamper_or_key_fail() {
        let c = cipher();
        let a = c.encrypt("secret-token", "access_token").unwrap();
        assert!(c.decrypt(&a, "refresh_token").is_err());
        let mut raw = STANDARD.decode(&a[3..]).unwrap();
        raw[20] ^= 1;
        assert!(c.decrypt(&format!("v1:{}", STANDARD.encode(raw)), "access_token").is_err());
        let other = TokenCipher::from_base64(&STANDARD.encode([8u8; 32])).unwrap();
        assert!(other.decrypt(&a, "access_token").is_err());
    }

    #[test]
    fn rejects_bad_keys() {
        assert!(TokenCipher::from_base64("pas-du-base64!").is_err());
        assert!(TokenCipher::from_base64(&STANDARD.encode([1u8; 16])).is_err());
    }
}
