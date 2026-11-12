//! Import from SeePass 7.x (the C# / WinForms / SQLite version).
//!
//! 7.x stored `settings.config` (Newtonsoft JSON) and `data.db` (SQLite, table
//! `data(username, pass, domain)`) in `C:\PasswordManager\`, and its "Export"
//! button zipped those two files into `data.zip`. Either form is accepted here.
//!
//! Reproducing its crypto exactly:
//!
//! * `FinalKey(pass + user, pass, PIM)`: start from `pass + user + pass`, then
//!   PIM times replace it with `BitConverter.ToString(SHA256(ASCII(s)))`
//!   (upper-case hex pairs joined by `-`). `Encoding.ASCII` turns every UTF-16
//!   code unit above 0x7F into `?`.
//! * Each field is `Base64(AES-256-CBC(UTF-16LE(text)))` with key and IV both
//!   taken from one PBKDF2-HMAC-SHA1 stream (1000 iterations): bytes 0..32 are
//!   the key, 32..48 the IV. The PBKDF2 password is `UTF-8(FinalKey)`.
//! * The PBKDF2 salt is `Encoding.Default.GetBytes(settings.encryptVector)`,
//!   where `encryptVector` was itself produced by `Encoding.Default.GetString`
//!   of 16 random bytes. `Encoding.Default` is the machine's ANSI code page, so
//!   we try the common ones and keep whichever decrypts.
//! * Login was verified only through the `pim` hash chain; the stored
//!   username/password hashes were never actually compared (and were written
//!   into each other's fields), so they are ignored here too.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use aes::Aes256;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::vault::Entry;

/// Where 7.x always kept its data.
pub const DEFAULT_LEGACY_DIR: &str = r"C:\PasswordManager";

#[derive(Debug, Deserialize)]
struct LegacySettings {
    pim: Option<String>,
    pim_salt: Option<String>,
    #[serde(rename = "encryptVector")]
    encrypt_vector: Option<String>,
    #[serde(rename = "userFlag", default)]
    user_flag: bool,
}

/// Raw legacy files, loaded from a folder or an exported `data.zip`.
pub struct LegacySource {
    settings: LegacySettings,
    db: Vec<u8>,
}

impl LegacySource {
    /// `path` may be the 7.x data folder or the `data.zip` it exported.
    pub fn load(path: &Path) -> Result<Self> {
        let (settings, db) = if path.is_dir() {
            let s = fs::read(path.join("settings.config"))
                .map_err(|e| legacy(format!("cannot read settings.config: {e}")))?;
            let d = fs::read(path.join("data.db")).map_err(|e| legacy(format!("cannot read data.db: {e}")))?;
            (s, d)
        } else {
            let file = fs::File::open(path)?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| legacy(format!("not a valid zip: {e}")))?;
            (read_zip_entry(&mut zip, "settings.config")?, read_zip_entry(&mut zip, "data.db")?)
        };
        let settings = parse_settings(&settings)?;
        Ok(Self { settings, db })
    }

    /// Decrypt every stored login. Fails if the credentials are wrong.
    pub fn decrypt(&self, username: &str, password: &str, pim: u32) -> Result<Vec<Entry>> {
        if !self.settings.user_flag {
            return Err(legacy("this SeePass 7 folder has no account set up"));
        }
        if !self.verify_login(username, password, pim) {
            return Err(legacy("wrong username, password or PIM"));
        }
        let rows = read_rows(&self.db)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let vector = self
            .settings
            .encrypt_vector
            .as_deref()
            .filter(|v| *v != "default")
            .ok_or_else(|| legacy("vault has rows but no encryption vector"))?;

        let final_key = Zeroizing::new(final_key(password, username, pim));
        let (key, iv) = salt_candidates(vector)
            .into_iter()
            .map(|salt| derive_aes(&final_key, &salt))
            .find(|(k, iv)| decrypt_field(&rows[0].domain, k, iv).is_ok())
            .ok_or_else(|| legacy("could not determine the encryption vector's code page"))?;

        rows.iter()
            .map(|row| {
                let mut e = Entry::new();
                e.title = decrypt_field(&row.domain, &key, &iv)?.to_string();
                e.username = decrypt_field(&row.username, &key, &iv)?.to_string();
                e.password = decrypt_field(&row.pass, &key, &iv)?.to_string();
                e.notes = "Imported from SeePass 7".into();
                Ok(e)
            })
            .collect()
    }

    fn verify_login(&self, username: &str, password: &str, pim: u32) -> bool {
        let (Some(stored), Some(salt)) = (&self.settings.pim, &self.settings.pim_salt) else {
            return false;
        };
        let mut h = net_hash(&format!("{password}{username}{salt}"));
        for _ in 0..pim {
            h = net_hash(&format!("{h}{salt}"));
        }
        constant_time_eq(h.as_bytes(), stored.as_bytes())
    }
}

/// The folder 7.x used, if it still exists on this machine.
pub fn find_default() -> Option<PathBuf> {
    let p = PathBuf::from(DEFAULT_LEGACY_DIR);
    p.join("settings.config").exists().then_some(p)
}

