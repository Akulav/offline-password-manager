//! First-run setup and the lock screen.

use eframe::egui::{self, RichText};
use seepass::crypto::KdfParams;
use seepass::strength;
use seepass::vault::Vault;
use zeroize::Zeroizing;

use super::task::{self, Task};
use super::widgets;
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
    failures: u32,
    #[cfg(debug_assertions)]
    dev_tried: bool,
}

/// Centered card used by both screens.
fn card<R>(ctx: &egui::Context, logo: Option<egui::TextureHandle>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::CentralPanel::default()
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    let width = 400.0_f32.min(ui.available_width() - 8.0);
                    ui.add_space((ui.available_height() * 0.10).max(16.0));
                    ui.vertical_centered(|ui| {
                        widgets::logo(ui, &logo, 72.0);
                        ui.add_space(4.0);
                        ui.label(RichText::new("SeePass").size(26.0).strong());
                        ui.label(RichText::new("Offline password manager").weak());
                    });
                    ui.add_space(14.0);
                    // Centre the fixed-width card.
                    let pad = ((ui.available_width() - width) / 2.0 - ui.spacing().item_spacing.x).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        let v = ui.visuals();
                        egui::Frame::new()
                            .fill(v.window_fill)
                            .stroke(v.window_stroke)
                            .corner_radius(egui::CornerRadius::same(14))
                            .shadow(egui::Shadow {
                                offset: [0, 6],
                                blur: 20,
                                spread: 0,
                                color: egui::Color32::from_black_alpha(if v.dark_mode { 90 } else { 26 }),
                            })
                            .inner_margin(egui::Margin::same(22))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(width - 44.0);
                                    add(ui)
                                })
                                .inner
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

    card(ctx, shared.logo.clone(), |ui| {
        ui.label(RichText::new("Create your vault").size(18.0).strong());
        ui.label(
            RichText::new(
                "Choose a master password. It is the only way to open your vault - it cannot be recovered or reset.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        widgets::secret_field(ui, &mut s.password, &mut s.show, "Master password");
        widgets::strength_meter(ui, &s.password);
        let enter = widgets::secret_field(ui, &mut s.confirm, &mut s.show, "Repeat master password").lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.checkbox(&mut s.strong_kdf, "Extra-strong key derivation (slower unlock, 256 MiB)");
        widgets::error_label(ui, &s.error);

        ui.add_space(4.0);
        let create = ui.add_enabled(
            !busy,
            widgets::primary_button("Create vault").min_size(egui::vec2(ui.available_width(), 36.0)),
        );
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
                .color(widgets::ACCENT),
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
                s.failures += 1;
                s.error = Some(e.to_string());
                s.password.clear();
                s.focused = false;
            }
        }
    }
    if !shared.vault_path.exists() {
        return Some(Next::Setup);
    }
    // Debug builds only: auto-unlock for UI development (see docs/architecture.md).
    #[cfg(debug_assertions)]
    let dev_unlock = !std::mem::replace(&mut s.dev_tried, true)
        && std::env::var("SEEPASS_DEV_PASSWORD").is_ok_and(|p| {
            *s.password = p;
            true
        });
    #[cfg(not(debug_assertions))]
    let dev_unlock = false;
    let busy = s.task.is_some();
    let mut next = None;

    card(ctx, shared.logo.clone(), |ui| {
        ui.label(RichText::new("Welcome back").size(18.0).strong());
        ui.label(RichText::new("Enter your master password to unlock.").weak());
        let resp = widgets::secret_field(ui, &mut s.password, &mut s.show, "Master password");
        if !s.focused {
            resp.request_focus();
            s.focused = true;
        }
        let enter = dev_unlock || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if s.failures > 1 {
            widgets::error_label(ui, &s.error.as_ref().map(|e| format!("{e} ({} failed attempts)", s.failures)));
        } else {
            widgets::error_label(ui, &s.error);
        }
        ui.add_space(4.0);
        let clicked = ui
            .add_enabled(!busy, widgets::primary_button("Unlock").min_size(egui::vec2(ui.available_width(), 36.0)))
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
