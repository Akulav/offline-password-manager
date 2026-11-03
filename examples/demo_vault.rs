//! Create a vault full of fake entries for trying out the UI.
//!
//! ```sh
//! SEEPASS_HOME=./demo cargo run --example demo_vault
//! SEEPASS_HOME=./demo cargo run
//! ```
//!
//! The master password is printed on stdout. Never point this at a real vault.

use seepass::crypto::KdfParams;
use seepass::generator::{generate, GeneratorOptions};
use seepass::settings::AppPaths;
use seepass::vault::{Entry, Vault};

const MASTER: &str = "demo-vault-correct-horse-battery";

fn main() -> seepass::Result<()> {
    let path = std::env::args().nth(1).map(Into::into).unwrap_or_else(|| AppPaths::discover().default_vault);
    let mut vault = Vault::create(&path, MASTER, KdfParams::STANDARD)?;
    let sites = [
        ("GitHub", "octo-dev", "https://github.com", ""),
        ("Bank", "jane.doe@example.com", "https://bank.example", "Card PIN is NOT stored here."),
        ("University portal", "student-4711", "https://portal.example.edu", "Exam schedule under Courses."),
        ("Steam", "gamer_jane", "store.steampowered.com", ""),
        ("Wi-Fi at home", "", "", "Router admin is 192.168.0.1"),
    ];
    for (i, (title, user, url, notes)) in sites.into_iter().enumerate() {
        let mut e = Entry::new();
        e.title = title.into();
        e.username = user.into();
        e.url = url.into();
        e.notes = notes.into();
        e.password = generate(&GeneratorOptions::default()).unwrap().to_string();
        match i {
            0 => e.favorite = true,
            // Steam and the university portal share a weak password, which shows the reuse warning.
            2 | 3 => e.password = "Summer2019!".into(),
            // The Wi-Fi password is two years old, which shows the rotation hint.
            4 => e.password_changed = e.created.saturating_sub(2 * 365 * 86_400),
            _ => {}
        }
        vault.upsert(e)?;
    }
    println!("Created {} with {} entries.", path.display(), vault.entries().len());
    println!("Master password: {MASTER}");
    Ok(())
}
