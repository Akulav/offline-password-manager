//! First-run setup and the lock screen.

use eframe::egui::{self, RichText};
use seepass::crypto::KdfParams;
use seepass::strength;
use seepass::vault::Vault;
use zeroize::Zeroizing;

use super::task::{self, Task};
use super::widgets::{self, ACCENT};
use super::{Next, Shared};

#[derive(Default)]
pub struct SetupState {
    password: Zeroizing<String>,
    confirm: Zeroizing<String>,
    show: bool,
    strong_kdf: bool,
    error: Option<String>,
    task: Option<Task<seepass::Result<Vault>>>,
}

#[derive(Default)]
pub struct LockState {
    password: Zeroizing<String>,
    show: bool,
    error: Option<String>,
    task: Option<Task<seepass::Result<Vault>>>,
    focused: bool,
}

/// Centered card used by both screens.
fn card<R>(ctx: &egui::Context, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::CentralPanel::default()
        .show(ctx, |ui| {
            let width = 380.0_f32.min(ui.available_width());
            ui.add_space((ui.available_height() * 0.12).max(12.0));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🛡").size(48.0).color(ACCENT));
                ui.heading(RichText::new("SeePass").strong());
                ui.add_space(8.0);
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        add(ui)
                    })
                    .inner
                })
                .inner
            })
            .inner
        })
        .inner
}

pub fn setup(ctx: &egui::Context, s: &mut SetupState, shared: &mut Shared) -> Option<Next> {
    if let Some(result) = task::take(&mut s.task) {
        match result {
            Ok(vault) => {
                shared.toast("Vault created");
                return Some(Next::Unlocked(vault));
            }
            Err(e) => s.error = Some(e.to_string()),
        }
    }
    let busy = s.task.is_some();
    let mut next = None;

    card(ctx, |ui| {
        ui.label("Create a master password. It is the only way to open your vault - it cannot be recovered or reset.");
        ui.add_space(4.0);
        widgets::secret_field(ui, &mut s.password, &mut s.show, "Master password");
        widgets::strength_meter(ui, &s.password);
        let enter = widgets::secret_field(ui, &mut s.confirm, &mut s.show, "Repeat master password").lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.checkbox(&mut s.strong_kdf, "Extra-strong key derivation (slower unlock, 256 MiB)");
        widgets::error_label(ui, &s.error);

        let create =
            ui.add_enabled(!busy, egui::Button::new("Create vault").min_size(egui::vec2(ui.available_width(), 32.0)));
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Deriving key...");
            });
        }
        if (create.clicked() || enter) && !busy {
            if let Some(problem) = strength::master_password_problem(&s.password) {
                s.error = Some(problem);
            } else if *s.password != *s.confirm {
                s.error = Some("The passwords do not match.".into());
            } else {
                s.error = None;
                let password = s.password.clone();
                let path = shared.vault_path.clone();
                let params = if s.strong_kdf { KdfParams::STRONG } else { KdfParams::STANDARD };
                s.task = Some(Task::spawn(ui.ctx(), move || Vault::create(path, &password, params)));
            }
        }

        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("Already have a vault or an encrypted backup?").small());
        if ui.add_enabled(!busy, egui::Button::new("Open existing vault file...")).clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Open SeePass vault")
                .add_filter("SeePass vault", &["seepass"])
                .add_filter("All files", &["*"])
                .pick_file()
            {
                shared.set_vault_path(path);
                next = Some(Next::Locked);
            }
        }
        #[cfg(feature = "legacy-import")]
        if seepass::legacy::find_default().is_some() {
            ui.label(
                RichText::new(
                    "SeePass 7 data was found on this PC. Create your new vault first, then use \
                     Settings → Import from SeePass 7.",
                )
                .small()
                .color(ACCENT),
            );
        }
    });
    next
}

pub fn unlock(ctx: &egui::Context, s: &mut LockState, shared: &mut Shared) -> Option<Next> {
    if let Some(result) = task::take(&mut s.task) {
        match result {
            Ok(vault) => return Some(Next::Unlocked(vault)),
            Err(e) => {
                s.error = Some(e.to_string());
                s.password.clear();
                s.focused = false;
            }
        }
    }
    if !shared.vault_path.exists() {
        return Some(Next::Setup);
    }
    let busy = s.task.is_some();
    let mut next = None;

    card(ctx, |ui| {
        ui.label("Enter your master password to unlock.");
        let resp = widgets::secret_field(ui, &mut s.password, &mut s.show, "Master password");
        if !s.focused {
            resp.request_focus();
            s.focused = true;
        }
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        widgets::error_label(ui, &s.error);
        let clicked = ui
            .add_enabled(!busy, egui::Button::new("Unlock").min_size(egui::vec2(ui.available_width(), 32.0)))
            .clicked();
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Deriving key...");
            });
        }
        if (clicked || enter) && !busy && !s.password.is_empty() {
            s.error = None;
            let password = s.password.clone();
            let path = shared.vault_path.clone();
            s.task = Some(Task::spawn(ui.ctx(), move || Vault::unlock(path, &password)));
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("Open a different vault...").small()).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open SeePass vault")
                    .add_filter("SeePass vault", &["seepass"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    shared.set_vault_path(path);
                    *s = LockState::default();
                }
            }
            if shared.vault_path != shared.paths.default_vault
                && ui.add_enabled(!busy, egui::Button::new("Use default vault").small()).clicked()
            {
                let default = shared.paths.default_vault.clone();
                shared.set_vault_path(default);
                *s = LockState::default();
                next = Some(Next::Locked);
            }
        });
    });
    next
}
