//! ✏ Sentence missions: say an original sentence with a given word. Used by homework ("나만의
//! 문장") and by 오늘의 문장 만들기 in 새 단어 (the day's words on the week's theme and grammar).
//! The screen looks nothing like read-aloud so it isn't mistaken for "just record something":
//! the word sits in a sentence template, and what was said is checked for the word on the device
//! before Claude judges it (on the device alone when Claude can't be used).

use crate::app::App;
use crate::claude::WordUseCheck;
use crate::widgets::*;
use egui::{Color32, RichText, Ui, Vec2};

#[derive(Default)]
pub struct Mission {
    pub capture: Capture,
    /// What was said last, and whether it used the word in a real sentence.
    pub said: Option<(String, bool)>,
    pub show_hint: bool,
    pub check: Pending<WordUseCheck>,
    pub error: Option<String>,
    pub result: Option<WordUseCheck>,
}

pub struct MissionSpec {
    pub word: String,
    pub meaning_ko: String,
    /// The week's curriculum (shown line, theme, grammar) for 오늘의 문장 만들기; None for homework.
    pub week: Option<(String, String, String)>,
    /// Sentence starters offered as hints ("…" = the learner's own words).
    pub starters: Vec<String>,
    /// Passed before (e.g. a homework task reopened): offer 다음 straight away.
    pub already_passed: bool,
}

pub enum MissionEvent {
    /// A judgement came back (from Claude, or the device when Claude can't be used).
    Checked(WordUseCheck),
    /// The learner moves on after passing.
    Next,
}

