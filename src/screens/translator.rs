//! 🌐 Translator: say (or type) something in Korean and get it in Japanese at three levels —
//! easy, everyday native, and advanced — shown as text and spoken, ready to repeat aloud.

use crate::app::App;
use crate::claude::Translation;
use crate::models::*;
use crate::stt::{self, SttStatus};
use crate::widgets::*;
use egui::{RichText, Ui};

/// Multilingual whisper models that understand Korean: (id, label).
pub const KOREAN_MODELS: &[(&str, &str)] = &[("small", "466MB · 한국어 정확도 권장"), ("base", "142MB · 빠름")];

#[derive(Default)]
pub struct TranslatorState {
    pub korean: String,
    capture: Option<Capture>,
    job: Pending<Translation>,
    pub result: Option<Result<Translation, String>>,
    drill: Option<Drill>,
    /// Recent translations (Korean, versions) for this session.
    history: Vec<(String, Translation)>,
}

impl App {
    pub fn translator_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "🌐 자동 번역기", "한국어로 말하거나 쓰면, 쉬운 · 중간 · 고급 일본어 표현으로 바꿔서 보여 주고 읽어 줍니다.");
        let claude = self.claude();
        if claude.is_none() {
            card(ui, |ui| {
                ui.label(RichText::new("번역기는 Claude API 키가 필요합니다. 설정에서 키를 입력하세요.").color(warn()));
            });
            return;
        }
        // Results
        if let Some(r) = self.translator.job.poll() {
            if let Ok(t) = &r {
                self.translator.history.insert(0, (self.translator.korean.clone(), t.clone()));
                self.translator.history.truncate(10);
                // Speak the version closest to the learner's level.
                let pick = if self.store.profile.level < 4.0 { 0 } else if self.store.profile.level < 7.0 { 1 } else { 2 };
                if let Some(v) = t.versions.get(pick.min(t.versions.len().saturating_sub(1))) {
                    self.voice.say(&v.japanese);
                }
                self.store.log_attempt("translator", &self.translator.korean, &t.versions.iter().map(|v| v.japanese.clone()).collect::<Vec<_>>().join(" | "), 0.0);
            }
            self.translator.result = Some(r.map_err(|e| format!("{e:#}")));
            self.translator.drill = None;
        }

        let mut ask: Option<String> = None;
        card(ui, |ui| {
            ui.label(RichText::new("🎤 한국어로 말하기").strong());
            let ko_ready = self.voice.stt_ko.is_ready();
            if ko_ready {
                let cap = self.translator.capture.get_or_insert_with(Capture::korean);
                ui.horizontal_wrapped(|ui| {
                    if let Some(text) = cap.ui(ui, &mut self.voice, 20.0) {
                        self.translator.korean = text.clone();
                        ask = Some(text);
                    }
                    ui.label(RichText::new("예: \"오늘 회의 좀 미룰 수 있을까요?\"").color(muted()));
                });
            } else {
                self.korean_stt_setup_ui(ui);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("⌨ 또는 직접 입력").strong());
            ui.horizontal_wrapped(|ui| {
                let resp = ui.add(egui::TextEdit::singleline(&mut self.translator.korean).desired_width(ui.available_width() - 110.0).font(egui::FontId::proportional(18.0)).hint_text("하고 싶은 말을 한국어로"));
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let can = !self.translator.korean.trim().is_empty() && !self.translator.job.running();
                if (ui.add_enabled(can, egui::Button::new(RichText::new("번역 ›").strong()).fill(ACCENT)).clicked() || enter) && can {
                    ask = Some(self.translator.korean.clone());
                }
            });
        });
        if let (Some(k), Some(c)) = (ask, claude.clone()) {
            self.translator.result = None;
            self.translator.job.spawn(move || c.translate_levels(&k));
        }
        if self.translator.job.running() {
            spinner_label(ui, "일본어 표현을 만들고 있어요…");
        }

        // Three versions
        let mut practice: Option<String> = None;
        let mut save: Option<(String, String)> = None;
        if let Some(r) = &self.translator.result {
            match r {
                Ok(t) => {
                    if !t.meaning_ko.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("이해한 뜻:").color(muted()));
                            crate::grammar::ko_label(ui, &t.meaning_ko, 16.0, muted());
                        });
                    }
                    let colors = [GOOD, ACCENT, crate::art::VIOLET];
                    for (i, v) in t.versions.iter().enumerate() {
                        card(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                chip(ui, &v.level_ko, colors[i % 3]);
                                ui.label(RichText::new(&v.japanese).size(21.0).strong());
                            });
                            if !v.note_ko.is_empty() {
                                ui.label(RichText::new(format!("💡 {}", v.note_ko)).color(gloss()));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("🔊 듣기").clicked() {
                                    self.voice.say(&v.japanese);
                                }
                                if ui.button("🐢 천천히").clicked() {
                                    self.voice.say_slow(&v.japanese);
                                }
                                if ui.button("🎤 따라 말하기").clicked() {
                                    practice = Some(v.japanese.clone());
                                }
                                if ui.button("⭐ 저장 (복습)").clicked() {
                                    save = Some((v.japanese.clone(), self.translator.korean.clone()));
                                }
                            });
                        });
                    }
                }
                Err(e) => error_label(ui, e),
            }
        }
        if let Some(p) = practice {
            let ko = self.translator.korean.clone();
            self.translator.drill = Some(Drill::new(&p, &ko, "", self.store.settings.reps_new, false, false).without_autoplay());
        }
        if let Some((en, ko)) = save {
            crate::srs::add_item(&mut self.store.items, StudyItem { kind: ItemKind::Sentence, text: en, meaning_ko: ko, category: "translator".into(), ..Default::default() });
            self.store.save_items();
            self.toast("⭐ 복습 목록에 저장했어요");
        }
        let pass = self.pass_score();
        let mut ev = DrillEvent::None;
        if let Some(d) = self.translator.drill.as_mut() {
            ev = card(ui, |ui| d.ui(ui, &mut self.voice, pass, claude.as_ref()));
        }
        match ev {
            DrillEvent::Attempt(a) => {
                let t = self.translator.drill.as_ref().map(|d| d.target.clone()).unwrap_or_default();
                self.record_attempt("translator", &t, &a);
            }
            DrillEvent::Completed => {
                self.translator.drill = None;
                self.toast("👏 완벽해요! 이제 그 표현은 내 것이에요.");
            }
            DrillEvent::None => {}
        }

        if self.translator.history.len() > 1 {
            ui.add_space(8.0);
            ui.label(RichText::new("최근 번역").strong());
            let mut reopen: Option<usize> = None;
            for (i, (k, t)) in self.translator.history.iter().enumerate().skip(1) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(k).color(muted()));
                    if let Some(v) = t.versions.get(1).or(t.versions.first()) {
                        if ui.link(&v.japanese).clicked() {
                            reopen = Some(i);
                        }
                    }
                });
            }
            if let Some(i) = reopen {
                let (k, t) = self.translator.history[i].clone();
                self.translator.korean = k;
                self.translator.result = Some(Ok(t));
            }
        }
    }

    /// Download / load the multilingual model used for Korean speech.
    fn korean_stt_setup_ui(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("한국어 음성 인식 모델이 필요해요 (한 번만 받으면 됩니다).").color(muted()));
        ui.horizontal_wrapped(|ui| {
            for (id, label) in KOREAN_MODELS {
                if ui.radio_value(&mut self.store.settings.korean_model, id.to_string(), format!("{id} ({label})")).changed() {
                    self.store.save_settings();
                }
            }
        });
        let path = stt::model_path(&self.store.models_dir(), &self.store.settings.korean_model);
        let s = &self.voice.stt_ko;
        match s.status() {
            SttStatus::Downloading => {
                let d = s.downloaded.load(std::sync::atomic::Ordering::Relaxed) as f32;
                let t = s.total.load(std::sync::atomic::Ordering::Relaxed).max(1) as f32;
                ui.add(egui::ProgressBar::new(d / t).text(format!("{:.0} / {:.0} MB", d / 1e6, t / 1e6)));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            status => {
                if let SttStatus::Error(e) = status {
                    error_label(ui, &e);
                }
                if path.exists() {
                    if ui.button("이 모델 불러오기").clicked() {
                        s.load(path);
                    }
                } else if ui.add(egui::Button::new(RichText::new("⬇ 한국어 음성 인식 모델 받기").strong()).fill(ACCENT)).clicked() {
                    s.download(self.store.models_dir(), self.store.settings.korean_model.clone());
                }
            }
        }
    }
}
