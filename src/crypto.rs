//! On-disk vault encryption.
//!
//! File layout (all integers little-endian):
//!
//! ```text
//! offset len  field
//! 0      8    magic  "SEEPASS\0"
//! 8      2    format version (currently 1)
//! 10     4    argon2id memory cost, KiB
//! 14     4    argon2id time cost (passes)
//! 18     4    argon2id parallelism
//! 22     16   argon2id salt
//! 38     24   XChaCha20-Poly1305 nonce
//! 62     ..   ciphertext || 16-byte Poly1305 tag
//! ```
//!
//! The whole 62-byte header is passed as associated data, so tampering with
//! the KDF parameters, salt or version is detected exactly like tampering with
//! the ciphertext.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

pub const MAGIC: &[u8; 8] = b"SEEPASS\0";
pub const FORMAT_VERSION: u16 = 1;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;
pub const HEADER_LEN: usize = 8 + 2 + 4 * 3 + SALT_LEN + NONCE_LEN;

/// Upper bounds accepted when *reading* a header, so a crafted file cannot
/// make us allocate gigabytes or spin forever before the tag is checked.
const MAX_M_COST_KIB: u32 = 2 * 1024 * 1024;
const MAX_T_COST: u32 = 64;
const MAX_P_COST: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    /// ~64 MiB, 3 passes: roughly 0.3-0.8 s on a typical desktop.
    pub const STANDARD: KdfParams = KdfParams { m_cost_kib: 64 * 1024, t_cost: 3, p_cost: 1 };
    /// ~256 MiB, 4 passes: for people who don't mind a couple of seconds at unlock.
    pub const STRONG: KdfParams = KdfParams { m_cost_kib: 256 * 1024, t_cost: 4, p_cost: 1 };
    /// Only for tests: the cheapest parameters argon2 accepts.
    #[doc(hidden)]
    pub const INSECURE_FAST: KdfParams = KdfParams { m_cost_kib: 8, t_cost: 1, p_cost: 1 };

    fn validate(self) -> Result<()> {
        if self.m_cost_kib > MAX_M_COST_KIB || self.t_cost > MAX_T_COST || self.p_cost > MAX_P_COST {
            return Err(Error::BadKdfParams);
        }
        Ok(())
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// A derived 256-bit vault key together with the salt/params it came from.
/// The key bytes are wiped from memory when this is dropped.
pub struct MasterKey {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; SALT_LEN],
    params: KdfParams,
}

impl MasterKey {
    /// Derive a key for a brand-new vault (fresh random salt).
    pub fn derive_new(password: &str, params: KdfParams) -> Result<Self> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        Self::derive(password, salt, params)
    }

    fn derive(password: &str, salt: [u8; SALT_LEN], params: KdfParams) -> Result<Self> {
        params.validate()?;
        let argon_params = Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(32))
            .map_err(|e| Error::Kdf(e.to_string()))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
        let mut key = Zeroizing::new([0u8; 32]);
        argon.hash_password_into(password.as_bytes(), &salt, key.as_mut()).map_err(|e| Error::Kdf(e.to_string()))?;
        Ok(Self { key, salt, params })
    }

    pub fn params(&self) -> KdfParams {
        self.params
    }

    fn header(&self, nonce: &[u8; NONCE_LEN]) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[0..8].copy_from_slice(MAGIC);
        h[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        h[10..14].copy_from_slice(&self.params.m_cost_kib.to_le_bytes());
        h[14..18].copy_from_slice(&self.params.t_cost.to_le_bytes());
        h[18..22].copy_from_slice(&self.params.p_cost.to_le_bytes());
        h[22..38].copy_from_slice(&self.salt);
        h[38..62].copy_from_slice(nonce);
        h
    }

    /// Encrypt `plaintext` into a complete vault file. A fresh random nonce is
    /// used for every call, which is safe with XChaCha20's 192-bit nonces.
    pub fn seal(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let header = self.header(&nonce);
        let cipher = XChaCha20Poly1305::new(self.key.as_ref().into());
        let ct = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: &header })
            .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory buffers");
        let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(&ct);
        out
    }
}

struct Header {
    params: KdfParams,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
}

fn parse_header(file: &[u8]) -> Result<Header> {
    if file.len() < 8 || &file[0..8] != MAGIC {
        return Err(Error::BadMagic);
    }
    if file.len() < HEADER_LEN + 16 {
        return Err(Error::Truncated);
    }
    let version = u16::from_le_bytes([file[8], file[9]]);
    if version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let u32_at = |o: usize| u32::from_le_bytes(file[o..o + 4].try_into().unwrap());
    let params = KdfParams { m_cost_kib: u32_at(10), t_cost: u32_at(14), p_cost: u32_at(18) };
    params.validate()?;
    Ok(Header { params, salt: file[22..38].try_into().unwrap(), nonce: file[38..62].try_into().unwrap() })
}

/// Derive the key from `password` and decrypt a vault file.
pub fn open(password: &str, file: &[u8]) -> Result<(MasterKey, Zeroizing<Vec<u8>>)> {
    let header = parse_header(file)?;
    let key = MasterKey::derive(password, header.salt, header.params)?;
    let cipher = XChaCha20Poly1305::new(key.key.as_ref().into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&header.nonce), Payload { msg: &file[HEADER_LEN..], aad: &file[..HEADER_LEN] })
        .map_err(|_| Error::Decrypt)?;
    Ok((key, Zeroizing::new(plaintext)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: KdfParams = KdfParams::INSECURE_FAST;

    #[test]
    fn roundtrip() {
        let key = MasterKey::derive_new("correct horse", P).unwrap();
        let file = key.seal(b"hello vault");
        let (_, pt) = open("correct horse", &file).unwrap();
        assert_eq!(&pt[..], b"hello vault");
    }

    #[test]
    fn nonces_differ_between_saves() {
        let key = MasterKey::derive_new("pw", P).unwrap();
        assert_ne!(key.seal(b"x"), key.seal(b"x"));
    }

    #[test]
    fn wrong_password_rejected() {
        let file = MasterKey::derive_new("right", P).unwrap().seal(b"data");
        assert!(matches!(open("wrong", &file), Err(Error::Decrypt)));
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let mut file = MasterKey::derive_new("pw", P).unwrap().seal(b"data");
        let last = file.len() - 1;
        file[last] ^= 1;
        assert!(matches!(open("pw", &file), Err(Error::Decrypt)));
    }

    #[test]
    fn tampered_header_rejected() {
        let mut file = MasterKey::derive_new("pw", P).unwrap().seal(b"data");
        file[40] ^= 1; // inside the nonce
        assert!(matches!(open("pw", &file), Err(Error::Decrypt)));
    }

    #[test]
    fn hostile_kdf_params_rejected_before_derivation() {
        let mut file = MasterKey::derive_new("pw", P).unwrap().seal(b"data");
        file[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(open("pw", &file), Err(Error::BadKdfParams)));
    }

    #[test]
    fn garbage_rejected() {
        assert!(matches!(open("pw", b"not a vault at all"), Err(Error::BadMagic)));
        assert!(matches!(open("pw", MAGIC), Err(Error::Truncated)));
    }
}
