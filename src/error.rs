use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wrong master password, or the vault file is corrupted")]
    Decrypt,
    #[error("this file is not a SeePass vault")]
    BadMagic,
    #[error("vault format version {0} is newer than this SeePass build supports")]
    UnsupportedVersion(u16),
    #[error("the vault file is truncated")]
    Truncated,
    #[error("the vault header requests unreasonable key-derivation parameters")]
    BadKdfParams,
    #[error("a vault already exists at {0}")]
    AlreadyExists(String),
    #[error("key derivation failed: {0}")]
    Kdf(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("vault data is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("legacy import failed: {0}")]
    Legacy(String),
}

pub type Result<T> = std::result::Result<T, Error>;
