//! Desktop entry point (macOS / Windows): opens the window at the size the layout needs.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use speakup::{app, applog, storage};

fn main() -> eframe::Result<()> {
    applog::init(&storage::data_dir());
    // Exactly sidebar + content column + margins (32 + 24) + scrollbar, so nothing is wasted.
    let width = app::SIDEBAR_WIDTH + app::CONTENT_WIDTH + 32.0 + 24.0 + 12.0;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakUp 6M — 영어 6개월 완성")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon_256.png")).unwrap_or_default())
            .with_inner_size([width, 800.0])
            .with_min_inner_size([width - 160.0, 600.0]),
        // Always open at the fitted size (sidebar + content column, ~1,200 wide). eframe's own
        // persistence is not compiled in: in eframe 0.31 `persist_window: false` only stops saving,
        // and an old saved size (2,186 wide) kept being restored.
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native("SpeakUp 6M", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
