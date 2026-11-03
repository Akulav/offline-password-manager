//! Headless UI tests: drive the real app through egui's accessibility tree.
//!
//! These need no window or GPU. Set `SEEPASS_UI_SNAPSHOTS=<dir>` to also render
//! each step to a PNG with wgpu (needs a GPU or a software adapter), which is
//! how the screenshots in the docs are made:
//!
//! ```sh
//! SEEPASS_UI_SNAPSHOTS=target/ui cargo test --bin seepass ui::tests -- --test-threads=1
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use seepass::crypto::KdfParams;
use seepass::settings::AppPaths;
use seepass::vault::{Entry, Vault};

use super::App;

const MASTER: &str = "ui-test-master-password-42";

struct Fixture {
    _dir: tempfile::TempDir,
    paths: AppPaths,
}

fn fixture(entries: &[(&str, &str, &str, bool)]) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths =
        AppPaths { default_vault: dir.path().join("vault.seepass"), settings: dir.path().join("settings.json") };
    if !entries.is_empty() {
        let mut v = Vault::create(&paths.default_vault, MASTER, KdfParams::INSECURE_FAST).unwrap();
        for (title, user, pw, fav) in entries {
            let mut e = Entry::new();
            e.title = (*title).into();
            e.username = (*user).into();
            e.password = (*pw).into();
            e.url = format!("https://{}.example", title.to_lowercase());
            e.favorite = *fav;
            v.upsert(e).unwrap();
        }
    }
    Fixture { _dir: dir, paths }
}

fn demo() -> Fixture {
    fixture(&[
        ("GitHub", "octo-dev", "k8#Qz!v2Lp@9wRmT", true),
        ("Bank", "jane@example.com", "Xr4!pL9#qW2zVb7&", false),
        ("Steam", "gamer_jane", "Summer2019!", false),
        ("University", "student-4711", "Summer2019!", false),
    ])
}

fn harness(paths: AppPaths) -> Harness<'static, Option<App>> {
    Harness::builder().with_size(egui::vec2(1000.0, 660.0)).build_state(
        move |ctx, app: &mut Option<App>| {
            app.get_or_insert_with(|| App::with_paths(ctx, paths.clone(), None)).frame(ctx);
        },
        None,
    )
}

/// Step until `done` holds (background tasks finish on their own thread).
fn wait_for(h: &mut Harness<'_, Option<App>>, what: &str, done: impl Fn(&Harness<'_, Option<App>>) -> bool) {
    let start = Instant::now();
    while !done(h) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        h.step();
        std::thread::sleep(Duration::from_millis(15));
    }
    h.run_steps(3);
}

fn type_text(h: &mut Harness<'_, Option<App>>, text: &str) {
    h.input_mut().events.push(egui::Event::Text(text.into()));
    h.step();
}

/// Render the current frame to `$SEEPASS_UI_SNAPSHOTS/<name>.png`, if requested.
fn snapshot(h: &mut Harness<'_, Option<App>>, name: &str) {
    let Ok(dir) = std::env::var("SEEPASS_UI_SNAPSHOTS") else { return };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    match h.render() {
        Ok(img) => img.save(dir.join(format!("{name}.png"))).unwrap(),
        Err(e) => eprintln!("could not render {name}: {e}"),
    }
}

fn unlocked(h: &Harness<'_, Option<App>>) -> bool {
    h.state().as_ref().is_some_and(App::is_unlocked)
}

fn unlock(h: &mut Harness<'_, Option<App>>) {
    h.run_steps(2); // first frame focuses the password field
    type_text(h, MASTER);
    h.press_key(egui::Key::Enter);
    wait_for(h, "unlock", unlocked);
}

#[test]
fn setup_screen_rejects_weak_master_password() {
    let f = fixture(&[]);
    let mut h = harness(f.paths.clone());
    h.run_steps(2);
    snapshot(&mut h, "01-setup");
    h.get_by_label("Create vault").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("at least 10 characters").is_some());
    assert!(!f.paths.default_vault.exists());
}

