//! The unlocked vault: entry list, details, editor and generator.

use eframe::egui::{self, RichText};
use seepass::generator::{self, GeneratorOptions};
use seepass::vault::{Entry, Vault};
use zeroize::Zeroizing;

use super::dialogs::{self, Dialog};
use super::widgets::{self, ACCENT, DANGER};
use super::{Next, Shared};

pub struct Unlocked {
    pub vault: Vault,
    search: String,
    selected: Option<String>,
    editor: Option<Editor>,
    reveal: bool,
    generator: Generator,
    pub settings_open: bool,
    pub dialog: Option<Dialog>,
    #[cfg_attr(not(feature = "legacy-import"), allow(dead_code))]
    legacy_hint_dismissed: bool,
}

struct Editor {
    draft: Entry,
    is_new: bool,
    show_password: bool,
    error: Option<String>,
}

struct Generator {
    open: bool,
    opts: GeneratorOptions,
    output: Zeroizing<String>,
}

impl Unlocked {
    pub fn new(vault: Vault) -> Self {
        let opts = GeneratorOptions::default();
        Self {
            vault,
            search: String::new(),
            selected: None,
            editor: None,
            reveal: false,
            generator: Generator { open: false, opts, output: generator::generate(&opts).unwrap_or_default() },
            settings_open: false,
            dialog: None,
            legacy_hint_dismissed: false,
        }
    }

    fn select(&mut self, id: Option<String>) {
        self.selected = id;
        self.reveal = false;
        self.editor = None;
    }

    fn start_edit(&mut self, entry: Entry, is_new: bool) {
        self.editor = Some(Editor { draft: entry, is_new, show_password: is_new, error: None });
    }
}

pub fn show(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) -> Option<Next> {
    let mut next = None;

    // Global shortcuts.
    let (lock_key, find_key, new_key) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::L),
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::F),
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
        )
    });
    if lock_key {
        return Some(Next::Locked);
    }
    if new_key && u.dialog.is_none() {
        u.selected = None;
        u.start_edit(Entry::new(), true);
    }

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("🛡 SeePass").heading().color(ACCENT));
            ui.add_space(12.0);
            let search =
                ui.add(egui::TextEdit::singleline(&mut u.search).hint_text("🔍 Search (Ctrl+F)").desired_width(260.0));
            if find_key {
                search.request_focus();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🔒 Lock").on_hover_text("Ctrl+L").clicked() {
                    next = Some(Next::Locked);
                }
                if ui.selectable_label(u.settings_open, "⚙ Settings").clicked() {
                    u.settings_open = !u.settings_open;
                }
                if ui.selectable_label(u.generator.open, "🎲 Generator").clicked() {
                    u.generator.open = !u.generator.open;
                }
                if ui.button("➕ New").on_hover_text("Ctrl+N").clicked() {
                    u.selected = None;
                    u.start_edit(Entry::new(), true);
                }
            });
        });
        ui.add_space(2.0);
    });

    entry_list(ctx, u);

    egui::CentralPanel::default().show(ctx, |ui| {
        #[cfg(feature = "legacy-import")]
        legacy_hint(ui, u);

        if u.editor.is_some() {
            editor(ui, u, shared);
        } else if let Some(id) = u.selected.clone() {
            details(ui, u, shared, &id);
        } else {
            ui.centered_and_justified(|ui| {
                let n = u.vault.entries().len();
                ui.label(
                    RichText::new(if n == 0 {
                        "Your vault is empty. Press ➕ New to add your first login.".to_string()
                    } else {
                        format!("{n} entries. Select one on the left.")
                    })
                    .weak(),
                );
            });
        }
    });

    generator_window(ctx, u, shared);
    if let Some(n) = dialogs::settings_window(ctx, u, shared) {
        next = Some(n);
    }
    if let Some(n) = dialogs::show_dialog(ctx, u, shared) {
        next = Some(n);
    }
    next
}

