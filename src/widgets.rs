//! Reusable UI pieces: background jobs, the big microphone button, and the speaking drill
//! (listen → say it out loud → word-by-word feedback → repeat N times).

use crate::claude::{Claude, CoachTip};
use crate::models::Chunk;
use crate::scoring::{self, ScoreResult, WordMark};
use crate::voice::Voice;
use anyhow::Result;
use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, Ui, Vec2};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub const ACCENT: Color32 = Color32::from_rgb(61, 123, 255);
pub const GOOD: Color32 = Color32::from_rgb(52, 199, 120);
pub const BAD: Color32 = Color32::from_rgb(255, 92, 92);
pub const WARN: Color32 = Color32::from_rgb(255, 184, 0);
pub const MUTED: Color32 = Color32::from_rgb(168, 178, 198);
pub const CARD: Color32 = Color32::from_rgb(22, 28, 44);
pub const BORDER: Color32 = Color32::from_rgb(40, 50, 76);
pub const BG: Color32 = Color32::from_rgb(12, 16, 27);
pub const SIDEBAR: Color32 = Color32::from_rgb(9, 12, 21);

// ---------------------------------------------------------------- background jobs

pub struct Pending<T>(Option<Receiver<Result<T>>>);

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Send + 'static> Pending<T> {
    pub fn spawn(&mut self, f: impl FnOnce() -> Result<T> + Send + 'static) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        self.0 = Some(rx);
    }

    pub fn from_receiver(&mut self, rx: Receiver<Result<T>>) {
        self.0 = Some(rx);
    }

    pub fn running(&self) -> bool {
        self.0.is_some()
    }

    pub fn poll(&mut self) -> Option<Result<T>> {
        let rx = self.0.as_ref()?;
        match rx.try_recv() {
            Ok(r) => {
                self.0 = None;
                Some(r)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.0 = None;
                Some(Err(anyhow::anyhow!("작업이 중단되었습니다")))
            }
        }
    }
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(16.0)
        .inner_margin(20.0)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Big page title with a one-line description underneath.
pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.add_space(4.0);
    let (icon, text) = crate::art::split_icon(title);
    ui.horizontal(|ui| {
        if let Some(icon) = icon {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
            let (a, b) = crate::art::badge_colors(text);
            crate::art::icon_badge(ui.painter(), rect, icon, a, b);
            ui.add_space(4.0);
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(text).size(28.0).strong().color(Color32::WHITE));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(15.0).color(MUTED));
            }
        });
    });
    ui.add_space(12.0);
}

/// Small rounded status pill.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(20.0)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(13.0).color(color));
        });
}

/// Main call-to-action button.
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(17.0).strong().color(Color32::WHITE)).fill(ACCENT).corner_radius(12.0).min_size(Vec2::new(200.0, 46.0)))
}

pub fn spinner_label(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(RichText::new(text).color(MUTED));
    });
}

pub fn error_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(format!("⚠ {text}")).color(BAD));
}

// ---------------------------------------------------------------- microphone capture

#[derive(Default)]
pub struct Capture {
    rx: Pending<String>,
    pub error: Option<String>,
    recording: bool,
    /// Voiced duration of the last recording, for speaking-speed (WPM) measurement.
    pub last_secs: f32,
    /// Stop automatically after ~0.9 s of silence following speech (speed drills).
    pub auto_stop: bool,
    heard_voice: bool,
    silent_since: Option<std::time::Instant>,
    /// When the last recording was stopped (for response-time measurement).
    pub stopped_at: Option<std::time::Instant>,
    /// Recognize Korean instead of the target language (translator).
    pub korean: bool,
}

impl Capture {
    /// A capture that recognizes Korean (translator).
    pub fn korean() -> Self {
        Self { korean: true, ..Default::default() }
    }

    /// A capture that stops by itself when the speaker goes quiet.
    pub fn auto_stopping() -> Self {
        Self { auto_stop: true, ..Default::default() }
    }

