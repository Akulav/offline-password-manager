//! Where SeePass lives: installing, updating and removing it (same model as Akulav Launcher).
//!
//! **One file per OS does both install and update.** SeePass never touches the network, so it
//! doesn't check for updates by itself: running a newer download *is* the update.
//!
//! * **Windows** — `SeePass-setup.exe` (Inno Setup, `installer/seepass.iss`). The first run asks for
//!   the install folder; Inno remembers it, so a newer setup installs into the same folder.
//! * **Linux / macOS** — the bare binary. Started from anywhere, it offers to install itself into a
//!   folder the user picks (remembered in `<config dir>/seepass/install-dir`); a newer download
//!   offers to update that folder.
//!
//! **Files go there.** The install folder holds the program *and* its files (the vault, its `.bak`,
//! settings). A [`MARKER`] file identifies an install folder. A vault and settings left in the
//! pre-8.2 per-user folders are moved in on the first start of the installed copy, across drives too.

use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "SeePass";
/// `.desktop` file, `~/.local/bin` command and the folder of the remembered install folder.
#[cfg_attr(windows, allow(dead_code))]
pub const APP_ID: &str = "seepass";
#[cfg_attr(windows, allow(dead_code))]
pub const EXE_NAME: &str = if cfg!(windows) { "SeePass.exe" } else { "seepass" };
#[cfg_attr(windows, allow(dead_code))]
const BUNDLE_ID: &str = "com.akulav.seepass";
#[cfg_attr(windows, allow(dead_code))]
const DESKTOP_KEYS: &str = "GenericName=Password Manager\nComment=Offline password manager - made by Akulav\nCategories=Utility;Security;\nKeywords=password;vault;security;\n";
#[cfg_attr(windows, allow(dead_code))]
const ICON_PNG: &[u8] = include_bytes!("../assets/seepass-256.png");
/// Present in every install folder; holds the installed version.
pub const MARKER: &str = ".seepass-install";
/// Everything SeePass keeps in its folder. Deleting data removes these and nothing else.
pub const DATA_ENTRIES: &[&str] = &["vault.seepass.bak", "vault.seepass", "settings.json"];
/// `SEEPASS_HOME` overrides the data folder, `SEEPASS_PORTABLE` keeps data next to the program.
pub const ENV_DATA_DIR: &str = "SEEPASS_HOME";
const ENV_PORTABLE: &str = "SEEPASS_PORTABLE";

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Files of versions before 8.2, in the per-user folders: (file, name in the install folder).
pub fn legacy_files() -> Vec<(PathBuf, &'static str)> {
    let Some(d) = directories::ProjectDirs::from("", "", "SeePass") else { return Vec::new() };
    vec![
        (d.data_dir().join("vault.seepass"), "vault.seepass"),
        (d.data_dir().join("vault.seepass.bak"), "vault.seepass.bak"),
        (d.config_dir().join("settings.json"), "settings.json"),
    ]
}

fn legacy_dirs() -> Vec<PathBuf> {
    let Some(d) = directories::ProjectDirs::from("", "", "SeePass") else { return Vec::new() };
    vec![d.data_dir().to_path_buf(), d.config_dir().to_path_buf()]
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

#[cfg_attr(not(unix), allow(dead_code))]
fn config_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.config_dir().to_path_buf())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn data_local_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.data_local_dir().to_path_buf())
}

// ---------------------------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------------------------

/// Folder a program belongs to: its own folder, or for `X.app/Contents/MacOS/bin` the folder that
/// holds `X.app`.
pub fn root_of_exe(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let bundle = dir
        .parent()
        .filter(|_| dir.file_name().is_some_and(|n| n == "MacOS"))
        .filter(|contents| contents.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|b| b.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(b) => b.parent().map(Path::to_path_buf),
        None => Some(dir.to_path_buf()),
    }
}

pub fn program_root() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| root_of_exe(&e))
}