fn entry_list(ctx: &egui::Context, u: &mut Unlocked) {
    egui::SidePanel::left("entries").resizable(true).default_width(280.0).min_width(200.0).show(ctx, |ui| {
        let mut items: Vec<(&str, &str, &str)> = u
            .vault
            .entries()
            .iter()
            .filter(|e| e.matches(&u.search))
            .map(|e| (e.id.as_str(), e.title.as_str(), e.username.as_str()))
            .collect();
        items.sort_by_key(|(_, t, _)| t.to_lowercase());

        ui.add_space(4.0);
        ui.label(RichText::new(format!("{} of {}", items.len(), u.vault.entries().len())).small().weak());
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                let style = ui.style().clone();
                for (id, title, user) in items {
                    let selected = u.selected.as_deref() == Some(id);
                    let mut job = egui::text::LayoutJob::default();
                    RichText::new(if title.is_empty() { "(untitled)" } else { title }).strong().append_to(
                        &mut job,
                        &style,
                        egui::FontSelection::Default,
                        egui::Align::Center,
                    );
                    if !user.is_empty() {
                        RichText::new(format!("\n{user}")).small().weak().append_to(
                            &mut job,
                            &style,
                            egui::FontSelection::Default,
                            egui::Align::Center,
                        );
                    }
                    if ui.selectable_label(selected, job).clicked() {
                        clicked = Some(id.to_string());
                    }
                }
            });
        });
        if let Some(id) = clicked {
            u.select(Some(id));
        }
    });
}

fn details(ui: &mut egui::Ui, u: &mut Unlocked, shared: &mut Shared, id: &str) {
    let Some(entry) = u.vault.get(id) else {
        u.select(None);
        return;
    };
    let entry = entry.clone();

    ui.add_space(6.0);
    ui.heading(if entry.title.is_empty() { "(untitled)" } else { &entry.title });
    ui.label(RichText::new(format!("Updated {}", widgets::ago(entry.updated))).small().weak());
    ui.add_space(8.0);

    egui::Grid::new("details").num_columns(3).spacing([12.0, 10.0]).show(ui, |ui| {
        ui.label(RichText::new("Username").weak());
        ui.label(RichText::new(&entry.username).monospace());
        if ui.add_enabled(!entry.username.is_empty(), egui::Button::new("📋 Copy")).clicked() {
            shared.copy_secret("Username", &entry.username);
        }
        ui.end_row();

        ui.label(RichText::new("Password").weak());
        let shown =
            if u.reveal { entry.password.clone() } else { "•".repeat(entry.password.chars().count().min(16)) };
        ui.label(RichText::new(shown).monospace());
        ui.horizontal(|ui| {
            if ui.add_enabled(!entry.password.is_empty(), egui::Button::new("📋 Copy")).clicked() {
                shared.copy_secret("Password", &entry.password);
            }
            if ui.selectable_label(u.reveal, "👁").on_hover_text("Reveal").clicked() {
                u.reveal = !u.reveal;
            }
        });
        ui.end_row();

        if !entry.url.is_empty() {
            ui.label(RichText::new("Website").weak());
            ui.label(&entry.url);
            let url = if entry.url.contains("://") { entry.url.clone() } else { format!("https://{}", entry.url) };
            ui.hyperlink_to("Open ↗", url);
            ui.end_row();
        }
    });

    if !entry.notes.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("Notes").weak());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(&entry.notes);
        });
    }

    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if ui.button("✏ Edit").clicked() {
            u.start_edit(entry.clone(), false);
        }
        if ui.button(RichText::new("🗑 Delete").color(DANGER)).clicked() {
            u.dialog = Some(Dialog::ConfirmDelete { id: entry.id.clone(), title: entry.title.clone() });
        }
    });
}

