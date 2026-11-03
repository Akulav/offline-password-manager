//! egui desktop front end.

mod clipboard;
mod dialogs;
mod lock;
mod task;
#[cfg(test)]
mod tests;
mod vault_view;
mod widgets;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use seepass::settings::{AppPaths, Settings};
use seepass::vault::Vault;

use clipboard::ClipboardGuard;

pub struct App {
    shared: Shared,
    screen: Screen,
}

/// State that outlives any one screen.
pub struct Shared {
    pub paths: AppPaths,
    pub settings: Settings,
    pub vault_path: PathBuf,
    pub clipboard: ClipboardGuard,
    pub logo: Option<egui::TextureHandle>,
    toast: Option<Toast>,
    last_activity: Instant,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

enum Screen {
    Setup(lock::SetupState),
    Locked(lock::LockState),
    Unlocked(Box<vault_view::Unlocked>),
}

/// Screen transitions requested by a screen's `show` function.
pub enum Next {
    Setup,
    Locked,
    Unlocked(Vault),
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, vault_override: Option<PathBuf>) -> Self {
        Self::with_paths(&cc.egui_ctx, AppPaths::discover(), vault_override)
    }

    /// Construct without eframe (used by the UI tests).
    pub fn with_paths(ctx: &egui::Context, paths: AppPaths, vault_override: Option<PathBuf>) -> Self {
        let settings = Settings::load(&paths.settings);
        let vault_path =
            vault_override.or_else(|| settings.vault_path.clone()).unwrap_or_else(|| paths.default_vault.clone());
        let screen = initial_screen(&vault_path);
        let shared = Shared {
            paths,
            settings,
            vault_path,
            clipboard: ClipboardGuard::new(),
            logo: widgets::load_logo(ctx),
            toast: None,
            last_activity: Instant::now(),
        };
        widgets::apply_style(ctx, shared.settings.dark_mode);
        Self { shared, screen }
    }

    /// One frame of the whole app.
    pub fn frame(&mut self, ctx: &egui::Context) {
        let active = ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving());
        if active {
            self.shared.last_activity = Instant::now();
        }

        let next_clear = self.shared.clipboard.tick();

        if let Screen::Unlocked(_) = self.screen {
            let limit = self.shared.settings.auto_lock_secs;
            if limit > 0 && self.shared.last_activity.elapsed() >= Duration::from_secs(limit) {
                self.lock("Locked after inactivity");
            }
            // Keep ticking while idle so the auto-lock timer can fire.
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if let Some(d) = next_clear {
            ctx.request_repaint_after(d);
        }

        widgets::status_bar(ctx, &mut self.shared, matches!(self.screen, Screen::Unlocked(_)));

        let next = match &mut self.screen {
            Screen::Setup(s) => lock::setup(ctx, s, &mut self.shared),
            Screen::Locked(s) => lock::unlock(ctx, s, &mut self.shared),
            Screen::Unlocked(u) => vault_view::show(ctx, u, &mut self.shared),
        };
        match next {
            Some(Next::Locked) if matches!(self.screen, Screen::Unlocked(_)) => self.lock("Vault locked"),
            Some(n) => self.go(n),
            None => {}
        }
    }

    #[cfg(test)]
    pub fn is_unlocked(&self) -> bool {
        matches!(self.screen, Screen::Unlocked(_))
    }

    fn go(&mut self, next: Next) {
        self.screen = match next {
            Next::Setup => Screen::Setup(Default::default()),
            Next::Locked => Screen::Locked(Default::default()),
            Next::Unlocked(vault) => {
                self.shared.last_activity = Instant::now();
                Screen::Unlocked(Box::new(vault_view::Unlocked::new(vault)))
            }
        };
    }

    fn lock(&mut self, reason: &str) {
        // Dropping the Unlocked screen drops the Vault, which zeroizes the key
        // and every entry.
        self.shared.clipboard.clear_if_ours();
        self.go(Next::Locked);
        self.shared.toast(reason);
    }
}

fn initial_screen(vault_path: &std::path::Path) -> Screen {
    if vault_path.exists() {
        Screen::Locked(Default::default())
    } else {
        Screen::Setup(Default::default())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

impl Shared {
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), error: false, until: Instant::now() + Duration::from_secs(4) });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), error: true, until: Instant::now() + Duration::from_secs(8) });
    }

    /// Time until auto-lock fires, or `None` when auto-lock is off.
    pub fn auto_lock_remaining(&self) -> Option<Duration> {
        let limit = self.settings.auto_lock_secs;
        (limit > 0).then(|| Duration::from_secs(limit).saturating_sub(self.last_activity.elapsed()))
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.paths.settings) {
            self.error(format!("Could not save settings: {e}"));
        }
    }

    /// Copy a secret, scheduling a clipboard wipe per the user's setting.
    pub fn copy_secret(&mut self, what: &str, text: &str) {
        let secs = self.settings.clipboard_clear_secs;
        let clear = (secs > 0).then(|| Duration::from_secs(secs));
        match self.clipboard.copy(text, clear) {
            Ok(()) if secs > 0 => self.toast(format!("{what} copied - clipboard clears in {secs} s")),
            Ok(()) => self.toast(format!("{what} copied")),
            Err(e) => self.error(format!("Copy failed: {e}")),
        }
    }

    /// Point the app at a different vault file and remember the choice.
    pub fn set_vault_path(&mut self, path: PathBuf) {
        self.settings.vault_path = (path != self.paths.default_vault).then(|| path.clone());
        self.vault_path = path;
        self.save_settings();
    }
}