    /// Starts recording immediately (used to auto-start the mic in speed drills).
    pub fn start_now(&mut self, voice: &mut Voice) {
        if !self.recording && !self.rx.running() {
            self.start(voice);
        }
    }

    fn start(&mut self, voice: &mut Voice) {
        self.error = None;
        self.heard_voice = false;
        self.silent_since = None;
        voice.speaker.stop();
        voice.player.stop();
        if voice.recorder.is_recording() {
            // Another capture is still recording; stop it first.
            voice.recorder.stop();
        }
        voice.recorder.start();
        self.recording = true;
    }

    fn finish(&mut self, voice: &mut Voice) {
        self.recording = false;
        self.stopped_at = Some(std::time::Instant::now());
        let problem = voice.recorder.problem();
        let audio = voice.recorder.stop();
        if let Some(p) = problem {
            // Nothing real was recorded; don't let the recognizer guess at silence.
            self.error = Some(p.message());
            return;
        }
        self.last_secs = crate::audio::voiced_seconds(&audio, crate::audio::STT_RATE);
        voice.last_recording = audio.clone();
        let stt = if self.korean { &voice.stt_ko } else { &voice.stt };
        self.rx.from_receiver(stt.transcribe(audio));
    }

    /// Big round mic button. Returns the transcript once recognition finishes.
    pub fn ui(&mut self, ui: &mut Ui, voice: &mut Voice, max_secs: f32) -> Option<String> {
        // The mic failed to open, or someone else took the recorder.
        if self.recording {
            match voice.recorder.state() {
                crate::audio::RecState::Failed(e) => {
                    self.recording = false;
                    self.error = Some(e);
                    voice.recorder.reset();
                }
                crate::audio::RecState::Idle => self.recording = false,
                _ => {}
            }
        }
        let mut result = None;
        if let Some(r) = self.rx.poll() {
            match r {
                Ok(t) if !t.trim().is_empty() => result = Some(t),
                Ok(_) => self.error = Some("말한 내용을 알아듣지 못했어요. 다시 해볼까요?".into()),
                Err(e) => self.error = Some(format!("{e:#}")),
            }
        }
        let size = 84.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 16.0), Sense::click());
        let a11y = if self.recording { "⏹ 녹음 끝내기" } else { "🎤 말하기" };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, a11y));
        let c = rect.center();
        let p = ui.painter();
        let transcribing = self.rx.running();
        if self.recording {
            let lvl = voice.recorder.level().min(1.0);
            p.circle_filled(c, size / 2.0 + 4.0 + lvl * 14.0, BAD.gamma_multiply(0.25));
            p.circle_filled(c, size / 2.0, BAD);
            p.rect_filled(egui::Rect::from_center_size(c, Vec2::splat(24.0)), 4.0, Color32::WHITE);
            ui.ctx().request_repaint();
        } else {
            let fill = if transcribing { MUTED } else if resp.hovered() { ACCENT.gamma_multiply(1.2) } else { ACCENT };
            p.circle_filled(c, size / 2.0, fill);
            draw_mic_icon(p, c);
        }
        if resp.clicked() && !transcribing {
            if self.recording {
                self.finish(voice);
            } else {
                self.start(voice);
            }
        }
        if self.recording && voice.recorder.elapsed() > max_secs {
            self.finish(voice);
        }
        // A blocked microphone records pure zeros: stop right away and say why.
        if self.recording && voice.recorder.problem().is_some() && voice.recorder.elapsed() > 1.5 {
            self.finish(voice);
        }
        if self.recording && self.auto_stop {
            let lvl = voice.recorder.level();
            if lvl > 0.08 {
                self.heard_voice = true;
                self.silent_since = None;
            } else if self.heard_voice && lvl < 0.03 {
                let since = *self.silent_since.get_or_insert_with(std::time::Instant::now);
                if since.elapsed().as_secs_f32() > 0.9 {
                    self.finish(voice);
                }
            }
        }
        let status = if self.recording && voice.recorder.state() == crate::audio::RecState::Starting {
            ui.ctx().request_repaint();
            "마이크 준비 중…".to_string()
        } else if self.recording {
            format!("듣고 있어요… {:.0}초 (다 말하면 다시 누르세요)", voice.recorder.elapsed())
        } else if transcribing {
            ui.ctx().request_repaint();
            "인식 중…".to_string()
        } else {
            "눌러서 말하기".to_string()
        };
        ui.label(RichText::new(status).color(MUTED));
        if let Some(e) = &self.error {
            error_label(ui, e);
        }
        result
    }
}

