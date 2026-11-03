use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, FontId, RichText, Stroke, TextStyle};
use seepass::strength;

use super::Shared;

/// Brand colour, taken from the app icon's gradient.
pub const ACCENT: Color32 = Color32::from_rgb(84, 88, 240);
pub const DANGER: Color32 = Color32::from_rgb(225, 75, 85);
pub const WARNING: Color32 = Color32::from_rgb(230, 160, 40);
pub const SUCCESS: Color32 = Color32::from_rgb(60, 175, 110);

/// Avatar colours, picked by hashing the entry title.
const AVATAR_COLORS: [Color32; 8] = [
    Color32::from_rgb(172, 126, 241),
    Color32::from_rgb(249, 118, 176),
    Color32::from_rgb(253, 138, 114),
    Color32::from_rgb(95, 77, 221),
    Color32::from_rgb(24, 161, 251),
    Color32::from_rgb(38, 166, 154),
    Color32::from_rgb(102, 187, 106),
    Color32::from_rgb(255, 167, 38),
];

pub fn apply_style(ctx: &egui::Context, dark: bool) {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    if dark {
        v.panel_fill = Color32::from_rgb(22, 23, 30);
        v.window_fill = Color32::from_rgb(30, 31, 40);
        v.extreme_bg_color = Color32::from_rgb(15, 16, 21);
        v.faint_bg_color = Color32::from_rgb(33, 35, 46);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(40, 42, 55);
        v.widgets.inactive.bg_fill = Color32::from_rgb(40, 42, 55);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 55, 72);
        v.widgets.hovered.bg_fill = Color32::from_rgb(52, 55, 72);
        v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(48, 50, 64));
    } else {
        v.panel_fill = Color32::from_rgb(247, 248, 252);
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(237, 239, 247);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(232, 234, 243);
        v.widgets.inactive.bg_fill = Color32::from_rgb(232, 234, 243);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(220, 223, 238);
        v.widgets.hovered.bg_fill = Color32::from_rgb(220, 223, 238);
        v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(222, 225, 236));
    }
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = if dark { Color32::from_rgb(150, 160, 255) } else { ACCENT };
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(70) };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(50) };
    let r = CornerRadius::same(7);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = r;
    }
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.window_margin = egui::Margin::same(16);
        s.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(15.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
    });
}

/// Upload the app icon once so screens can draw it.
pub fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/seepass-256.png")).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    let options =
        egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
    Some(ctx.load_texture("seepass-logo", image, options))
}

/// The app icon at `size` points, or nothing if it failed to load.
pub fn logo(ui: &mut egui::Ui, logo: &Option<egui::TextureHandle>, size: f32) {
    if let Some(tex) = logo {
        ui.add(egui::Image::new((tex.id(), egui::vec2(size, size))));
    }
}

/// Filled accent button for the main action on a screen.
pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(ACCENT)
}

/// A rounded panel used for the details "cards" and banners.
pub fn card(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 10))
}

pub fn banner(color: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 8))
}

pub fn avatar_color(title: &str) -> Color32 {
    let h = title.to_lowercase().bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    AVATAR_COLORS[h as usize % AVATAR_COLORS.len()]
}

/// Paint a round avatar with the title's first letter, centred on `center`.
pub fn paint_avatar(painter: &egui::Painter, center: egui::Pos2, radius: f32, title: &str) {
    painter.circle_filled(center, radius, avatar_color(title));
    let letter: String = title.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    painter.text(center, egui::Align2::CENTER_CENTER, letter, FontId::proportional(radius * 1.05), Color32::WHITE);
}

pub fn avatar(ui: &mut egui::Ui, title: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint_avatar(ui.painter(), rect.center(), size / 2.0, title);
}

