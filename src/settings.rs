//! Non-secret UI preferences, stored as plain JSON next to (not inside) the
//! vault so the lock screen can be themed before anything is decrypted.

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
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

/// Where SeePass keeps its files. Per-user, so no administrator rights are needed
/// (7.x hard-coded `C:\PasswordManager\` and demanded admin to write there).
///
/// * Windows: `%APPDATA%\SeePass\`
/// * macOS: `~/Library/Application Support/SeePass/`
/// * Linux: `$XDG_DATA_HOME/seepass/` and `$XDG_CONFIG_HOME/seepass/`
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub default_vault: PathBuf,
    pub settings: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        if let Ok(dir) = std::env::var("SEEPASS_HOME") {
            let dir = PathBuf::from(dir);
            return Self { default_vault: dir.join("vault.seepass"), settings: dir.join("settings.json") };
        }
        match ProjectDirs::from("", "", "SeePass") {
            Some(d) => Self {
                default_vault: d.data_dir().join("vault.seepass"),
                settings: d.config_dir().join("settings.json"),
            },
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
