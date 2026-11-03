//! The unlocked vault: entry list, details, editor and generator.

use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Modifiers, RichText};
use seepass::generator::{self, GeneratorOptions};
use seepass::strength;
use seepass::vault::{Entry, Vault};
use zeroize::Zeroizing;

use super::dialogs::{self, Dialog};
use super::widgets::{self, DANGER, WARNING};
use super::{Next, Shared};

/// Passwords older than this get a gentle "consider changing" note.
const OLD_PASSWORD_SECS: u64 = 365 * 86_400;
const UNDO_WINDOW: Duration = Duration::from_secs(10);

pub struct Unlocked {
    pub vault: Vault,
    search: String,
    sort: Sort,
    selected: Option<String>,
    editor: Option<Editor>,
    reveal: bool,
    generator: Generator,
    pub settings_open: bool,
    pub dialog: Option<Dialog>,
    /// Last deleted entry, offered for undo for a few seconds.
    pub undo: Option<(Entry, Instant)>,
    #[cfg_attr(not(feature = "legacy-import"), allow(dead_code))]
    legacy_hint_dismissed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Recent,
}

/// Where the user wants to go next; held back while unsaved edits are pending.
#[derive(Clone, PartialEq, Eq)]
pub enum Nav {
    Select(Option<String>),
    New,
}

struct Editor {
    draft: Entry,
    /// What the draft looked like when editing started, to detect unsaved changes.
    original: Entry,
    is_new: bool,
    show_password: bool,
    error: Option<String>,
}

impl Editor {
    fn dirty(&self) -> bool {
        self.draft != self.original
    }
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
            sort: Sort::Name,
            selected: None,
            editor: None,
            reveal: false,
            generator: Generator { open: false, opts, output: generator::generate(&opts).unwrap_or_default() },
            settings_open: false,
            dialog: None,
            undo: None,
            legacy_hint_dismissed: false,
        }
    }

    /// Navigate, or ask first if the editor has unsaved changes.
    pub fn request(&mut self, nav: Nav) {
        if self.editor.as_ref().is_some_and(Editor::dirty) {
            self.dialog = Some(Dialog::DiscardChanges(nav));
        } else {
            self.apply(nav);
        }
    }

    pub fn apply(&mut self, nav: Nav) {
        self.reveal = false;
        self.editor = None;
        match nav {
            Nav::Select(id) => self.selected = id,
            Nav::New => {
                self.selected = None;
                let e = Entry::new();
                self.editor =
                    Some(Editor { original: e.clone(), draft: e, is_new: true, show_password: true, error: None });
            }
        }
    }

    fn start_edit(&mut self, entry: Entry) {
        self.editor =
            Some(Editor { original: entry.clone(), draft: entry, is_new: false, show_password: false, error: None });
    }

    /// Ids of the entries currently shown in the list, in display order.
    fn visible_ids(&self) -> Vec<String> {
        let mut items: Vec<&Entry> = self.vault.entries().iter().filter(|e| e.matches(&self.search)).collect();
        match self.sort {
            Sort::Name => items.sort_by_cached_key(|e| (!e.favorite, e.title.to_lowercase())),
            Sort::Recent => items.sort_by_key(|e| (!e.favorite, std::cmp::Reverse(e.updated))),
        }
        items.into_iter().map(|e| e.id.clone()).collect()
    }
}