fn draw_mic_icon(p: &egui::Painter, c: egui::Pos2) {
    let w = Color32::WHITE;
    p.rect_filled(egui::Rect::from_center_size(c - Vec2::new(0.0, 6.0), Vec2::new(16.0, 26.0)), 8.0, w);
    let s = Stroke::new(3.0_f32, w);
    let pts: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 12.0);
            c + Vec2::new(-14.0 * a.cos(), 2.0 + 12.0 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, s));
    p.line_segment([c + Vec2::new(0.0, 14.0), c + Vec2::new(0.0, 22.0)], s);
}

// ---------------------------------------------------------------- the speaking drill

pub fn speaker_button(ui: &mut Ui, voice: &mut Voice, text: &str) {
    if ui.button("🔊").on_hover_text("듣기").clicked() {
        voice.say(text);
    }
}

pub fn word_feedback(ui: &mut Ui, r: &ScoreResult, size: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for m in &r.marks {
            ui.label(RichText::new(&m.word).size(size).color(if m.ok { GOOD } else { BAD }));
        }
    });
}

pub struct Attempt {
    pub score: f32,
    pub transcript: String,
    /// Target words the recognizer didn't hear (likely mispronounced). Empty for recall tries.
    pub missed: Vec<String>,
}

pub const GLOSS: Color32 = Color32::from_rgb(255, 214, 120);

/// 직독직해 view: English chunks separated by " / " with the Korean meaning under each.
/// With `marks`, each chunk is coloured green/red by whether its words were recognized.
/// True for a word that ends a sentence ("day.", "you?", "stop!\"").
fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', '”', '’', ')']).ends_with(['.', '!', '?'])
}

/// Word-by-word feedback with each sentence on its own line.
pub fn word_feedback_lines(ui: &mut Ui, r: &ScoreResult, size: f32) {
    let mut start = 0;
    for (i, m) in r.marks.iter().enumerate() {
        if ends_sentence(&m.word) || i + 1 == r.marks.len() {
            word_feedback(ui, &ScoreResult { score: r.score, marks: r.marks[start..=i].to_vec() }, size);
            start = i + 1;
        }
    }
}

/// 직독직해 with each sentence on its own line; `marks` (one per word) color the chunks.
pub fn chunk_view_lines(ui: &mut Ui, chunks: &[Chunk], marks: Option<&[WordMark]>, size: f32) {
    let (mut start, mut word) = (0, 0);
    for (i, c) in chunks.iter().enumerate() {
        let last_word = c.en.split_whitespace().last().unwrap_or("");
        if ends_sentence(last_word) || i + 1 == chunks.len() {
            let words: usize = chunks[start..=i].iter().map(|c| c.en.split_whitespace().count()).sum();
            let m = marks.and_then(|m| m.get(word..word + words));
            chunk_view(ui, &chunks[start..=i], m, size);
            word += words;
            start = i + 1;
        }
    }
}

/// Plain text with each sentence on its own line.
pub fn sentence_lines(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    for s in crate::neural_tts::sentences(text) {
        ui.label(RichText::new(s).size(size).color(color));
    }
}

pub fn chunk_view(ui: &mut Ui, chunks: &[Chunk], marks: Option<&[WordMark]>, size: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 10.0);
        let mut k = 0;
        for (i, c) in chunks.iter().enumerate() {
            if i > 0 {
                ui.vertical(|ui| {
                    ui.label(RichText::new("/").size(size).color(MUTED));
                    ui.label(RichText::new("/").size(size * 0.62).color(MUTED));
                });
            }
            let n = c.en.split_whitespace().count();
            let color = match marks {
                Some(m) if k + n <= m.len() => {
                    if m[k..k + n].iter().all(|w| w.ok) { GOOD } else { BAD }
                }
                _ => Color32::WHITE,
            };
            k += n;
            ui.vertical(|ui| {
                ui.label(RichText::new(&c.en).size(size).strong().color(color));
                ui.label(RichText::new(&c.ko).size(size * 0.62).color(GLOSS));
            });
        }
    });
}