/// One row of the entry list: avatar, title, username, optional star.
pub fn entry_row(ui: &mut egui::Ui, selected: bool, title: &str, user: &str, favorite: bool) -> egui::Response {
    let height = 48.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click());
    // Give screen readers (and the UI tests) something to read.
    let label = if title.is_empty() { "(untitled)" } else { title };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let v = ui.visuals();
        if selected {
            painter.rect_filled(rect, CornerRadius::same(8), ACCENT.gamma_multiply(0.35));
        } else if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(8), v.widgets.hovered.weak_bg_fill);
        }
        let title = if title.is_empty() { "(untitled)" } else { title };
        paint_avatar(&painter, egui::pos2(rect.left() + 24.0, rect.center().y), 15.0, title);

        let text_left = rect.left() + 48.0;
        let right_pad = if favorite { 28.0 } else { 8.0 };
        let max_w = (rect.right() - right_pad - text_left).max(10.0);
        let strong = v.strong_text_color();
        // On the accent-tinted selected row the usual weak grey is too faint.
        let weak = if selected { strong.gamma_multiply(0.75) } else { v.weak_text_color() };
        let title_galley = painter.layout(title.to_string(), FontId::proportional(15.0), strong, f32::INFINITY);
        let clip =
            egui::Rect::from_min_max(egui::pos2(text_left, rect.top()), egui::pos2(text_left + max_w, rect.bottom()));
        let painter = painter.with_clip_rect(clip);
        if user.is_empty() {
            painter.galley(egui::pos2(text_left, rect.center().y - title_galley.size().y / 2.0), title_galley, strong);
        } else {
            painter.galley(egui::pos2(text_left, rect.top() + 6.0), title_galley, strong);
            painter.text(
                egui::pos2(text_left, rect.bottom() - 7.0),
                egui::Align2::LEFT_BOTTOM,
                user,
                FontId::proportional(12.5),
                weak,
            );
        }
        if favorite {
            ui.painter().text(
                egui::pos2(rect.right() - 14.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "★",
                FontId::proportional(14.0),
                WARNING,
            );
        }
    }
    resp
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

pub fn score_color(score: u8) -> Color32 {
    match score {
        0 | 1 => DANGER,
        2 => WARNING,
        3 => Color32::from_rgb(110, 185, 90),
        _ => SUCCESS,
    }
}

pub fn strength_meter(ui: &mut egui::Ui, password: &str) {
    if password.is_empty() {
        return;
    }
    let s = strength::assess(password, &[]);
    let color = score_color(s.score);
    ui.add(
        egui::ProgressBar::new((s.score as f32 + 1.0) / 5.0)
            .fill(color)
            .desired_height(6.0)
            .desired_width(ui.available_width()),
    );
    ui.label(RichText::new(format!("{} · offline crack time ≈ {}", strength::label(s.score), s.crack_time)).small());
    if let Some(w) = s.warning {
        ui.label(RichText::new(w).small().color(color));
    }
}

/// Small coloured pill, e.g. "strong" next to a password.
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.2))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(color)));
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
            if unlocked {
                ui.label(RichText::new("🔓 Unlocked").small().color(SUCCESS));
                if let Some(left) = shared.auto_lock_remaining() {
                    let text = RichText::new(format!("locks in {}", mmss(left))).small();
                    ui.label(if left < Duration::from_secs(60) { text.color(WARNING) } else { text.weak() })
                        .on_hover_text("Auto-lock after inactivity (Settings)");
                }
            } else {
                ui.label(RichText::new("🔒 Locked").small());
            }
            if let Some(left) = shared.clipboard.remaining() {
                ui.separator();
                ui.label(
                    RichText::new(format!("📋 clipboard clears in {} s", left.as_secs() + 1)).small().color(WARNING),
                );
            }
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

fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

/// "3 days ago" style formatting for unix timestamps.
pub fn ago(unix: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    human_age(now.saturating_sub(unix))
}

pub fn human_age(secs: u64) -> String {
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        86_400..=172_799 => "yesterday".into(),
        172_800..=5_183_999 => format!("{} days ago", secs / 86_400),
        5_184_000..=63_071_999 => format!("{} months ago", secs / 2_592_000),
        _ => format!("{} years ago", secs / 31_536_000),
    }
}
