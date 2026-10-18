//! SpeakUp 6M — a voice-first English speaking & listening trainer (macOS / Windows / Linux).
//! Native Rust GUI (egui); Claude API for all language intelligence; whisper.cpp for offline
//! speech recognition; OS voices for text-to-speech.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod applog;
mod art;
mod audio;
mod claude;
mod content;
mod curriculum;
mod expressions;
mod grammar;
mod idioms;
mod language;
mod library;
mod mic_permission;
mod neural_tts;
mod passages;
mod models;
mod scoring;
mod screens;
mod srs;
mod storage;
mod stt;
mod theme;
mod voice;
mod widgets;
mod wordbank;
mod youtube;

#[cfg(test)]
mod functional_tests;

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
        // Always open at the fitted size instead of a size remembered from an earlier run.
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native("SpeakUp 6M", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
