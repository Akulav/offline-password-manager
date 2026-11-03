//! Settings window and modal dialogs (delete, change password, backups, import).

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use seepass::crypto::{KdfParams, MasterKey};
use seepass::vault::{self, Entry, VaultData};
use seepass::{strength, Result};
use zeroize::Zeroizing;

use super::task::{self, Task};
use super::vault_view::Unlocked;
use super::widgets::{self, DANGER};
use super::{Next, Shared};

pub enum Dialog {
    ConfirmDelete {
        id: String,
        title: String,
    },
    ChangePassword(PasswordForm, Option<Task<Result<MasterKey>>>),
    ExportBackup {
        path: PathBuf,
        form: PasswordForm,
        task: Option<Task<Result<()>>>,
    },
    ImportBackup {
        path: PathBuf,
        password: Zeroizing<String>,
        show: bool,
        error: Option<String>,
        task: Option<Task<Result<VaultData>>>,
    },
    #[cfg_attr(not(feature = "legacy-import"), allow(dead_code))]
    ImportLegacy(LegacyForm),
    DestroyVault {
        typed: String,
    },
}

#[derive(Default)]
pub struct PasswordForm {
    current: Zeroizing<String>,
    new: Zeroizing<String>,
    confirm: Zeroizing<String>,
    show: bool,
    strong: bool,
    error: Option<String>,
}

#[cfg_attr(not(feature = "legacy-import"), allow(dead_code))]
pub struct LegacyForm {
    path: PathBuf,
    username: String,
    password: Zeroizing<String>,
    pim: String,
    show: bool,
    error: Option<String>,
    task: Option<Task<Result<Vec<Entry>>>>,
}

impl Dialog {
    #[cfg_attr(not(feature = "legacy-import"), allow(dead_code))]
    pub fn import_legacy(path: PathBuf) -> Self {
        Dialog::ImportLegacy(LegacyForm {
            path,
            username: String::new(),
            password: Zeroizing::default(),
            pim: String::new(),
            show: false,
            error: None,
            task: None,
        })
    }
}

pub fn settings_window(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) -> Option<Next> {
    let mut open = u.settings_open;
    egui::Window::new("⚙ Settings").open(&mut open).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.label(RichText::new("Appearance").strong());
        if ui.checkbox(&mut shared.settings.dark_mode, "Dark mode").changed() {
            widgets::apply_style(ctx, shared.settings.dark_mode);
            shared.save_settings();
        }

        ui.separator();
        ui.label(RichText::new("Security").strong());
        let mut lock_min = shared.settings.auto_lock_secs / 60;
        ui.horizontal(|ui| {
            ui.label("Auto-lock after");
            let r = ui.add(egui::DragValue::new(&mut lock_min).range(0..=240).suffix(" min"));
            ui.label(RichText::new("(0 = never)").small().weak());
            if r.changed() {
                shared.settings.auto_lock_secs = lock_min * 60;
                shared.save_settings();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Clear clipboard after");
            let r = ui.add(egui::DragValue::new(&mut shared.settings.clipboard_clear_secs).range(0..=600).suffix(" s"));
            ui.label(RichText::new("(0 = never)").small().weak());
            if r.changed() {
                shared.save_settings();
            }
        });
        let p = u.vault.kdf_params();
        ui.label(
            RichText::new(format!("Key derivation: Argon2id, {} MiB, {} passes", p.m_cost_kib / 1024, p.t_cost))
                .small()
                .weak(),
        );
        if ui.button("Change master password...").clicked() {
            u.dialog = Some(Dialog::ChangePassword(PasswordForm::default(), None));
        }

        ui.separator();
        ui.label(RichText::new("Backup & import").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Export encrypted backup...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Export encrypted backup")
                    .set_file_name("seepass-backup.seepass")
                    .add_filter("SeePass vault", &["seepass"])
                    .save_file()
                {
                    u.dialog = Some(Dialog::ExportBackup { path, form: PasswordForm::default(), task: None });
                }
            }
            if ui.button("Import / merge backup...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Import SeePass backup")
                    .add_filter("SeePass vault", &["seepass"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    u.dialog = Some(Dialog::ImportBackup {
                        path,
                        password: Zeroizing::default(),
                        show: false,
                        error: None,
                        task: None,
                    });
                }
            }
            #[cfg(feature = "legacy-import")]
            if ui.button("Import from SeePass 7...").clicked() {
                let start = seepass::legacy::find_default();
                let picked = match start {
                    Some(dir) => Some(dir),
                    None => rfd::FileDialog::new()
                        .set_title("Select SeePass 7 export (data.zip)")
                        .add_filter("SeePass 7 export", &["zip"])
                        .pick_file(),
                };
                if let Some(path) = picked {
                    u.dialog = Some(Dialog::import_legacy(path));
                }
            }
        });

        ui.separator();
        ui.label(RichText::new("Vault file").strong());
        ui.label(RichText::new(u.vault.path().display().to_string()).small().monospace());
        ui.label(
            RichText::new(format!("The previous version is kept as {}", vault::backup_path(u.vault.path()).display()))
                .small()
                .weak(),
        );

        ui.separator();
        ui.label(RichText::new("Danger zone").strong().color(DANGER));
        if ui.button(RichText::new("Delete this vault...").color(DANGER)).clicked() {
            u.dialog = Some(Dialog::DestroyVault { typed: String::new() });
        }
    });
    u.settings_open = open;
    None
}