pub enum DrillEvent {
    None,
    /// A scored attempt.
    Attempt(Attempt),
    /// The required number of passing repetitions has been reached.
    Completed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_end_detection() {
        assert!(ends_sentence("day.") && ends_sentence("you?\"") && ends_sentence("stop!”"));
        assert!(!ends_sentence("Mr") && !ends_sentence("well,"));
    }

    #[test]
    fn hints_reveal_letters_gradually() {
        let mut d = Drill::new("schedule", "일정", "", 1, true, true);
        assert_eq!(d.hidden_text(), "________");
        d.hint = 1;
        assert_eq!(d.hidden_text(), "s_______");
        d.hint = 3;
        assert_eq!(d.hidden_text(), "sch_____");
        d.hint = 50;
        assert_eq!(d.hidden_text(), "schedul_", "the last letter is never given away");
        let mut s = Drill::new("Nice to meet you.", "만나서 반가워요.", "", 1, true, false);
        s.hint = 1;
        assert_eq!(s.hidden_text(), "N___ t_ m___ y__.");
    }
}

#[derive(Default)]
pub struct Drill {
    pub target: String,
    pub ko: String,
    pub tip: String,
    pub reps_needed: u32,
    pub reps_passed: u32,
    /// Production practice: show the Korean, the learner says the English from memory.
    pub recall: bool,
    /// Single word / short phrase (recall is checked by "did they say it").
    pub is_word: bool,
    pub revealed: bool,
    pub hint: usize,
    pub first_score: Option<f32>,
    pub last: Option<(String, ScoreResult)>,
    pub last_wpm: f32,
    /// 직독직해 for the target (empty = not available yet).
    pub chunks: Vec<Chunk>,
    pub show_gloss: bool,
    gloss_job: Pending<Vec<Chunk>>,
    /// A gloss fetched by this drill, for the app to cache.
    pub fresh_gloss: Option<(String, Vec<Chunk>)>,
    pub capture: Capture,
    coach: Pending<CoachTip>,
    pub coach_tip: Option<Result<CoachTip, String>>,
    autoplayed: bool,
    done: bool,
}

impl Drill {
    pub fn new(target: &str, ko: &str, tip: &str, reps: u32, recall: bool, is_word: bool) -> Self {
        Self {
            target: target.to_string(),
            ko: ko.to_string(),
            tip: tip.to_string(),
            reps_needed: reps.max(1),
            recall: recall && !ko.is_empty(),
            is_word,
            revealed: !(recall && !ko.is_empty()),
            ..Default::default()
        }
    }

    pub fn with_chunks(mut self, chunks: Vec<Chunk>) -> Self {
        self.chunks = chunks;
        self.show_gloss = true;
        self
    }

    /// Don't speak the target with TTS when the drill appears (e.g. native audio is playing).
    pub fn without_autoplay(mut self) -> Self {
        self.autoplayed = true;
        self
    }

