//! Read-along of a script, shared by 스피드 리딩 and 4/3/2: the whole script on screen sentence
//! by sentence (the current one stands out, passed ones turn green, a click jumps there), and one
//! sentence at a time — hear it, say it, see the score.

use crate::app::App;
use crate::scoring::ScoreResult;
use crate::widgets::*;
use egui::{Color32, RichText, Ui};

#[derive(Default)]
pub struct ReadAlong {
    pub idx: usize,
    pub capture: Capture,
    pub result: Option<(String, ScoreResult)>,
    pub passed: Vec<bool>,
}

impl ReadAlong {
    pub fn new(sentences: usize) -> Self {
        Self { passed: vec![false; sentences], ..Default::default() }
    }

    pub fn done(&self) -> usize {
        self.passed.iter().filter(|p| **p).count()
    }

    /// Moves to sentence `k` with a fresh microphone.
    pub fn go(&mut self, k: usize) {
        self.idx = k;
        self.result = None;
        self.capture = Capture::default();
    }
}

/// Length in Japanese characters (the unit of reading speed).
fn words(t: &str) -> usize {
    crate::ja::char_count(t)
}

impl App {
    /// The whole script, one clickable label per sentence. `current` highlights the sentence
    /// being read along. Returns the sentence clicked.
    pub fn script_view(&mut self, ui: &mut Ui, sentences: &[String], ra: &ReadAlong, current: bool, size: f32) -> Option<usize> {
        let mut jump = None;
        egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                for (k, sen) in sentences.iter().enumerate() {
                    let here = current && k == ra.idx;
                    let passed = ra.passed.get(k).copied().unwrap_or(false);
                    let color = if here { ACCENT } else if passed { GOOD } else { text_strong() };
                    let mut text = RichText::new(format!("{sen} ")).size(size).color(color);
                    if here {
                        text = text.strong().underline();
                    }
                    let r = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text("클릭하면 이 문장부터 따라 읽어요");
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        jump = Some(k);
                    }
                }
            });
        });
        jump
    }

    /// One sentence at a time. `kind` labels the attempts in the log. Returns true when the
    /// learner presses `next_label` after the last sentence.
    pub fn read_along_ui(&mut self, ui: &mut Ui, ra: &mut ReadAlong, sentences: &[String], size: f32, kind: &str, next_label: &str) -> bool {
        let n = sentences.len();
        if n == 0 {
            return false;
        }
        ra.passed.resize(n, false);
        let i = ra.idx.min(n - 1);
        let sentence = &sentences[i];
        let pass = self.store.settings.pass_score;
        let mut go: Option<usize> = None;
        let mut next = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("따라 읽기").size(18.0).strong().color(ACCENT));
                ui.label(RichText::new(format!("{}/{n} 문장 · 통과 {}", i + 1, ra.done())).color(muted()));
            });
            ui.horizontal_wrapped(|ui| {
                speaker_button(ui, &mut self.voice, sentence);
                ui.label(RichText::new(sentence).size(size).strong().color(text_strong()));
            });
            ui.label(RichText::new("🔊 듣고 → 🎤 눌러서 똑같이 따라 읽어요").color(muted()).small());
            ui.vertical_centered(|ui| {
                let limit = 5.0 + words(sentence) as f32 * 0.3;
                if let Some(t) = ra.capture.ui(ui, &mut self.voice, limit) {
                    let r = crate::scoring::score(sentence, &t);
                    self.store.log_attempt(kind, sentence, &t, r.score);
                    if r.score >= pass {
                        ra.passed[i] = true;
                    }
                    ra.result = Some((t, r));
                }
            });
            if let Some((t, r)) = &ra.result {
                let ok = r.score >= pass;
                ui.label(RichText::new(format!("{} {:.0}점 — \"{t}\"", if ok { "✅" } else { "🔁" }, r.score)).color(if ok { GOOD } else { warn() }));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("◀ 이전 문장").min_size(ACTION_BUTTON)).clicked() {
                    go = Some(i - 1);
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new("다음 문장 ▶").min_size(ACTION_BUTTON)).clicked() {
                    go = Some(i + 1);
                }
                if i + 1 == n && ui.add(egui::Button::new(RichText::new(next_label).color(Color32::WHITE)).fill(GOOD).min_size(ACTION_BUTTON)).clicked() {
                    next = true;
                }
            });
        });
        if let Some(k) = go {
            ra.go(k);
            self.voice.say(&sentences[k]);
        }
        next
    }
}