pub fn show(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) -> Option<Next> {
    let mut next = None;
    let typing = ctx.memory(|m| m.focused().is_some());
    let modal_open = u.dialog.is_some();

    // Global shortcuts.
    let (lock_key, find_key, new_key) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::COMMAND, Key::L),
            i.consume_key(Modifiers::COMMAND, Key::F),
            i.consume_key(Modifiers::COMMAND, Key::N),
        )
    });
    if lock_key {
        return Some(Next::Locked);
    }
    if new_key && !modal_open {
        u.request(Nav::New);
    }
    if !typing && !modal_open && u.editor.is_none() {
        list_shortcuts(ctx, u, shared);
    }
    if u.undo.as_ref().is_some_and(|(_, at)| at.elapsed() >= UNDO_WINDOW) {
        u.undo = None;
    }

    egui::TopBottomPanel::top("toolbar").frame(toolbar_frame(ctx)).show(ctx, |ui| {
        ui.horizontal(|ui| {
            widgets::logo(ui, &shared.logo, 26.0);
            ui.label(RichText::new("SeePass").size(18.0).strong());
            ui.add_space(16.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut u.search)
                    .hint_text("🔍  Search   Ctrl+F")
                    .desired_width(280.0)
                    .margin(egui::vec2(8.0, 5.0)),
            );
            if find_key {
                search.request_focus();
            }
            if search.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                u.search.clear();
                search.surrender_focus();
            }
            if !u.search.is_empty() && ui.small_button("✖").on_hover_text("Clear search").clicked() {
                u.search.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🔒 Lock").on_hover_text("Lock now (Ctrl+L)").clicked() {
                    next = Some(Next::Locked);
                }
                if ui.selectable_label(u.settings_open, "⚙ Settings").clicked() {
                    u.settings_open = !u.settings_open;
                }
                if ui.selectable_label(u.generator.open, "🎲 Generator").clicked() {
                    u.generator.open = !u.generator.open;
                }
                if ui.add(widgets::primary_button("➕ New")).on_hover_text("Add an entry (Ctrl+N)").clicked() {
                    u.request(Nav::New);
                }
            });
        });
    });

    entry_list(ctx, u);

    egui::CentralPanel::default().show(ctx, |ui| {
        undo_banner(ui, u, shared);
        #[cfg(feature = "legacy-import")]
        legacy_hint(ui, u);

        if u.editor.is_some() {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| editor(ui, u, shared));
        } else if let Some(id) = u.selected.clone() {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| details(ui, u, shared, &id));
        } else {
            empty_state(ui, u, &shared.logo);
        }
    });

    generator_window(ctx, u, shared);
    if let Some(n) = dialogs::settings_window(ctx, u, shared) {
        next = Some(n);
    }
    if let Some(n) = dialogs::show_dialog(ctx, u, shared) {
        next = Some(n);
    }
    if u.undo.is_some() {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    next
}

fn toolbar_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(14, 10))
}

/// Arrow keys move through the list; Ctrl+C / Ctrl+B copy; E edits; Delete deletes.
fn list_shortcuts(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) {
    let (up, down, copy_pw, copy_user, edit, delete, fav) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::COMMAND, Key::C),
            i.consume_key(Modifiers::COMMAND, Key::B),
            i.consume_key(Modifiers::NONE, Key::E) || i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Delete),
            i.consume_key(Modifiers::NONE, Key::S),
        )
    });
    if up || down {
        let ids = u.visible_ids();
        if !ids.is_empty() {
            let pos = u.selected.as_ref().and_then(|s| ids.iter().position(|i| i == s));
            let new = match (pos, down) {
                (None, true) => 0,
                (None, false) => ids.len() - 1,
                (Some(p), true) => (p + 1).min(ids.len() - 1),
                (Some(p), false) => p.saturating_sub(1),
            };
            u.apply(Nav::Select(Some(ids[new].clone())));
        }
    }
    let Some(entry) = u.selected.as_deref().and_then(|id| u.vault.get(id)).cloned() else { return };
    if copy_pw && !entry.password.is_empty() {
        shared.copy_secret("Password", &entry.password);
    }
    if copy_user && !entry.username.is_empty() {
        shared.copy_secret("Username", &entry.username);
    }
    if edit {
        u.start_edit(entry.clone());
    }
    if fav {
        toggle_favorite(u, shared, &entry);
    }
    if delete {
        u.dialog = Some(Dialog::ConfirmDelete { id: entry.id.clone(), title: entry.title.clone() });
    }
}