#[test]
fn wrong_password_shows_error_and_stays_locked() {
    let f = demo();
    let mut h = harness(f.paths.clone());
    h.run_steps(2);
    snapshot(&mut h, "02-lock");
    type_text(&mut h, "not the password");
    h.press_key(egui::Key::Enter);
    wait_for(&mut h, "error", |h| h.query_by_label_contains("wrong master password").is_some());
    assert!(!unlocked(&h));
}

#[test]
fn browse_details_edit_and_generator() {
    let f = demo();
    let mut h = harness(f.paths.clone());
    unlock(&mut h);
    snapshot(&mut h, "03-unlocked");

    // Favorites come first, then A-Z.
    assert!(h.query_by_label("★ FAVORITES").is_some());
    assert!(h.query_by_label("ALL ENTRIES").is_some());

    // Details of an entry whose password is reused.
    h.get_by_label("Steam").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("also used by 1 other entry").is_some());
    snapshot(&mut h, "04-details");

    // Edit: change the title and see the unsaved-changes marker.
    h.get_by_label("✏ Edit").click();
    h.run_steps(3);
    assert!(h.query_by_label("No changes").is_some());
    let is_title = |n: &egui_kittest::kittest::Node<'_>| {
        n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("Steam")
    };
    h.get_by(is_title).focus();
    h.run_steps(2);
    h.get_by(is_title).type_text(" (old)");
    h.run_steps(3);
    assert!(h.query_by_label("• Unsaved changes").is_some());
    snapshot(&mut h, "05-editor");

    // Leaving with unsaved changes asks first, and "Discard" throws them away.
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.query_by_label("Discard changes?").is_some());
    snapshot(&mut h, "06-discard");
    h.get_by_label("Discard").click();
    h.run_steps(3);
    assert!(h.query_by_label("Edit entry").is_none());
    assert!(h.query_by_label_contains("(old)").is_none());

    // Generator window.
    h.get_by_label("🎲 Generator").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("bits of entropy").is_some());
    snapshot(&mut h, "07-generator");
}

#[test]
fn new_entry_is_saved_and_selected() {
    let f = demo();
    let mut h = harness(f.paths.clone());
    unlock(&mut h);
    h.get_by_label("➕ New").click();
    h.run_steps(3);
    // The title field is focused automatically for new entries.
    type_text(&mut h, "Proton Mail");
    h.get_by_label("💾 Save").click();
    h.run_steps(3);
    // Shown both in the list and as the details heading.
    assert!(h.query_all_by_label("Proton Mail").count() >= 2);
    drop(h);
    let v = Vault::unlock(&f.paths.default_vault, MASTER).unwrap();
    assert!(v.entries().iter().any(|e| e.title == "Proton Mail"));
}

#[test]
fn delete_then_undo() {
    let f = demo();
    let mut h = harness(f.paths.clone());
    unlock(&mut h);
    h.get_by_label("Bank").click();
    h.run_steps(3);
    h.get_by_label("🗑").click();
    h.run_steps(3);
    snapshot(&mut h, "08-delete");
    h.get_by_label("🗑 Delete").click();
    h.run_steps(3);
    assert!(h.query_by_label("Bank").is_none());
    assert!(h.query_by_label("Undo").is_some());
    snapshot(&mut h, "09-undo");
    h.get_by_label("Undo").click();
    h.run_steps(3);
    assert!(h.query_all_by_label("Bank").count() >= 1);
}

#[test]
fn settings_window_and_lock() {
    let f = demo();
    let mut h = harness(f.paths.clone());
    unlock(&mut h);
    h.get_by_label("⚙ Settings").click();
    h.run_steps(3);
    assert!(h.query_by_label("🔑 Change master password...").is_some());
    snapshot(&mut h, "10-settings");
    h.get_by_label("🔒 Lock").click();
    h.run_steps(3);
    assert!(!unlocked(&h));
}

#[test]
fn light_theme_renders() {
    let f = demo();
    std::fs::write(&f.paths.settings, br#"{"dark_mode": false}"#).unwrap();
    let mut h = harness(f.paths.clone());
    unlock(&mut h);
    h.get_by_label("GitHub").click();
    h.run_steps(3);
    snapshot(&mut h, "11-light");
}