    /// The answer with letters hidden; each hint reveals one more leading letter of every word
    /// (never the whole of a word, so one hint can't give away a single-word answer).
    fn hidden_text(&self) -> String {
        self.target
            .split_whitespace()
            .map(|w| {
                let chars: Vec<char> = w.chars().collect();
                let letters = chars.iter().filter(|c| c.is_alphanumeric()).count();
                let keep = self.hint.min(letters.saturating_sub(1));
                let mut seen = 0;
                chars
                    .iter()
                    .map(|c| {
                        if !c.is_alphanumeric() {
                            return *c;
                        }
                        seen += 1;
                        if seen <= keep { *c } else { '_' }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn ui(&mut self, ui: &mut Ui, voice: &mut Voice, pass: f32, claude: Option<&Claude>) -> DrillEvent {
        let mut event = DrillEvent::None;
        if !self.autoplayed && self.revealed {
            self.autoplayed = true;
            voice.say(&self.target);
        }
        if let Some(r) = self.coach.poll() {
            self.coach_tip = Some(r.map_err(|e| format!("{e:#}")));
        }
        if let Some(Ok(chunks)) = self.gloss_job.poll() {
            if crate::storage::chunks_match(&self.target, &chunks) {
                self.chunks = chunks.clone();
                self.show_gloss = true;
                self.fresh_gloss = Some((self.target.clone(), chunks));
            }
        }

        ui.vertical_centered(|ui| {
            if self.recall && !self.revealed {
                ui.label(RichText::new("영어로 말해 보세요").color(MUTED));
                ui.label(RichText::new(&self.ko).size(26.0).strong());
                ui.add_space(6.0);
                ui.label(RichText::new(self.hidden_text()).size(22.0).monospace().color(MUTED));
                if !self.tip.is_empty() && self.hint > 0 {
                    ui.label(RichText::new(&self.tip).color(MUTED));
                }
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 220.0).max(0.0) / 2.0);
                    if ui.button("💡 힌트").clicked() {
                        self.hint += 1;
                    }
                    if ui.button("정답 보기").clicked() {
                        self.revealed = true;
                        self.first_score.get_or_insert(0.0);
                        voice.say(&self.target);
                    }
                });
            } else {
                let marks = self.last.as_ref().map(|(_, r)| r.marks.as_slice());
                if self.show_gloss && !self.chunks.is_empty() {
                    chunk_view(ui, &self.chunks, marks, 26.0);
                } else {
                    match &self.last {
                        Some((_, r)) => word_feedback(ui, r, 26.0),
                        None => {
                            ui.label(RichText::new(&self.target).size(26.0).strong());
                        }
                    }
                }
                if !self.ko.is_empty() {
                    ui.label(RichText::new(&self.ko).color(MUTED));
                }
                if !self.tip.is_empty() {
                    ui.label(RichText::new(format!("💡 {}", self.tip)).color(WARN));
                }
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                    if ui.button("🔊 듣기").clicked() {
                        voice.say(&self.target);
                    }
                    if ui.button("🐢 천천히").clicked() {
                        voice.say_slow(&self.target);
                    }
                    if ui.button("🐇 빠르게").clicked() {
                        voice.say_fast(&self.target);
                    }
                    if !voice.last_recording.is_empty() && ui.button("🎧 내 목소리").clicked() {
                        voice.play_last_recording();
                    }
                    if !self.is_word && self.target.split_whitespace().count() > 1 {
                        if !self.chunks.is_empty() {
                            ui.toggle_value(&mut self.show_gloss, "📖 직독직해");
                        } else if self.gloss_job.running() {
                            ui.spinner();
                        } else if let Some(c) = claude {
                            if ui.button("📖 직독직해").clicked() {
                                let (c, t) = (c.clone(), self.target.clone());
                                self.gloss_job.spawn(move || c.chunk_gloss(&t));
                            }
                        }
                    }
                });
            }
            ui.add_space(8.0);

            if let Some(transcript) = self.capture.ui(ui, voice, 30.0) {
                let recall_try = self.recall && !self.revealed;
                let result = if recall_try && self.is_word {
                    let hit = scoring::contains_phrase(&transcript, &self.target);
                    let mut r = scoring::score(&self.target, &transcript);
                    if hit {
                        r.score = 100.0;
                        r.marks.iter_mut().for_each(|m| m.ok = true);
                    }
                    r
                } else {
                    scoring::score(&self.target, &transcript)
                };
                let s = result.score;
                self.first_score.get_or_insert(s);
                self.revealed = true;
                self.autoplayed = true;
                if s >= pass {
                    self.reps_passed += 1;
                }
                self.coach_tip = None;
                let missed = if recall_try { Vec::new() } else { result.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect() };
                let words = result.marks.iter().filter(|m| m.ok).count();
                self.last_wpm = crate::audio::wpm(words, self.capture.last_secs);
                self.last = Some((transcript.clone(), result));
                event = DrillEvent::Attempt(Attempt { score: s, transcript, missed });
                if self.reps_passed >= self.reps_needed && !self.done {
                    self.done = true;
                }
            }

            if let Some((heard, r)) = &self.last {
                let color = if r.score >= pass { GOOD } else if r.score >= pass - 20.0 { WARN } else { BAD };
                ui.label(RichText::new(format!("{:.0}점", r.score)).size(30.0).strong().color(color));
                ui.label(RichText::new(format!("들린 말: \"{heard}\"")).color(MUTED));
                if self.last_wpm > 0.0 && r.marks.len() >= 4 {
                    ui.label(RichText::new(format!("⚡ 말하기 속도 {:.0} WPM (원어민 대화 약 150)", self.last_wpm)).color(MUTED).small());
                }
                if r.score < pass {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                        if let Some(c) = claude {
                            if self.coach.running() {
                                ui.spinner();
                            } else if ui.button("💡 AI 발음 코칭").clicked() {
                                let (c, t, h) = (c.clone(), self.target.clone(), heard.clone());
                                let missed: Vec<String> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect();
                                self.coach.spawn(move || c.pronunciation_coach(&t, &h, &missed));
                            }
                        }
                    });
                }
            }
            match &self.coach_tip {
                Some(Ok(t)) => {
                    ui.label(RichText::new(format!("💡 {}", t.tip_ko)).color(Color32::LIGHT_BLUE));
                    if !t.focus_words.is_empty() {
                        ui.label(RichText::new(format!("집중 연습: {}", t.focus_words.join(", "))).color(WARN));
                    }
                }
                Some(Err(e)) => error_label(ui, e),
                None => {}
            }

