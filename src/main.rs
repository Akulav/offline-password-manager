// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

fn main() -> eframe::Result {
    let vault_override = parse_args();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("SeePass {}", seepass::VERSION))
        .with_inner_size([980.0, 640.0])
        .with_min_inner_size([720.0, 480.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/shield.png")) {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    eframe::run_native(
        "SeePass",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, vault_override)))),
    )
}

/// `seepass [--vault] <path>` opens a specific vault file instead of the default.
fn parse_args() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--vault" {
            return args.next().map(PathBuf::from);
        }
        if !arg.to_string_lossy().starts_with('-') {
            return Some(PathBuf::from(arg));
        }
    }
    None
}