pub fn is_install_root(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

pub fn installed_version(root: &Path) -> Option<String> {
    let v = std::fs::read_to_string(root.join(MARKER)).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

fn write_marker(root: &Path) -> std::io::Result<()> {
    std::fs::write(root.join(MARKER), CURRENT_VERSION)
}

/// The install folder: the running copy's, otherwise the remembered one (Linux/macOS).
pub fn installed_root() -> Option<PathBuf> {
    program_root().filter(|r| is_install_root(r)).or_else(|| platform::remembered_root().filter(|r| is_install_root(r)))
}

/// `portable.txt` next to the program, `--portable` or `SEEPASS_PORTABLE`: data next to it.
pub fn portable() -> bool {
    std::env::var_os(ENV_PORTABLE).is_some() || program_root().is_some_and(|r| r.join("portable.txt").exists())
}

/// The data folder: the folder SeePass lives in (the install folder when installed, else next to
/// the program). `None` only when `SEEPASS_HOME` is not set and the program's path is unknown.
pub fn data_root() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os(ENV_DATA_DIR).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    if portable() {
        return program_root();
    }
    installed_root().or_else(program_root)
}

/// True for an installed copy (it never uses the pre-8.2 folders).
pub fn running_installed() -> bool {
    program_root().is_some_and(|r| is_install_root(&r))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (canon(a), canon(b));
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Check a folder typed into the install prompt. `~` expands to the home folder.
pub fn parse_install_dir(text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Choose a folder.".into());
    }
    let path = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            home_dir().ok_or("Could not find your home folder.")?.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    };
    if !path.is_absolute() {
        return Err("Use a full path, for example ~/Apps/SeePass.".into());
    }
    if path.parent().is_none() {
        return Err("Pick a folder, not the root of a drive.".into());
    }
    if path.is_file() {
        return Err("That's a file, not a folder.".into());
    }
    Ok(path)
}

/// Compare dotted versions numerically ("10.0.0" > "9.0.0").
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches(['v', 'V']).split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Moving the old vault into the install folder
// ---------------------------------------------------------------------------------------------

/// Move a file: a rename, or across drives a copy (checked to be complete) and then delete. A
/// failed copy leaves the original untouched.
pub fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let copied = std::fs::copy(src, dst)?;
    if copied != std::fs::metadata(src)?.len() || std::fs::read(src)? != std::fs::read(dst)? {
        let _ = std::fs::remove_file(dst);
        return Err(std::io::Error::other("the copy is incomplete"));
    }
    std::fs::remove_file(src)
}

/// Vault, backup and settings of versions before 8.2 → the install folder.
fn migrate_legacy_data(root: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    for (src, name) in legacy_files() {
        let dst = root.join(name);
        if !src.is_file() || dst.exists() || same_path(&src, &dst) {
            continue;
        }
        match move_file(&src, &dst) {
            Ok(()) => notes.push(format!("moved {} to {}", src.display(), dst.display())),
            Err(e) => notes.push(format!("could not move {} ({e}); it was left there", src.display())),
        }
    }
    for d in legacy_dirs() {
        let _ = std::fs::remove_dir(d); // only when nothing else is left
    }
    notes
}

