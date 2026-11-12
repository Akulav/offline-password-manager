//! Linux/macOS, started outside the install folder: install into a folder of the user's choice,
//! update the installed copy, or just run this one.

use eframe::egui::{self, RichText};
use seepass::install::{self, Offer};

pub enum PromptResult {
    Open,
    /// Installed (or updated) and the installed copy started: close this window.
    Close,
    /// "Just run it".
    JustRun,
}

pub struct InstallPrompt {
    offer: Offer,
    dir: String,
    error: Option<String>,
}

impl InstallPrompt {
    pub fn new(offer: Offer) -> Option<Self> {
        let dir = match &offer {
            Offer::None => return None,
            Offer::Install { suggested } => suggested.display().to_string(),
            Offer::Update { root, .. } => root.display().to_string(),
        };
        Some(Self { offer, dir, error: None })
    }

    pub fn show(&mut self, ctx: &egui::Context) -> PromptResult {
        let mut result = PromptResult::Open;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space((ui.available_height() * 0.5 - 170.0).max(16.0));
            ui.vertical_centered(|ui| {
                egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(24)).show(ui, |ui| {
                    ui.set_width(520.0_f32.min(ui.available_width()));
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        result = self.body(ui);
                    });
                });
            });
        });
        result
    }

    fn body(&mut self, ui: &mut egui::Ui) -> PromptResult {
        let mut result = PromptResult::Open;
        let updating = matches!(self.offer, Offer::Update { .. });
        ui.label(RichText::new(format!("Version {}  ·  made by Akulav", install::CURRENT_VERSION)).size(12.0).weak());
        ui.label(RichText::new(if updating { "Update SeePass" } else { "Install SeePass" }).size(20.0).strong());
        ui.add_space(8.0);
        match &self.offer {
            Offer::Update { root, installed } => {
                let from =
                    installed.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "an older version".into());
                ui.label(format!(
                    "SeePass {from} is installed in {}. Update it to {}? Your vault and settings stay where they are.",
                    root.display(),
                    install::CURRENT_VERSION
                ));
            }
            _ => {
                ui.label(
                    "Choose a folder. SeePass and everything it keeps (your encrypted vault, its backup and settings) \
                     go there, and updates keep using it.",
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Install folder").strong());
                if ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(f32::INFINITY)).changed() {
                    self.error = None;
                }
                let extras = if cfg!(target_os = "macos") {
                    "Creates SeePass.app in this folder."
                } else {
                    "Also adds an app-menu entry and the seepass command."
                };
                ui.label(RichText::new(extras).size(12.0).weak());
            }
        }
        if let Some(err) = &self.error {
            ui.add_space(6.0);
            ui.colored_label(egui::Color32::from_rgb(0xf0, 0x6a, 0x6a), err);
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(if updating { "Update" } else { "Install" }).strong()).clicked() {
                let r = match &self.offer {
                    Offer::Update { root, .. } => install::install_and_launch(root, &[]),
                    _ => install::parse_install_dir(&self.dir)
                        .and_then(|d| install::install_and_launch(&d, &["--installed"])),
                };
                match r {
                    Ok(()) => result = PromptResult::Close,
                    Err(e) => self.error = Some(e),
                }
            }
            if ui.button("Just run it").on_hover_text("Run this copy without installing it").clicked() {
                result = PromptResult::JustRun;
            }
        });
        result
    }
}