fn toggle_favorite(u: &mut Unlocked, shared: &mut Shared, entry: &Entry) {
    if let Err(e) = u.vault.set_favorite(&entry.id, !entry.favorite) {
        shared.error(format!("Could not save: {e}"));
    }
}

fn entry_list(ctx: &egui::Context, u: &mut Unlocked) {
    egui::SidePanel::left("entries")
        .resizable(true)
        .default_width(290.0)
        .width_range(220.0..=480.0)
        .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(8, 8)))
        .show(ctx, |ui| {
            let ids = u.visible_ids();
            ui.horizontal(|ui| {
                let total = u.vault.entries().len();
                let label = if u.search.is_empty() {
                    format!("{total} entries")
                } else {
                    format!("{} of {total} match", ids.len())
                };
                ui.label(RichText::new(label).small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("sort")
                        .selected_text(match u.sort {
                            Sort::Name => "A–Z",
                            Sort::Recent => "Recent",
                        })
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut u.sort, Sort::Name, "A–Z");
                            ui.selectable_value(&mut u.sort, Sort::Recent, "Recently edited");
                        });
                });
            });
            ui.add_space(2.0);

            let mut clicked = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if ids.is_empty() && !u.search.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("No matches").weak()));
                }
                let first_regular = ids.iter().position(|id| u.vault.get(id).is_some_and(|e| !e.favorite));
                for (i, id) in ids.iter().enumerate() {
                    let Some(e) = u.vault.get(id) else { continue };
                    if i == 0 && e.favorite {
                        section(ui, "★ FAVORITES");
                    }
                    if Some(i) == first_regular && i > 0 {
                        ui.add_space(6.0);
                        section(ui, "ALL ENTRIES");
                    }
                    let selected = u.selected.as_deref() == Some(id.as_str());
                    let resp = widgets::entry_row(ui, selected, &e.title, &e.username, e.favorite);
                    if selected && ui.input(|i| i.key_down(Key::ArrowUp) || i.key_down(Key::ArrowDown)) {
                        resp.scroll_to_me(None);
                    }
                    if resp.clicked() {
                        clicked = Some(id.clone());
                    }
                }
            });
            if let Some(id) = clicked {
                u.request(Nav::Select(Some(id)));
            }
        });
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().weak());
}

fn empty_state(ui: &mut egui::Ui, u: &mut Unlocked, logo: &Option<egui::TextureHandle>) {
    let n = u.vault.entries().len();
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.24).max(20.0));
        widgets::logo(ui, logo, 64.0);
        ui.add_space(4.0);
        if n == 0 {
            ui.heading("Your vault is empty");
            ui.label(RichText::new("Add your first login, or bring your passwords over.").weak());
            ui.add_space(8.0);
            let add = widgets::primary_button("➕ Add your first login").min_size(egui::vec2(220.0, 36.0));
            if ui.add(add).clicked() {
                u.request(Nav::New);
            }
            ui.horizontal(|ui| {
                // Centre the two buttons as a group.
                ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                if ui.button("Import encrypted backup...").clicked() {
                    dialogs::pick_backup_import(u);
                }
                #[cfg(feature = "legacy-import")]
                if ui.button("Import from SeePass 7...").clicked() {
                    dialogs::pick_legacy_import(u);
                }
            });
        } else {
            ui.heading(format!("{n} entries"));
            ui.label(
                RichText::new("Select an entry on the left, or move through the list with the arrow keys.").weak(),
            );
            ui.add_space(12.0);
            // Centre the fixed-width help card.
            let w = 440.0_f32.min(ui.available_width());
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(w);
                    shortcut_help(ui);
                });
            });
        }
    });
}