// ---------------------------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
pub enum Offer {
    #[default]
    None,
    Install {
        suggested: PathBuf,
    },
    Update {
        root: PathBuf,
        installed: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct StartupInfo {
    pub installed: bool,
    pub offer: Offer,
    pub notes: Vec<String>,
}

pub enum Startup {
    Run(StartupInfo),
    Exit,
}

/// Flags this module handles.
pub fn is_install_flag(arg: &str) -> bool {
    matches!(arg, "--portable" | "--here" | "--installed" | "--uninstall")
}

pub fn on_startup(args: &[String]) -> Startup {
    let flag = |f: &str| args.iter().any(|a| a == f);
    if flag("--portable") {
        std::env::set_var(ENV_PORTABLE, "1");
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
    if flag("--uninstall") {
        platform::uninstall_interactive();
        return Startup::Exit;
    }
    let mut info = StartupInfo { installed: flag("--installed"), ..Default::default() };
    if let Some(root) = program_root().filter(|r| is_install_root(r)) {
        platform::on_installed_start(&root);
        if std::env::var_os(ENV_DATA_DIR).is_none() {
            info.notes = migrate_legacy_data(&root);
        }
    } else if !cfg!(debug_assertions) && !flag("--here") && !portable() {
        info.offer = platform::startup_offer();
    }
    Startup::Run(info)
}

/// Install this binary into `root` (or update the copy there), then start it with `args`.
/// On success the caller must exit. Windows installs with the setup instead.
pub fn install_and_launch(root: &Path, args: &[&str]) -> Result<(), String> {
    platform::install_and_launch(root, args)
}

// ---------------------------------------------------------------------------------------------
// Windows: Inno Setup
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use super::{installed_root, Offer, CURRENT_VERSION};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub fn remembered_root() -> Option<PathBuf> {
        None
    }

    pub fn startup_offer() -> Offer {
        Offer::None
    }

    pub fn on_installed_start(root: &Path) {
        if super::installed_version(root).as_deref() != Some(CURRENT_VERSION) {
            let _ = super::write_marker(root);
        }
    }

    pub fn install_and_launch(_root: &Path, _args: &[&str]) -> Result<(), String> {
        Err("On Windows, install with SeePass-setup.exe.".into())
    }

    pub fn uninstall_interactive() {
        if let Some(u) = installed_root().map(|r| r.join("unins000.exe")).filter(|u| u.is_file()) {
            let _ = Command::new(u).spawn();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Linux / macOS: the binary installs itself
// ---------------------------------------------------------------------------------------------

#[cfg(unix)]
mod platform {
    use super::{
        installed_root, installed_version, is_newer, same_path, write_marker, Offer, APP_ID, APP_NAME, CURRENT_VERSION,
        EXE_NAME,
    };
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn pointer_file() -> Option<PathBuf> {
        super::config_dir().map(|d| d.join(APP_ID).join("install-dir"))
    }

    pub fn remembered_root() -> Option<PathBuf> {
        let text = std::fs::read_to_string(pointer_file()?).ok()?;
        let dir = text.trim();
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    }

    fn remember(root: &Path) {
        if let Some(file) = pointer_file() {
            if let Some(d) = file.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&file, root.to_string_lossy().as_bytes());
        }
    }

    fn forget() {
        if let Some(f) = pointer_file() {
            let _ = std::fs::remove_file(&f);
            if let Some(d) = f.parent() {
                let _ = std::fs::remove_dir(d);
            }
        }
    }

    pub fn default_install_dir() -> PathBuf {
        let home = super::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        if cfg!(target_os = "macos") {
            home.join("Applications").join(APP_NAME)
        } else {
            super::data_local_dir().unwrap_or_else(|| home.join(".local/share")).join(APP_NAME)
        }
    }

    fn bundle(root: &Path) -> PathBuf {
        root.join(format!("{APP_NAME}.app"))
    }

    fn installed_bin(root: &Path) -> PathBuf {
        if cfg!(target_os = "macos") {
            bundle(root).join("Contents/MacOS").join(EXE_NAME)
        } else {
            root.join(EXE_NAME)
        }
    }

    pub fn startup_offer() -> Offer {
        match installed_root() {
            Some(root) => {
                let installed = installed_version(&root);
                let missing = !installed_bin(&root).is_file();
                if missing || installed.as_deref().is_none_or(|v| is_newer(CURRENT_VERSION, v)) {
                    Offer::Update { root, installed }
                } else {
                    Offer::None
                }
            }
            None => Offer::Install { suggested: default_install_dir() },
        }
    }

    pub fn on_installed_start(root: &Path) {
        if installed_version(root).as_deref() != Some(CURRENT_VERSION) {
            let _ = write_marker(root);
        }
        if remembered_root().is_none_or(|r| !same_path(&r, root)) {
            remember(root);
        }
    }

    /// Replace `target` with a copy of this program; a running copy keeps its old file (`.old`).
    fn copy_self(target: &Path) -> Result<(), String> {
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        if same_path(&me, target) {
            return Ok(());
        }
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let staged = target.with_extension("new");
        std::fs::copy(&me, &staged).map_err(|e| format!("could not copy SeePass to {}: {e}", target.display()))?;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        let old = target.with_extension("old");
        let _ = std::fs::remove_file(&old);
        let had = target.exists();
        if had {
            std::fs::rename(target, &old).map_err(|e| e.to_string())?;
        }
        if let Err(e) = std::fs::rename(&staged, target) {
            if had {
                let _ = std::fs::rename(&old, target);
            }
            return Err(e.to_string());
        }
        Ok(())
    }

    fn install_to(root: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(root).map_err(|e| format!("could not create {}: {e}", root.display()))?;
        let bin = installed_bin(root);
        copy_self(&bin)?;
        if cfg!(target_os = "macos") {
            write_bundle(root)?;
        } else {
            integrate(&bin);
        }
        write_marker(root).map_err(|e| e.to_string())?;
        remember(root);
        Ok(bin)
    }

    pub fn install_and_launch(root: &Path, args: &[&str]) -> Result<(), String> {
        let bin = install_to(root)?;
        let mut cmd = if cfg!(target_os = "macos") {
            let mut c = Command::new("/usr/bin/open");
            c.arg("-n").arg(bundle(root));
            if !args.is_empty() {
                c.arg("--args");
            }
            c
        } else {
            Command::new(&bin)
        };
        cmd.args(args).current_dir(root);
        cmd.spawn().map(|_| ()).map_err(|e| format!("installed, but could not start it: {e}"))
    }

    fn uninstall(delete_data: bool) -> Result<(), String> {
        let root = installed_root().ok_or("SeePass is not installed")?;
        if cfg!(target_os = "macos") {
            let _ = std::fs::remove_dir_all(bundle(&root));
        } else {
            remove_integration();
            let bin = installed_bin(&root);
            for p in [bin.clone(), bin.with_extension("old"), bin.with_extension("new")] {
                let _ = std::fs::remove_file(p);
            }
        }
        let _ = std::fs::remove_file(root.join(super::MARKER));
        forget();
        if delete_data {
            for name in super::DATA_ENTRIES {
                let _ = std::fs::remove_file(root.join(name));
            }
        }
        let _ = std::fs::remove_dir(&root);
        Ok(())
    }

    pub fn uninstall_interactive() {
        use std::io::{IsTerminal, Write};
        let ask = |q: &str| {
            print!("{q} [y/N] ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).is_ok()
                && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
        };
        if !std::io::stdin().is_terminal() {
            eprintln!("Run `seepass --uninstall` from a terminal.");
            return;
        }
        let Some(root) = installed_root() else {
            println!("SeePass is not installed.");
            return;
        };
        if !ask(&format!("Uninstall SeePass from {}?", root.display())) {
            return;
        }
        let delete_data = ask("Also DELETE your vault, its backup and settings? This cannot be undone.");
        match uninstall(delete_data) {
            Ok(()) => println!("SeePass has been uninstalled."),
            Err(e) => eprintln!("Uninstall failed: {e}"),
        }
    }

    // --- Linux desktop integration ------------------------------------------------------------

    fn applications_dir() -> Option<PathBuf> {
        super::data_local_dir().map(|d| d.join("applications"))
    }
    fn icon_path() -> Option<PathBuf> {
        super::data_local_dir().map(|d| d.join("icons/hicolor/256x256/apps").join(format!("{APP_ID}.png")))
    }
    fn bin_link() -> Option<PathBuf> {
        super::home_dir().map(|h| h.join(".local/bin").join(APP_ID))
    }

    /// Quote a path for a .desktop `Exec=` key (freedesktop spec: escape " ` $ \ inside quotes).
    pub(super) fn exec_quote(p: &Path) -> String {
        let mut out = String::from("\"");
        for c in p.to_string_lossy().chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }

    fn integrate(exe: &Path) {
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName={APP_NAME}\n{}Exec={}\nIcon={APP_ID}\nTerminal=false\nStartupWMClass={APP_ID}\n",
            super::DESKTOP_KEYS,
            exec_quote(exe)
        );
        if let Some(icon) = icon_path() {
            if let Some(p) = icon.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::write(&icon, super::ICON_PNG);
        }
        if let Some(dir) = applications_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join(format!("{APP_ID}.desktop")), entry);
            let _ = Command::new("update-desktop-database").arg(&dir).output();
        }
        if let Some(link) = bin_link() {
            if let Some(p) = link.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::remove_file(&link);
            let _ = std::os::unix::fs::symlink(exe, &link);
        }
    }

    fn remove_integration() {
        if let Some(dir) = applications_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{APP_ID}.desktop")));
            let _ = Command::new("update-desktop-database").arg(&dir).output();
        }
        for p in [icon_path(), bin_link()].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
    }

    // --- macOS app bundle ---------------------------------------------------------------------

    /// `.icns` holding the 256×256 PNG icon (`ic08`).
    pub(super) fn icns() -> Vec<u8> {
        let png = super::ICON_PNG;
        let entry_len = 8 + png.len() as u32;
        let mut out = Vec::with_capacity(8 + entry_len as usize);
        out.extend_from_slice(b"icns");
        out.extend_from_slice(&(8 + entry_len).to_be_bytes());
        out.extend_from_slice(b"ic08");
        out.extend_from_slice(&entry_len.to_be_bytes());
        out.extend_from_slice(png);
        out
    }

    fn info_plist() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>{APP_NAME}</string>
  <key>CFBundleDisplayName</key><string>{APP_NAME}</string>
  <key>CFBundleIdentifier</key><string>{}</string>
  <key>CFBundleExecutable</key><string>{EXE_NAME}</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{CURRENT_VERSION}</string>
  <key>CFBundleVersion</key><string>{CURRENT_VERSION}</string>
  <key>NSHumanReadableCopyright</key><string>Made by Akulav</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#,
            super::BUNDLE_ID
        )
    }

    fn write_bundle(root: &Path) -> Result<(), String> {
        let app = bundle(root);
        std::fs::create_dir_all(app.join("Contents/Resources")).map_err(|e| e.to_string())?;
        std::fs::write(app.join("Contents/Resources/icon.icns"), icns()).map_err(|e| e.to_string())?;
        std::fs::write(app.join("Contents/Info.plist"), info_plist()).map_err(|e| e.to_string())?;
        let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&app).output();
        let _ = Command::new("/usr/bin/codesign").args(["--force", "--deep", "-s", "-"]).arg(&app).output();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_roots() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(root_of_exe(&p("/opt/x/seepass")), Some(p("/opt/x")));
        assert_eq!(
            root_of_exe(&p("/Users/a/Applications/SeePass/SeePass.app/Contents/MacOS/seepass")),
            Some(p("/Users/a/Applications/SeePass"))
        );
        assert_eq!(root_of_exe(&p("/x/MacOS/seepass")), Some(p("/x/MacOS")));
    }

    #[test]
    fn versions_and_folders() {
        assert!(is_newer("8.2.0", "8.1.9"));
        assert!(!is_newer("8.1.0", "8.1.0"));
        assert!(!is_newer("7.1.1", "8.0.0"));
        assert!(parse_install_dir("").is_err());
        assert!(parse_install_dir("relative/dir").is_err());
        if cfg!(windows) {
            assert!(parse_install_dir(r"C:\").is_err());
            assert_eq!(parse_install_dir(r"D:\Apps\SeePass").unwrap(), PathBuf::from(r"D:\Apps\SeePass"));
        } else {
            assert_eq!(parse_install_dir(" /opt/x ").unwrap(), PathBuf::from("/opt/x"));
        }
    }

    #[test]
    fn moves_files_completely() {
        let d = tempfile::tempdir().unwrap();
        let (src, dst) = (d.path().join("a/vault.seepass"), d.path().join("b/vault.seepass"));
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&src, b"secret bytes").unwrap();
        move_file(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"secret bytes");
        assert!(!src.exists());
    }

    #[cfg(unix)]
    #[test]
    fn unix_files() {
        assert_eq!(platform::exec_quote(Path::new("/a b/$x")), r#""/a b/\$x""#);
        let i = platform::icns();
        assert_eq!(&i[..4], b"icns");
        assert_eq!(u32::from_be_bytes(i[4..8].try_into().unwrap()) as usize, i.len());
    }
}