            ui.add_space(6.0);
            reps_dots(ui, self.reps_passed, self.reps_needed);
            if self.done {
                ui.add_space(6.0);
                if ui.add(egui::Button::new(RichText::new("다음 ▶").size(18.0)).fill(GOOD).min_size(Vec2::new(160.0, 40.0))).clicked() {
                    event = DrillEvent::Completed;
                }
            } else if self.last.is_some() {
                ui.label(RichText::new(format!("{}점 이상으로 {}번 말하면 통과", pass as i32, self.reps_needed)).color(MUTED));
            }
        });
        event
    }
}

pub fn reps_dots(ui: &mut Ui, passed: u32, needed: u32) {
    let r = 7.0;
    let w = needed as f32 * (r * 2.0 + 8.0);
    ui.horizontal(|ui| {
        ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, r * 2.0 + 4.0), Sense::hover());
        for i in 0..needed {
            let c = rect.left_center() + Vec2::new(r + 4.0 + i as f32 * (r * 2.0 + 8.0), 0.0);
            if i < passed {
                ui.painter().circle_filled(c, r, GOOD);
            } else {
                ui.painter().circle_stroke(c, r, Stroke::new(2.0_f32, MUTED));
            }
        }
    });
}

/// Progress ring (daily goal), drawn like Speak's home screen.
pub fn ring(ui: &mut Ui, frac: f32, label: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let c = rect.center();
    let rad = size / 2.0 - 6.0;
    let p = ui.painter();
    p.circle_stroke(c, rad, Stroke::new(9.0_f32, Color32::from_rgb(40, 48, 70)));
    let n = 64;
    let pts: Vec<egui::Pos2> = (0..=((n as f32 * frac.clamp(0.0, 1.0)) as usize))
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * (i as f32 / n as f32);
            c + Vec2::new(a.cos(), a.sin()) * rad
        })
        .collect();
    if pts.len() > 1 {
        p.add(egui::Shape::line(pts, Stroke::new(9.0_f32, ACCENT)));
    }
    p.text(c, Align2::CENTER_CENTER, label, FontId::proportional(size * 0.2), Color32::WHITE);
}