fn shortcut_help(ui: &mut egui::Ui) {
    let rows = [
        ("Ctrl+N", "new entry"),
        ("Ctrl+F", "search"),
        ("↑ ↓", "move in list"),
        ("E", "edit"),
        ("Ctrl+C", "copy password"),
        ("Ctrl+B", "copy username"),
        ("S", "favorite"),
        ("Del", "delete"),
        ("Ctrl+S", "save"),
        ("Ctrl+L", "lock"),
    ];
    widgets::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("shortcuts").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
            for pair in rows.chunks(2) {
                for (k, what) in pair {
                    // Monospace: the proportional font has no arrow glyphs.
                    ui.label(RichText::new(*k).monospace().strong());
                    ui.label(RichText::new(*what).weak());
                }
                ui.end_row();
            }
        });
    });
}

fn undo_banner(ui: &mut egui::Ui, u: &mut Unlocked, shared: &mut Shared) {
    let Some((entry, at)) = &u.undo else { return };
    let left = UNDO_WINDOW.saturating_sub(at.elapsed()).as_secs() + 1;
    let title = entry.title.clone();
    let mut restore = false;
    widgets::banner(WARNING).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(format!("Deleted \"{title}\"."));
            restore = ui.button("Undo").clicked();
            ui.label(RichText::new(format!("{left} s")).small().weak());
        });
    });
    ui.add_space(8.0);
    if restore {
        if let Some((entry, _)) = u.undo.take() {
            let id = entry.id.clone();
            match u.vault.restore(entry) {
                Ok(()) => {
                    shared.toast("Entry restored");
                    u.apply(Nav::Select(Some(id)));
                }
                Err(e) => shared.error(format!("Could not restore: {e}")),
            }
        }
    }
}

fn details(ui: &mut egui::Ui, u: &mut Unlocked, shared: &mut Shared, id: &str) {
    let Some(entry) = u.vault.get(id).cloned() else {
        u.apply(Nav::Select(None));
        return;
    };
    let reused = u.vault.reuse_count(id);
    let title = if entry.title.is_empty() { "(untitled)" } else { entry.title.as_str() };

    // Header: avatar, title, favorite, actions.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        widgets::avatar(ui, title, 52.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).heading().strong());
            let mut sub = format!("Edited {}", widgets::ago(entry.updated));
            if !entry.url.is_empty() {
                sub = format!("{} · {sub}", short_host(&entry.url));
            }
            ui.label(RichText::new(sub).small().weak());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(RichText::new("🗑").color(DANGER)).on_hover_text("Delete (Del)").clicked() {
                u.dialog = Some(Dialog::ConfirmDelete { id: entry.id.clone(), title: entry.title.clone() });
            }
            if ui.button("✏ Edit").on_hover_text("Edit (E)").clicked() {
                u.start_edit(entry.clone());
            }
            let star = if entry.favorite { RichText::new("★").color(WARNING) } else { RichText::new("☆") };
            let tip = if entry.favorite { "Remove from favorites (S)" } else { "Add to favorites (S)" };
            if ui.button(star.size(18.0)).on_hover_text(tip).clicked() {
                toggle_favorite(u, shared, &entry);
            }
        });
    });
    ui.add_space(10.0);

    // Warnings.
    if reused > 0 {
        widgets::banner(WARNING).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let others = if reused == 1 { "1 other entry".to_string() } else { format!("{reused} other entries") };
            ui.label(format!("⚠ This password is also used by {others}. If one site leaks, all of them are exposed."));
        });
        ui.add_space(6.0);
    }
    if !entry.password.is_empty() && entry.password_age_secs() > OLD_PASSWORD_SECS {
        widgets::banner(WARNING).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(format!(
                "⏳ Password last changed {}. Consider rotating it.",
                widgets::human_age(entry.password_age_secs())
            ));
        });
        ui.add_space(6.0);
    }

    // Fields.
    if !entry.username.is_empty() {
        field_card(
            ui,
            "USERNAME",
            |ui| {
                ui.label(RichText::new(&entry.username).monospace().size(16.0));
            },
            |ui| {
                if ui.button("📋 Copy").on_hover_text("Ctrl+B").clicked() {
                    shared.copy_secret("Username", &entry.username);
                }
            },
        );
    }
    if !entry.password.is_empty() {
        let s = strength::assess(&entry.password, &[&entry.title, &entry.username]);
        let reveal = u.reveal;
        field_card(
            ui,
            "PASSWORD",
            |ui| {
                let shown =
                    if reveal { entry.password.clone() } else { "•".repeat(entry.password.chars().count().min(18)) };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(shown).monospace().size(16.0));
                    widgets::pill(ui, strength::label(s.score), widgets::score_color(s.score));
                });
            },
            |ui| {
                if ui.button("📋 Copy").on_hover_text("Ctrl+C").clicked() {
                    shared.copy_secret("Password", &entry.password);
                }
                if ui.selectable_label(u.reveal, "👁").on_hover_text(if u.reveal { "Hide" } else { "Reveal" }).clicked()
                {
                    u.reveal = !u.reveal;
                }
            },
        );
    }
    if !entry.url.is_empty() {
        let url = if entry.url.contains("://") { entry.url.clone() } else { format!("https://{}", entry.url) };
        field_card(
            ui,
            "WEBSITE",
            |ui| {
                ui.label(&entry.url);
            },
            |ui| {
                ui.hyperlink_to("Open ↗", &url);
            },
        );
    }
    if !entry.notes.is_empty() {
        field_card(
            ui,
            "NOTES",
            |ui| {
                ui.label(&entry.notes);
            },
            |_| {},
        );
    }
    if entry.username.is_empty() && entry.password.is_empty() && entry.url.is_empty() && entry.notes.is_empty() {
        ui.label(RichText::new("This entry has no details yet. Press Edit to add some.").weak());
    }

    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "Created {} · password changed {}",
            widgets::ago(entry.created),
            widgets::human_age(entry.password_age_secs())
        ))
        .small()
        .weak(),
    );
}

