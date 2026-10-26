//! SpeakUp 6M — a voice-first English speaking & listening trainer (macOS / Windows).
//! Native Rust GUI (egui); Claude API for all language intelligence; whisper.cpp for offline
//! speech recognition; a neural voice (Kokoro, offline) for text-to-speech, OS voices as fallback.
//!
//! The app is a library so the desktop binary (main.rs) and the Android build (android.rs,
//! a shared library loaded by the activity) run the same code.

pub mod app;
pub mod applog;
mod art;
mod audio;
mod claude;
mod content;
mod curriculum;
mod dictionary;
mod expressions;
mod freetalk;
mod grammar;
mod grammar_book;
mod idioms;
mod language;
mod library;
mod mic_permission;
mod neural_tts;
mod passages;
mod print;
mod models;
mod offline_talk;
mod scoring;
mod screens;
mod srs;
pub mod storage;
mod speed_scripts;
mod web;
mod scenes;
mod credits;
mod tenses;
mod stt;
mod theme;
mod voice;
mod widgets;
mod wordbank;

#[cfg(test)]
mod functional_tests;

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "ios")]
pub mod ios;