pub fn show_dialog(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) -> Option<Next> {
    let dialog = u.dialog.as_mut()?;
    let mut close = false;
    let mut next = None;

    let modal = egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
        ui.set_width(380.0);
        match dialog {
            Dialog::ConfirmDelete { id, title } => {
                ui.heading("Delete entry?");
                ui.label(format!("\"{title}\" will be removed from the vault."));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Delete").color(DANGER)).clicked() {
                        match u.vault.delete(id) {
                            Ok(_) => shared.toast("Entry deleted"),
                            Err(e) => shared.error(format!("Could not delete: {e}")),
                        }
                        close = true;
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }

            Dialog::ChangePassword(form, task) => {
                if let Some(result) = task::take(task) {
                    match result.and_then(|key| u.vault.set_key(key)) {
                        Ok(()) => {
                            shared.toast("Master password changed");
                            close = true;
                        }
                        Err(e) => form.error = Some(e.to_string()),
                    }
                }
                ui.heading("Change master password");
                widgets::secret_field(ui, &mut form.current, &mut form.show, "Current master password");
                widgets::secret_field(ui, &mut form.new, &mut form.show, "New master password");
                widgets::strength_meter(ui, &form.new);
                widgets::secret_field(ui, &mut form.confirm, &mut form.show, "Repeat new master password");
                ui.checkbox(&mut form.strong, "Extra-strong key derivation (256 MiB)");
                widgets::error_label(ui, &form.error);
                let busy = task.is_some();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Change")).clicked() {
                        form.error = validate_new(&form.new, &form.confirm, true);
                        if form.error.is_none() {
                            // Verify the current password and derive the new key off-thread.
                            let path = u.vault.path().to_path_buf();
                            let (cur, new) = (form.current.clone(), form.new.clone());
                            let params = if form.strong { KdfParams::STRONG } else { KdfParams::STANDARD };
                            *task = Some(Task::spawn(ui.ctx(), move || {
                                let bytes = std::fs::read(&path)?;
                                seepass::crypto::open(&cur, &bytes)
                                    .map_err(|_| seepass::Error::Kdf("the current password is wrong".into()))?;
                                MasterKey::derive_new(&new, params)
                            }));
                        }
                    }
                    close |= ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked();
                    if busy {
                        ui.spinner();
                    }
                });
            }

            Dialog::ExportBackup { path, form, task } => {
                if let Some(result) = task::take(task) {
                    match result {
                        Ok(()) => {
                            shared.toast(format!("Backup written to {}", path.display()));
                            close = true;
                        }
                        Err(e) => form.error = Some(e.to_string()),
                    }
                }
                ui.heading("Export encrypted backup");
                ui.label(RichText::new(path.display().to_string()).small().monospace());
                ui.label("Choose a password for the backup. It can differ from your master password.");
                widgets::secret_field(ui, &mut form.new, &mut form.show, "Backup password");
                widgets::strength_meter(ui, &form.new);
                widgets::secret_field(ui, &mut form.confirm, &mut form.show, "Repeat backup password");
                widgets::error_label(ui, &form.error);
                let busy = task.is_some();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Export")).clicked() {
                        form.error = validate_new(&form.new, &form.confirm, false);
                        if form.error.is_none() {
                            let (dest, pw, params, data) =
                                (path.clone(), form.new.clone(), u.vault.kdf_params(), u.vault.snapshot());
                            *task = Some(Task::spawn(ui.ctx(), move || vault::write_backup(&dest, &pw, params, &data)));
                        }
                    }
                    close |= ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked();
                    if busy {
                        ui.spinner();
                    }
                });
            }

            Dialog::ImportBackup { path, password, show, error, task } => {
                if let Some(result) = task::take(task) {
                    match result.and_then(|data| u.vault.merge(data)) {
                        Ok(r) => {
                            shared.toast(format!(
                                "Imported: {} added, {} updated, {} unchanged",
                                r.added, r.updated, r.unchanged
                            ));
                            close = true;
                        }
                        Err(e) => *error = Some(e.to_string()),
                    }
                }
                ui.heading("Import / merge backup");
                ui.label(RichText::new(path.display().to_string()).small().monospace());
                ui.label("New entries are added; entries that exist in both keep the most recently edited copy.");
                widgets::secret_field(ui, password, show, "Backup password");
                widgets::error_label(ui, error);
                let busy = task.is_some();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy && !password.is_empty(), egui::Button::new("Import")).clicked() {
                        let (src, pw) = (path.clone(), password.clone());
                        *task = Some(Task::spawn(ui.ctx(), move || vault::Vault::read_backup(&src, &pw)));
                    }
                    close |= ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked();
                    if busy {
                        ui.spinner();
                    }
                });
            }

            Dialog::ImportLegacy(f) => {
                #[cfg(feature = "legacy-import")]
                legacy_dialog(ui, f, &mut u.vault, shared, &mut close);
                #[cfg(not(feature = "legacy-import"))]
                {
                    let _ = f;
                    close = true;
                }
            }

            Dialog::DestroyVault { typed } => {
                ui.heading(RichText::new("Delete this vault?").color(DANGER));
                ui.label(
                    "This permanently deletes the vault file and its automatic backup. \
                     Without an exported backup, the entries are gone for good.",
                );
                ui.label("Type DELETE to confirm:");
                ui.text_edit_singleline(typed);
                ui.horizontal(|ui| {
                    let ok = typed.as_str() == "DELETE";
                    if ui.add_enabled(ok, egui::Button::new(RichText::new("Delete vault").color(DANGER))).clicked() {
                        match vault::destroy(u.vault.path()) {
                            Ok(()) => {
                                shared.toast("Vault deleted");
                                next = Some(Next::Setup);
                            }
                            Err(e) => shared.error(format!("Could not delete vault: {e}")),
                        }
                        close = true;
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
        }
    });

    // Escape / click outside closes the dialog, except while work is running.
    let busy = match &u.dialog {
        Some(Dialog::ChangePassword(_, t)) => t.is_some(),
        Some(Dialog::ExportBackup { task, .. }) => task.is_some(),
        Some(Dialog::ImportBackup { task, .. }) => task.is_some(),
        Some(Dialog::ImportLegacy(f)) => f.task.is_some(),
        _ => false,
    };
    if close || (modal.should_close() && !busy) {
        u.dialog = None;
    }
    next
}