fn field_card(ui: &mut egui::Ui, label: &str, value: impl FnOnce(&mut egui::Ui), actions: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).small().strong().weak());
                value(ui);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
        });
    });
    ui.add_space(4.0);
}

fn short_host(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or(rest).trim_start_matches("www.")
}

fn editor(ui: &mut egui::Ui, u: &mut Unlocked, shared: &mut Shared) {
    let reuse = u.editor.as_ref().map_or(0, |ed| u.vault.count_with_password(&ed.draft.password, &ed.draft.id));
    let Some(ed) = u.editor.as_mut() else { return };
    let mut save = false;
    let mut cancel = false;
    let dirty = ed.dirty();

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let preview = if ed.draft.title.trim().is_empty() { "?" } else { ed.draft.title.as_str() };
        widgets::avatar(ui, preview, 44.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(if ed.is_new { "New entry" } else { "Edit entry" }).heading().strong());
            if dirty {
                ui.label(RichText::new("• Unsaved changes").small().color(WARNING));
            } else {
                ui.label(RichText::new("No changes").small().weak());
            }
        });
    });
    ui.add_space(10.0);

    let w = ui.available_width().min(560.0);
    let label = |ui: &mut egui::Ui, text: &str| ui.label(RichText::new(text).small().strong().weak());

    label(ui, "TITLE");
    let title = ui.add(egui::TextEdit::singleline(&mut ed.draft.title).hint_text("e.g. GitHub").desired_width(w));
    if ed.is_new && ed.draft.title.is_empty() && ed.error.is_none() && !ui.memory(|m| m.focused().is_some()) {
        title.request_focus();
    }

    label(ui, "USERNAME");
    ui.add(egui::TextEdit::singleline(&mut ed.draft.username).hint_text("user name or e-mail").desired_width(w));

    label(ui, "PASSWORD");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ed.draft.password)
                .password(!ed.show_password)
                .font(egui::TextStyle::Monospace)
                .desired_width(w - 84.0),
        );
        if ui.selectable_label(ed.show_password, "👁").on_hover_text("Show / hide").clicked() {
            ed.show_password = !ed.show_password;
        }
        if ui.button("🎲").on_hover_text("Generate (uses the generator's settings)").clicked() {
            if let Some(pw) = generator::generate(&u.generator.opts) {
                ed.draft.password = pw.to_string();
                ed.show_password = true;
            }
        }
    });
    ui.scope(|ui| {
        ui.set_max_width(w);
        widgets::strength_meter(ui, &ed.draft.password);
    });
    if reuse > 0 {
        ui.label(
            RichText::new(format!("⚠ Already used by {reuse} other entr{}", if reuse == 1 { "y" } else { "ies" }))
                .small()
                .color(WARNING),
        );
    }

    label(ui, "WEBSITE");
    ui.add(egui::TextEdit::singleline(&mut ed.draft.url).hint_text("https://example.com").desired_width(w));

    label(ui, "NOTES");
    ui.add(egui::TextEdit::multiline(&mut ed.draft.notes).desired_rows(4).desired_width(w));

    ui.checkbox(&mut ed.draft.favorite, "★ Favorite");

    widgets::error_label(ui, &ed.error);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let save_btn = widgets::primary_button("💾 Save");
        save = ui.add(save_btn).on_hover_text("Ctrl+S").clicked()
            || ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S));
        cancel = ui.button("Cancel").on_hover_text("Esc").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
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
                u.apply(Nav::Select(Some(id)));
            }
            Err(e) => ed.error = Some(format!("Could not save: {e}")),
        }
    } else if cancel {
        let back = (!ed.is_new).then(|| ed.draft.id.clone());
        u.request(Nav::Select(back));
    }
}