struct Row {
    username: String,
    pass: String,
    domain: String,
}

fn read_rows(db: &[u8]) -> Result<Vec<Row>> {
    // rusqlite needs a real file; the database only holds ciphertext.
    let tmp = tempfile_path();
    fs::write(&tmp, db)?;
    let result = (|| -> rusqlite::Result<Vec<Row>> {
        let conn = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut stmt = conn.prepare("SELECT username, pass, domain FROM data")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Row {
                    username: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    pass: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    domain: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                })
            })?
            .collect();
        rows
    })();
    let _ = fs::remove_file(&tmp);
    result.map_err(|e| legacy(format!("cannot read data.db: {e}")))
}

fn tempfile_path() -> PathBuf {
    use rand::RngCore;
    // In SeePass's own folder (deleted right after reading), never the shared temp folder.
    let dir = crate::install::data_root().unwrap_or_else(std::env::temp_dir);
    let _ = fs::create_dir_all(&dir);
    dir.join(format!(".seepass-legacy-{:016x}.db", rand::rngs::OsRng.next_u64()))
}

fn read_zip_entry(zip: &mut zip::ZipArchive<fs::File>, name: &str) -> Result<Vec<u8>> {
    let mut f = zip.by_name(name).map_err(|_| legacy(format!("{name} is missing from the zip")))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

fn parse_settings(bytes: &[u8]) -> Result<LegacySettings> {
    // Newtonsoft may write a UTF-8 BOM.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice(bytes).map_err(|e| legacy(format!("settings.config is not valid: {e}")))
}

/// `Encoding.ASCII.GetBytes`: one byte per UTF-16 code unit, non-ASCII -> `?`.
fn net_ascii(s: &str) -> Vec<u8> {
    s.encode_utf16().map(|u| if u < 0x80 { u as u8 } else { b'?' }).collect()
}

/// `BitConverter.ToString(SHA256(ASCII(s)))`.
fn net_hash(s: &str) -> String {
    let digest = Sha256::digest(net_ascii(s));
    digest.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("-")
}

fn final_key(password: &str, username: &str, pim: u32) -> String {
    let mut s = format!("{password}{username}{password}");
    for _ in 0..pim {
        s = net_hash(&s);
    }
    s
}

fn derive_aes(final_key: &str, salt: &[u8]) -> (Zeroizing<[u8; 32]>, [u8; 16]) {
    let mut out = Zeroizing::new([0u8; 48]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(final_key.as_bytes(), salt, 1000, out.as_mut());
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&out[..32]);
    (key, out[32..48].try_into().unwrap())
}

fn decrypt_field(b64: &str, key: &[u8; 32], iv: &[u8; 16]) -> Result<Zeroizing<String>> {
    let bad = || legacy("a stored field did not decrypt (wrong code page or corrupted data)");
    let mut buf = Zeroizing::new(B64.decode(b64.replace(' ', "+")).map_err(|_| bad())?);
    let pt = cbc::Decryptor::<Aes256>::new(key.into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| bad())?;
    if pt.len() % 2 != 0 {
        return Err(bad());
    }
    let units: Vec<u16> = pt.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16(&units).map(Zeroizing::new).map_err(|_| bad())
}

/// Possible byte strings `Encoding.Default.GetBytes(vector)` produced on the
/// machine that wrote the vault.
fn salt_candidates(vector: &str) -> Vec<Vec<u8>> {
    use encoding_rs::*;
    let mut out: Vec<Vec<u8>> = Vec::new();
    for enc in [
        WINDOWS_1252,
        WINDOWS_1250,
        WINDOWS_1251,
        WINDOWS_1253,
        WINDOWS_1254,
        WINDOWS_1257,
        WINDOWS_1255,
        WINDOWS_1256,
        WINDOWS_1258,
        WINDOWS_874,
    ] {
        let (bytes, _, had_errors) = enc.encode(vector);
        if !had_errors && !out.iter().any(|b| *b == *bytes) {
            out.push(bytes.into_owned());
        }
    }
    // A machine with the "UTF-8 for worldwide language support" beta option.
    let utf8 = vector.as_bytes().to_vec();
    if !out.contains(&utf8) {
        out.push(utf8);
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn legacy(msg: impl Into<String>) -> Error {
    Error::Legacy(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_ascii_matches_dotnet() {
        assert_eq!(net_ascii("aé😀"), b"a???"); // 😀 is a surrogate pair -> two '?'
    }

    #[test]
    fn net_hash_format() {
        // SHA256("abc"), formatted like BitConverter.ToString.
        assert_eq!(
            net_hash("abc"),
            "BA-78-16-BF-8F-01-CF-EA-41-41-40-DE-5D-AE-22-23-B0-03-61-A3-96-17-7A-9C-B4-10-FF-61-F2-00-15-AD"
        );
    }

    #[test]
    fn final_key_zero_pim_is_concatenation() {
        assert_eq!(final_key("p", "u", 0), "pup");
    }
}