fn editor(ui: &mut egui::Ui, u: &mut Unlocked, shared: &mut Shared) {
    let Some(ed) = u.editor.as_mut() else { return };
    let mut save = false;
    let mut cancel = false;

    ui.add_space(6.0);
    ui.heading(if ed.is_new { "New entry" } else { "Edit entry" });
    ui.add_space(8.0);
    let label_w = 90.0;
    let field_w = (ui.available_width() - label_w - 20.0).max(200.0);

    egui::Grid::new("editor").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
        ui.label("Title");
        let title =
            ui.add(egui::TextEdit::singleline(&mut ed.draft.title).hint_text("e.g. GitHub").desired_width(field_w));
        if ed.is_new && ed.draft.title.is_empty() && !title.has_focus() && ed.error.is_none() {
            title.request_focus();
        }
        ui.end_row();

        ui.label("Username");
        ui.add(egui::TextEdit::singleline(&mut ed.draft.username).hint_text("user or e-mail").desired_width(field_w));
        ui.end_row();

        ui.label("Password");
        ui.vertical(|ui| {
            ui.set_width(field_w);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut ed.draft.password)
                        .password(!ed.show_password)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(field_w - 90.0),
                );
                if ui.selectable_label(ed.show_password, "👁").clicked() {
                    ed.show_password = !ed.show_password;
                }
                if ui.button("🎲").on_hover_text("Generate with the generator's settings").clicked() {
                    if let Some(pw) = generator::generate(&u.generator.opts) {
                        ed.draft.password = pw.to_string();
                        ed.show_password = true;
                    }
                }
            });
            widgets::strength_meter(ui, &ed.draft.password);
        });
        ui.end_row();

        ui.label("Website");
        ui.add(egui::TextEdit::singleline(&mut ed.draft.url).hint_text("https://...").desired_width(field_w));
        ui.end_row();

        ui.label("Notes");
        ui.add(egui::TextEdit::multiline(&mut ed.draft.notes).desired_rows(4).desired_width(field_w));
        ui.end_row();
    });

    widgets::error_label(ui, &ed.error);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        save = ui.add(egui::Button::new("💾 Save").fill(ACCENT)).on_hover_text("Ctrl+S").clicked()
            || ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
    });

    if save {
        if ed.draft.title.trim().is_empty() {
            ed.error = Some("Give the entry a title.".into());
            return;
        }
        let entry = ed.draft.clone();
        let id = entry.id.clone();
        match u.vault.upsert(entry) {
            Ok(()) => {
                shared.toast("Saved");
                u.select(Some(id));
            }
            Err(e) => ed.error = Some(format!("Could not save: {e}")),
        }
    } else if cancel {
        let id = (!ed.is_new).then(|| ed.draft.id.clone());
        u.select(id);
    }
}

fn generator_window(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) {
    let mut open = u.generator.open;
    egui::Window::new("🎲 Password generator").open(&mut open).resizable(false).default_width(380.0).show(ctx, |ui| {
        let g = &mut u.generator;
        let before = g.opts;
        ui.add(egui::Slider::new(&mut g.opts.length, generator::MIN_LENGTH..=generator::MAX_LENGTH).text("length"));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut g.opts.lowercase, "a-z");
            ui.checkbox(&mut g.opts.uppercase, "A-Z");
            ui.checkbox(&mut g.opts.digits, "0-9");
            ui.checkbox(&mut g.opts.symbols, "!@#");
            ui.checkbox(&mut g.opts.avoid_ambiguous, "avoid look-alikes (l1O0)");
        });
        let regenerate = ui.button("↻ Regenerate").clicked();
        if regenerate || g.opts != before {
            g.output = generator::generate(&g.opts).unwrap_or_default();
        }
        ui.add(
            egui::TextEdit::singleline(&mut g.output.as_str())
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
        ui.label(RichText::new(format!("≈ {:.0} bits of entropy", g.opts.entropy_bits())).small().weak());
        ui.horizontal(|ui| {
            if ui.add_enabled(!g.output.is_empty(), egui::Button::new("📋 Copy")).clicked() {
                let pw = g.output.clone();
                shared.copy_secret("Password", &pw);
            }
            if let Some(ed) = u.editor.as_mut() {
                if ui.add_enabled(!g.output.is_empty(), egui::Button::new("Use in entry")).clicked() {
                    ed.draft.password = g.output.to_string();
                    ed.show_password = true;
                }
            }
        });
    });
    u.generator.open = open;
}

#[cfg(feature = "legacy-import")]
fn legacy_hint(ui: &mut egui::Ui, u: &mut Unlocked) {
    if u.legacy_hint_dismissed || !u.vault.entries().is_empty() || u.dialog.is_some() {
        return;
    }
    let Some(dir) = seepass::legacy::find_default() else { return };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("SeePass 7 data found").strong());
        ui.label(format!("Your old vault in {} can be imported into this one.", dir.display()));
        ui.horizontal(|ui| {
            if ui.button("Import now...").clicked() {
                u.dialog = Some(Dialog::import_legacy(dir.clone()));
            }
            if ui.button("Dismiss").clicked() {
                u.legacy_hint_dismissed = true;
            }
        });
    });
    ui.add_space(8.0);
}