fn generator_window(ctx: &egui::Context, u: &mut Unlocked, shared: &mut Shared) {
    let mut open = u.generator.open;
    egui::Window::new("🎲 Password generator").open(&mut open).resizable(false).default_width(400.0).show(ctx, |ui| {
        let g = &mut u.generator;
        let before = g.opts;

        // The output first: it's what people came for.
        widgets::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(RichText::new(g.output.as_str()).monospace().size(17.0).strong())
                    .wrap_mode(egui::TextWrapMode::Wrap),
            );
        });
        let bits = g.opts.entropy_bits();
        let color = widgets::score_color(match bits as u32 {
            0..=39 => 0,
            40..=59 => 2,
            60..=79 => 3,
            _ => 4,
        });
        ui.label(RichText::new(format!("≈ {bits:.0} bits of entropy")).small().color(color));

        ui.add(egui::Slider::new(&mut g.opts.length, generator::MIN_LENGTH..=generator::MAX_LENGTH).text("length"));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut g.opts.lowercase, "a-z");
            ui.checkbox(&mut g.opts.uppercase, "A-Z");
            ui.checkbox(&mut g.opts.digits, "0-9");
            ui.checkbox(&mut g.opts.symbols, "!@#");
            ui.checkbox(&mut g.opts.avoid_ambiguous, "avoid look-alikes (l1O0)");
        });
        let mut regenerate = false;
        ui.horizontal(|ui| {
            regenerate = ui.button("🔄 Regenerate").clicked();
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
        if regenerate || g.opts != before {
            g.output = generator::generate(&g.opts).unwrap_or_default();
        }
    });
    u.generator.open = open;
}

#[cfg(feature = "legacy-import")]
fn legacy_hint(ui: &mut egui::Ui, u: &mut Unlocked) {
    if u.legacy_hint_dismissed || !u.vault.entries().is_empty() || u.dialog.is_some() {
        return;
    }
    let Some(dir) = seepass::legacy::find_default() else { return };
    widgets::banner(widgets::ACCENT).show(ui, |ui| {
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
