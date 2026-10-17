//! SpeakUp 6M — a voice-first English speaking & listening trainer (macOS / Windows / Linux).
//! Native Rust GUI (egui); Claude API for all language intelligence; whisper.cpp for offline
//! speech recognition; OS voices for text-to-speech.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod art;
mod audio;
mod claude;
mod content;
mod curriculum;
mod language;
mod mic_permission;
mod neural_tts;
mod models;
mod scoring;
mod screens;
mod srs;
mod storage;
mod stt;
mod theme;
mod voice;
mod widgets;
mod youtube;

#[cfg(test)]
mod functional_tests;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakUp 6M — 영어 6개월 완성")
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([900.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("SpeakUp 6M", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
