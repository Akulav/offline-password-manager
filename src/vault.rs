//! The unlocked vault: entries in memory plus the key needed to write them back.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{self, KdfParams, MasterKey};
use crate::error::{Error, Result};

// Defaults are per field: a container-level `#[serde(default)]` would move
// fields out of a `Drop` type, which zeroize-on-drop forbids.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub notes: String,
    /// Unix seconds.
    #[serde(default)]
    pub created: u64,
    /// Unix seconds; used to resolve conflicts when merging backups.
    #[serde(default)]
    pub updated: u64,
}

impl Entry {
    pub fn new() -> Self {
        // No `..Default::default()`: Entry implements Drop (zeroize-on-drop).
        let mut e = Self::default();
        e.id = new_id();
        e.created = now();
        e.updated = e.created;
        e
    }

    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        [&self.title, &self.username, &self.url, &self.notes].iter().any(|f| f.to_lowercase().contains(&q))
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct VaultData {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
}

pub struct Vault {
    path: PathBuf,
    key: MasterKey,
    data: VaultData,
}

impl Vault {
    /// Create a new, empty vault at `path`. Refuses to overwrite an existing file.
    pub fn create(path: impl Into<PathBuf>, password: &str, params: KdfParams) -> Result<Self> {
        let path = path.into();
        if path.exists() {
            return Err(Error::AlreadyExists(path.display().to_string()));
        }
        let vault = Self { path, key: MasterKey::derive_new(password, params)?, data: VaultData::default() };
        vault.save()?;
        Ok(vault)
    }

    pub fn unlock(path: impl Into<PathBuf>, password: &str) -> Result<Self> {
        let path = path.into();
        let (key, data) = read_sealed(&path, password)?;
        Ok(Self { path, key, data })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kdf_params(&self) -> KdfParams {
        self.key.params()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.data.entries
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.data.entries.iter().find(|e| e.id == id)
    }

    /// Insert a new entry or replace the one with the same id, then persist.
    pub fn upsert(&mut self, mut entry: Entry) -> Result<()> {
        if entry.id.is_empty() {
            entry.id = new_id();
        }
        entry.updated = now();
        match self.data.entries.iter_mut().find(|e| e.id == entry.id) {
            Some(existing) => *existing = entry,
            None => self.data.entries.push(entry),
        }
        self.save()
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let before = self.data.entries.len();
        self.data.entries.retain(|e| e.id != id);
        let removed = self.data.entries.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Re-key the vault with a new master password (and a fresh salt).
    pub fn change_password(&mut self, new_password: &str, params: KdfParams) -> Result<()> {
        self.set_key(MasterKey::derive_new(new_password, params)?)
    }

    /// Re-key with a key derived elsewhere (e.g. on a worker thread).
    pub fn set_key(&mut self, key: MasterKey) -> Result<()> {
        self.key = key;
        self.save()
    }

    /// Check `password` against what is on disk without touching this vault.
    pub fn verify_password(&self, password: &str) -> Result<()> {
        read_sealed(&self.path, password).map(|_| ())
    }

    /// A copy of the entries, e.g. for exporting on a worker thread.
    pub fn snapshot(&self) -> VaultData {
        VaultData { entries: self.data.entries.clone() }
    }

    /// Write an encrypted backup with its own password. The backup is a normal
    /// vault file, so it can also be opened directly.
    pub fn export_backup(&self, dest: &Path, password: &str) -> Result<()> {
        write_backup(dest, password, self.key.params(), &self.data)
    }

    /// Read a backup (or any other vault file) for merging.
    pub fn read_backup(src: &Path, password: &str) -> Result<VaultData> {
        read_sealed(src, password).map(|(_, data)| data)
    }

    /// Merge entries from another vault. Unknown ids are added; for known ids
    /// the more recently updated copy wins.
    pub fn merge(&mut self, mut other: VaultData) -> Result<MergeReport> {
        let mut report = MergeReport::default();
        for incoming in other.entries.drain(..) {
            match self.data.entries.iter_mut().find(|e| e.id == incoming.id) {
                None => {
                    self.data.entries.push(incoming);
                    report.added += 1;
                }
                Some(existing) if incoming.updated > existing.updated => {
                    *existing = incoming;
                    report.updated += 1;
                }
                Some(_) => report.unchanged += 1,
            }
        }
        if report.added + report.updated > 0 {
            self.save()?;
        }
        Ok(report)
    }

    /// Add entries that came from somewhere other than a SeePass 8 vault
    /// (e.g. the legacy importer). Exact duplicates are skipped.
    pub fn import_entries(&mut self, entries: Vec<Entry>) -> Result<usize> {
        let mut added = 0;
        for e in entries {
            let dup = self
                .data
                .entries
                .iter()
                .any(|x| x.title == e.title && x.username == e.username && x.password == e.password);
            if !dup {
                self.data.entries.push(e);
                added += 1;
            }
        }
        if added > 0 {
            self.save()?;
        }
        Ok(added)
    }

    pub fn save(&self) -> Result<()> {
        let json = Zeroizing::new(serde_json::to_vec(&self.data)?);
        let sealed = self.key.seal(&json);
        if self.path.exists() {
            // Keep the previous generation around in case something goes wrong
            // between now and the next successful unlock.
            let _ = fs::copy(&self.path, backup_path(&self.path));
        }
        atomic_write(&self.path, &sealed)
    }
}

/// See [`Vault::export_backup`].
pub fn write_backup(dest: &Path, password: &str, params: KdfParams, data: &VaultData) -> Result<()> {
    let key = MasterKey::derive_new(password, params)?;
    let json = Zeroizing::new(serde_json::to_vec(data)?);
    atomic_write(dest, &key.seal(&json))
}

fn read_sealed(path: &Path, password: &str) -> Result<(MasterKey, VaultData)> {
    let bytes = fs::read(path)?;
    let (key, plaintext) = crypto::open(password, &bytes)?;
    let data: VaultData = serde_json::from_slice(&plaintext)?;
    Ok((key, data))
}

/// Path of the automatic previous-generation copy, `<vault>.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".bak");
    PathBuf::from(p)
}

/// Write to a sibling temp file, fsync, then rename over the destination so a
/// crash mid-write can never leave a half-written vault behind.
pub fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(format!(".tmp-{:08x}", OsRng.next_u32()));
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// Delete the vault and its backup. Because every byte on disk is encrypted
/// under a key derived from the master password, removing the files is enough;
/// overwriting them in place gives no real guarantee on SSDs or journaling
/// filesystems anyway.
pub fn destroy(path: &Path) -> Result<()> {
    for p in [path.to_path_buf(), backup_path(path)] {
        match fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: KdfParams = KdfParams::INSECURE_FAST;

    fn entry(title: &str, pw: &str) -> Entry {
        let mut e = Entry::new();
        e.title = title.into();
        e.username = "me".into();
        e.password = pw.into();
        e
    }

    #[test]
    fn create_add_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.seepass");
        let mut v = Vault::create(&path, "master", P).unwrap();
        v.upsert(entry("github", "s3cret")).unwrap();
        drop(v);

        let v = Vault::unlock(&path, "master").unwrap();
        assert_eq!(v.entries().len(), 1);
        assert_eq!(v.entries()[0].password, "s3cret");
        assert!(Vault::unlock(&path, "nope").is_err());
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.seepass");
        Vault::create(&path, "a", P).unwrap();
        assert!(matches!(Vault::create(&path, "b", P), Err(Error::AlreadyExists(_))));
    }

    #[test]
    fn plaintext_never_hits_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.seepass");
        let mut v = Vault::create(&path, "master", P).unwrap();
        v.upsert(entry("needle-title", "needle-password")).unwrap();
        for p in [path.clone(), backup_path(&path)] {
            let raw = fs::read(p).unwrap();
            assert!(!raw.windows(6).any(|w| w == b"needle"));
        }
    }