impl App {
    pub fn mission_ui(&mut self, ui: &mut Ui, m: &mut Mission, spec: &MissionSpec) -> Option<MissionEvent> {
        let word = spec.word.clone();
        let mut event = None;
        if let Some(r) = m.check.poll() {
            match r {
                Ok(res) => {
                    m.result = Some(res.clone());
                    event = Some(MissionEvent::Checked(res));
                }
                Err(e) => m.error = Some(format!("{e:#}")),
            }
        }
        // Mission banner.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(760.0), 78.0), egui::Sense::hover());
        crate::art::gradient_rect(ui.painter(), rect, 16.0, Color32::from_rgb(236, 120, 40), crate::art::PINK);
        let title = if spec.week.is_some() { "✏ 오늘의 문장 만들기" } else { "✏ 나만의 문장 만들기 미션" };
        ui.painter().text(rect.left_top() + Vec2::new(20.0, 14.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(22.0), Color32::WHITE);
        ui.painter().text(
            rect.left_top() + Vec2::new(20.0, 46.0),
            egui::Align2::LEFT_TOP,
            "따라 읽기가 아니에요! 아래 단어를 꼭 넣어 내 문장을 직접 만들어 말하세요",
            egui::FontId::proportional(15.0),
            Color32::from_rgb(255, 240, 230),
        );
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                if let Some((line, _, _)) = &spec.week {
                    ui.label(RichText::new(line).color(ACCENT).strong());
                    ui.add_space(4.0);
                }
                // The sentence template: ____ ( word ) ____ .
                ui.horizontal_wrapped(|ui| {
                    let blank = RichText::new("＿＿＿＿").size(28.0).color(muted());
                    let w = ui.fonts(|f| f.layout_no_wrap(word.clone(), egui::FontId::proportional(32.0), Color32::WHITE).size().x) + 230.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    ui.label(blank.clone());
                    egui::Frame::new().fill(warn()).corner_radius(10.0).inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| {
                        ui.label(RichText::new(format!("( {word} )")).size(32.0).strong().color(Color32::WHITE));
                    });
                    ui.label(blank);
                    ui.label(RichText::new(".").size(28.0).color(muted()));
                });
                crate::grammar::ko_label(ui, &format!("뜻: {}", spec.meaning_ko), 17.0, muted());
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(((ui.available_width() - 520.0) / 2.0).max(0.0));
                    for step in ["① 뜻 확인", "② 내 이야기로 문장 떠올리기", "③ 🎤 눌러서 말하기"] {
                        chip(ui, step, crate::art::VIOLET);
                    }
                });
                ui.add_space(4.0);
                ui.checkbox(&mut m.show_hint, "💡 막히면 힌트 보기");
                if m.show_hint {
                    egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                        ui.label(RichText::new(format!("이렇게 시작해 보세요 (… 자리에 내 이야기와 '{word}'를)")).color(ACCENT));
                        for starter in &spec.starters {
                            ui.label(RichText::new(starter).size(16.0));
                        }
                        if let Some((b, _)) = crate::dictionary::lookup(&word).filter(|(b, _)| !b.example.is_empty()) {
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("참고 예문: {}", b.example)).color(muted()));
                            ui.label(RichText::new("그대로 따라 하지 말고, 바꿔서 내 문장으로 만들어요!").color(warn()).small());
                        }
                    });
                }
                ui.add_space(8.0);
                if m.check.running() {
                    spinner_label(ui, "Claude가 내 문장을 확인 중…");
                } else if let Some(said) = m.capture.ui(ui, &mut self.voice, 30.0) {
                    self.store.log_attempt(if spec.week.is_some() { "sentence-mission" } else { "homework" }, &word, &said, 0.0);
                    let used = crate::dictionary::sentence_uses(&said, &word);
                    let long_enough = said.split_whitespace().count() >= 4;
                    m.said = Some((said.clone(), used && long_enough));
                    m.error = None;
                    m.result = None;
                    // Only a real sentence with the word is judged (saves credit too).
                    if used && long_enough {
                        match self.claude().filter(|_| !self.talk_offline()) {
                            Some(c) => {
                                let (w, mk, lvl) = (word.clone(), spec.meaning_ko.clone(), self.store.profile.level);
                                match spec.week.clone() {
                                    Some((_, theme, grammar)) => m.check.spawn(move || c.check_sentence_mission(&w, &mk, &theme, &grammar, &said, lvl)),
                                    None => m.check.spawn(move || c.check_word_use(&w, &mk, &said)),
                                }
                            }
                            None => {
                                // Without Claude: the device's check is the judgement.
                                let better = crate::dictionary::lookup(&word).map(|(b, _)| b.example.clone()).unwrap_or_default();
                                let res = WordUseCheck { ok: true, feedback_ko: "👍 단어를 넣어 문장을 잘 만들었어요 (기기에서 확인)".into(), better };
                                m.result = Some(res.clone());
                                event = Some(MissionEvent::Checked(res));
                            }
                        }
                    }
                }
                ui.label(RichText::new(format!("🎤 '{word}'을(를) 넣어서 말해요")).color(muted()).small());
                // What was said, with the word highlighted (or what's missing).
                if let Some((said, ok)) = m.said.clone() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("내 문장:").color(muted()));
                        for t in said.split_whitespace() {
                            if word.split_whitespace().any(|w| crate::dictionary::same_word(t, w)) {
                                egui::Frame::new().fill(GOOD.gamma_multiply(0.3)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(4, 0)).show(ui, |ui| {
                                    ui.label(RichText::new(t).size(18.0).strong().color(GOOD));
                                });
                            } else {
                                ui.label(RichText::new(t).size(18.0));
                            }
                        }
                    });
                    if !crate::dictionary::sentence_uses(&said, &word) {
                        error_label(ui, &format!("✖ '{word}'이(가) 빠졌어요 — 녹음만 하지 말고, '{word}'을(를) 넣은 내 문장을 만들어 다시 말해 보세요."));
                    } else if !ok {
                        error_label(ui, "✖ 단어만 말했어요 — 4단어 이상의 문장으로 말해 보세요. (예: 주어 + 동사 + 단어 + 이야기)");
                    } else {
                        ui.label(RichText::new(format!("✓ '{word}'을(를) 넣어 문장을 만들었어요")).color(GOOD).strong());
                    }
                }
                if let Some(e) = &m.error {
                    error_label(ui, e);
                }
                if let Some(r) = &m.result {
                    ui.label(RichText::new(&r.feedback_ko).color(if r.ok { GOOD } else { warn() }));
                    if !r.better.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("👉 {}", r.better)).size(16.0).color(gloss()));
                            if ui.button("🔊").clicked() {
                                self.voice.say(&r.better);
                            }
                        });
                    }
                }
                let passed = spec.already_passed || m.result.as_ref().is_some_and(|r| r.ok);
                if passed && ui.add(egui::Button::new(RichText::new("다음 ▶").color(Color32::WHITE)).fill(GOOD).min_size(ACTION_BUTTON)).clicked() {
                    event = Some(MissionEvent::Next);
                }
            });
        });
        event
    }
}
