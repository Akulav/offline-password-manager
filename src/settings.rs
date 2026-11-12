//! Non-secret UI preferences, stored as plain JSON next to (not inside) the
//! vault so the lock screen can be themed before anything is decrypted.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::vault::atomic_write;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark_mode: bool,
    /// Lock after this many seconds without keyboard/mouse input. 0 = never.
    pub auto_lock_secs: u64,
    /// Clear a copied secret from the clipboard after this many seconds. 0 = never.
    pub clipboard_clear_secs: u64,
    /// Overrides the default vault location.
    pub vault_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { dark_mode: true, auto_lock_secs: 300, clipboard_clear_secs: 20, vault_path: None }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> crate::Result<()> {
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// Where SeePass keeps its files: **the folder it is installed in** (next to the program when not
/// installed), never the OS's per-user app-data folders. See [`crate::install`].
///
/// One exception keeps nobody's vault out of reach: a copy that is *not installed* and has no vault
/// next to it keeps opening a vault from before 8.2 in the old per-user folder. Installing SeePass
/// moves that vault into the install folder.
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub default_vault: PathBuf,
    pub settings: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let root = crate::install::data_root();
        if std::env::var_os(crate::install::ENV_DATA_DIR).is_none()
            && !crate::install::running_installed()
            && root.as_ref().is_none_or(|r| !r.join("vault.seepass").exists())
        {
            let legacy = crate::install::legacy_files();
            let find = |name: &str| legacy.iter().find(|(_, n)| *n == name).map(|(p, _)| p.clone());
            if let (Some(vault), Some(settings)) = (find("vault.seepass"), find("settings.json")) {
                if vault.is_file() {
                    return Self { default_vault: vault, settings };
                }
            }
        }
        match root {
            Some(r) => Self { default_vault: r.join("vault.seepass"), settings: r.join("settings.json") },
            None => Self { default_vault: "vault.seepass".into(), settings: "seepass-settings.json".into() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        let s = Settings { dark_mode: false, auto_lock_secs: 60, ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