    #[test]
    fn upsert_edits_in_place_and_delete_removes() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::create(dir.path().join("v"), "m", P).unwrap();
        let mut e = entry("a", "1");
        v.upsert(e.clone()).unwrap();
        e.password = "2".into();
        v.upsert(e.clone()).unwrap();
        assert_eq!(v.entries().len(), 1);
        assert_eq!(v.get(&e.id).unwrap().password, "2");
        assert!(v.delete(&e.id).unwrap());
        assert!(!v.delete(&e.id).unwrap());
        assert!(v.entries().is_empty());
    }

    #[test]
    fn change_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v");
        let mut v = Vault::create(&path, "old", P).unwrap();
        v.upsert(entry("x", "y")).unwrap();
        v.change_password("new", P).unwrap();
        assert!(Vault::unlock(&path, "old").is_err());
        assert_eq!(Vault::unlock(&path, "new").unwrap().entries().len(), 1);
    }

    #[test]
    fn backup_roundtrip_and_merge() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = Vault::create(dir.path().join("a"), "pa", P).unwrap();
        let shared = entry("shared", "v1");
        a.upsert(shared.clone()).unwrap();
        a.upsert(entry("only-a", "x")).unwrap();
        let backup = dir.path().join("backup.seepass");
        a.export_backup(&backup, "backup-pw").unwrap();
        assert!(Vault::read_backup(&backup, "pa").is_err());

        let mut b = Vault::create(dir.path().join("b"), "pb", P).unwrap();
        b.upsert(entry("only-b", "z")).unwrap();
        let data = Vault::read_backup(&backup, "backup-pw").unwrap();
        let r = b.merge(data).unwrap();
        assert_eq!(r, MergeReport { added: 2, updated: 0, unchanged: 0 });
        assert_eq!(b.entries().len(), 3);

        // Merging the same backup again changes nothing.
        let r = b.merge(Vault::read_backup(&backup, "backup-pw").unwrap()).unwrap();
        assert_eq!(r, MergeReport { added: 0, updated: 0, unchanged: 2 });
    }

    #[test]
    fn merge_prefers_newer() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::create(dir.path().join("v"), "m", P).unwrap();
        let mut e = entry("site", "old");
        v.upsert(e.clone()).unwrap();
        e.password = "new".into();
        e.updated = u64::MAX;
        let r = v.merge(VaultData { entries: vec![e.clone()] }).unwrap();
        assert_eq!(r.updated, 1);
        assert_eq!(v.get(&e.id).unwrap().password, "new");
    }

    #[test]
    fn search() {
        let mut e = entry("GitHub", "x");
        e.url = "https://github.com".into();
        assert!(e.matches("git"));
        assert!(e.matches("HUB"));
        assert!(e.matches(""));
        assert!(!e.matches("gitlab"));
        assert!(!e.matches("x"), "passwords must not be searchable");
    }

    #[test]
    fn destroy_removes_vault_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v");
        let mut v = Vault::create(&path, "m", P).unwrap();
        v.upsert(entry("a", "b")).unwrap();
        assert!(backup_path(&path).exists());
        destroy(&path).unwrap();
        assert!(!path.exists() && !backup_path(&path).exists());
        destroy(&path).unwrap();
    }
}