fn validate_new(new: &str, confirm: &str, is_master: bool) -> Option<String> {
    if is_master {
        if let Some(p) = strength::master_password_problem(new) {
            return Some(p);
        }
    } else if new.chars().count() < strength::MIN_MASTER_LENGTH {
        return Some(format!("Use at least {} characters.", strength::MIN_MASTER_LENGTH));
    }
    (new != confirm).then(|| "The passwords do not match.".into())
}

#[cfg(feature = "legacy-import")]
fn legacy_dialog(
    ui: &mut egui::Ui,
    f: &mut LegacyForm,
    vault: &mut vault::Vault,
    shared: &mut Shared,
    close: &mut bool,
) {
    if let Some(result) = task::take(&mut f.task) {
        match result.and_then(|entries| vault.import_entries(entries)) {
            Ok(n) => {
                shared.toast(format!("Imported {n} entries from SeePass 7"));
                *close = true;
            }
            Err(e) => f.error = Some(e.to_string()),
        }
    }
    ui.heading("Import from SeePass 7");
    ui.label(RichText::new(f.path.display().to_string()).small().monospace());
    ui.label("Enter the username, password and PIM you used in SeePass 7. The old files are only read, never changed.");
    ui.add(egui::TextEdit::singleline(&mut f.username).hint_text("SeePass 7 username"));
    widgets::secret_field(ui, &mut f.password, &mut f.show, "SeePass 7 password");
    ui.add(egui::TextEdit::singleline(&mut f.pim).hint_text("PIM (100 - 100000)"));
    widgets::error_label(ui, &f.error);
    let busy = f.task.is_some();
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, egui::Button::new("Import")).clicked() {
            match f.pim.trim().parse::<u32>() {
                Ok(pim) if (100..=100_000).contains(&pim) => {
                    f.error = None;
                    let (path, user, pw) = (f.path.clone(), f.username.clone(), f.password.clone());
                    f.task = Some(Task::spawn(ui.ctx(), move || {
                        seepass::legacy::LegacySource::load(&path)?.decrypt(&user, &pw, pim)
                    }));
                }
                _ => f.error = Some("The PIM is a number between 100 and 100000.".into()),
            }
        }
        if !busy && ui.button("Choose another file...").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .set_title("Select SeePass 7 export (data.zip)")
                .add_filter("SeePass 7 export", &["zip"])
                .pick_file()
            {
                f.path = p;
            }
        }
        *close |= ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked();
        if busy {
            ui.spinner();
            ui.label("Decrypting...");
        }
    });
}
