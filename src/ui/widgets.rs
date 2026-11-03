use std::time::Instant;

use eframe::egui::{self, Color32, FontId, RichText, TextStyle};
use seepass::strength;

use super::Shared;

pub const ACCENT: Color32 = Color32::from_rgb(95, 77, 221);
pub const DANGER: Color32 = Color32::from_rgb(220, 70, 80);

pub fn apply_style(ctx: &egui::Context, dark: bool) {
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.selection.bg_fill = ACCENT;
    visuals.hyperlink_color = if dark { Color32::from_rgb(150, 140, 255) } else { ACCENT };
    ctx.set_visuals(visuals);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(15.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
    });
}

/// Single-line password field with a show/hide toggle.
pub fn secret_field(ui: &mut egui::Ui, value: &mut String, shown: &mut bool, hint: &str) -> egui::Response {
    // Measure before entering the row so stacked fields line up exactly.
    let width = ui.available_width() - 40.0;
    ui.horizontal(|ui| {
        let resp = ui.add(egui::TextEdit::singleline(value).password(!*shown).hint_text(hint).desired_width(width));
        if ui.selectable_label(*shown, "👁").on_hover_text(if *shown { "Hide" } else { "Show" }).clicked() {
            *shown = !*shown;
        }
        resp
    })
    .inner
}

pub fn strength_meter(ui: &mut egui::Ui, password: &str) {
    if password.is_empty() {
        return;
    }
    let s = strength::assess(password, &[]);
    let color = match s.score {
        0 | 1 => DANGER,
        2 => Color32::from_rgb(230, 160, 40),
        3 => Color32::from_rgb(90, 180, 90),
        _ => Color32::from_rgb(40, 170, 120),
    };
    ui.add(
        egui::ProgressBar::new((s.score as f32 + 1.0) / 5.0)
            .fill(color)
            .desired_height(6.0)
            .desired_width(ui.available_width()),
    );
    ui.label(RichText::new(format!("{} - offline crack time ≈ {}", strength::label(s.score), s.crack_time)).small());
    if let Some(w) = s.warning {
        ui.label(RichText::new(w).small().color(color));
    }
}

pub fn error_label(ui: &mut egui::Ui, error: &Option<String>) {
    if let Some(e) = error {
        ui.colored_label(DANGER, e);
    }
}

pub fn status_bar(ctx: &egui::Context, shared: &mut Shared, unlocked: bool) {
    if shared.toast.as_ref().is_some_and(|t| Instant::now() >= t.until) {
        shared.toast = None;
    }
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if unlocked { "🔓 Unlocked" } else { "🔒 Locked" }).small());
            ui.separator();
            // Right side first, so the (possibly long) path gets whatever is left.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("v{}", seepass::VERSION)).small().weak());
                if let Some(t) = &shared.toast {
                    let text = RichText::new(&t.text).small();
                    ui.label(if t.error { text.color(DANGER) } else { text.strong() });
                    if let Some(rem) = t.until.checked_duration_since(Instant::now()) {
                        ctx.request_repaint_after(rem);
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let path = shared.vault_path.display().to_string();
                    ui.add(egui::Label::new(RichText::new(&path).small().weak()).truncate()).on_hover_text(path);
                });
            });
        });
    });
}

/// "3 days ago" style formatting for unix timestamps.
pub fn ago(unix: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let d = now.saturating_sub(unix);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ => format!("{} days ago", d / 86_400),
    }
}
