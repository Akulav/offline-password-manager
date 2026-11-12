// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Install folder, moving a pre-8.2 vault into it, `--uninstall`.
    let startup = match seepass::install::on_startup(&args) {
        seepass::install::Startup::Exit => return Ok(()),
        seepass::install::Startup::Run(info) => info,
    };
    let vault_override = parse_args();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("SeePass")
        // Matches the .desktop file on Linux so the dock/taskbar shows the right icon.
        .with_app_id("seepass")
        .with_inner_size([1040.0, 680.0])
        .with_min_inner_size([760.0, 500.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/seepass-256.png")) {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    eframe::run_native(
        "SeePass",
        eframe::NativeOptions { viewport, centered: true, ..Default::default() },
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, vault_override, startup)))),
    )
}

/// `seepass [--vault] <path>` opens a specific vault file instead of the default.
fn parse_args() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--vault" {
            return args.next().map(PathBuf::from);
        }
        if seepass::install::is_install_flag(&arg.to_string_lossy()) {
            continue;
        }
        if !arg.to_string_lossy().starts_with('-') {
            return Some(PathBuf::from(arg));
        }
    }
    None
}
