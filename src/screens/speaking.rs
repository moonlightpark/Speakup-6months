//! Speaking: self-expression (answer a question about yourself → Claude feedback → read the
//! natural version aloud) and conversation (business role-play or free talk with Claude).

use crate::app::App;
use crate::claude::{RoleplayTurn, SpeechFeedback};
use crate::curriculum;
use crate::models::*;
use crate::widgets::*;
use chrono::Datelike;
use egui::{Color32, RichText, Ui};

#[derive(Default)]
pub struct SelfExprState {
    pub capture: Capture,
    pub answer: Option<String>,
    pub job: Pending<SpeechFeedback>,
    pub feedback: Option<SpeechFeedback>,
    pub error: Option<String>,
    pub show_model: bool,
    /// ✏ 영작 노트: the learner's draft before speaking.
    pub draft: String,
    /// Offline: the built-in question being practised (its English text).
    pub offline_pick: Option<String>,
    /// Offline: feedback worked out on the device (words, speed, useful phrases used).
    pub local: Option<LocalFeedback>,
    /// Offline: follow-up questions answered so far, the current answer, and which question
    /// was already read out.
    pub follow: usize,
    pub follow_capture: Capture,
    pub follow_answer: Option<String>,
    pub follow_said: Option<usize>,
}

/// Self-expression feedback without Claude.
#[derive(Clone, Debug, Default)]
pub struct LocalFeedback {
    pub words: usize,
    pub wpm: f32,
    pub used: Vec<String>,
}

pub struct ChatLine {
    pub user: bool,
    pub text: String,
    pub ko: String,
    pub correction: String,
    pub correction_ko: String,
}

#[derive(Default)]
pub struct RoleplayState {
    /// 0 = today's business role-play, 1 = free talk, 2 = scene collection, 3 = my own scene
    pub mode: usize,
    pub collection: usize,
    pub custom: (String, String, String),
    pub started: bool,
    pub lines: Vec<ChatLine>,
    pub capture: Capture,
    pub job: Pending<RoleplayTurn>,
    pub suggestions: Vec<Phrase>,
    pub finished: bool,
    pub summary: String,
    pub error: Option<String>,
    pub show_ko: bool,
    pub scenario: Option<BusinessScenario>,
    /// This conversation was already counted toward the scene's practice count.
    pub counted: bool,
    /// The conversation follows a built-in script (Claude not available).
    pub offline: bool,
    /// That script.
    pub script: Option<crate::offline_talk::TalkScenario>,
    /// 자유 회화: the chosen category (index in `freetalk::CATEGORIES`).
    pub free_cat: usize,
}

/// Splits a paragraph into sentences for read-aloud drills.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, '.' | '?' | '!') && cur.trim().split_whitespace().count() >= 3 {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        if cur.trim().split_whitespace().count() < 3 && !out.is_empty() {
            let last = out.pop().unwrap();
            out.push(format!("{last} {}", cur.trim()));
        } else {
            out.push(cur.trim().to_string());
        }
    }
    out
}

impl App {
    pub fn self_expression_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) || !self.lesson_ready_ui(ui) {
            return;
        }
        let se = self.lesson.as_ref().map(|l| l.self_expression.clone()).unwrap_or_default();
        let offline = self.talk_offline();
        let oq = if offline { self.offline_question(&se.question) } else { None };
        let se = oq.map(|q| q.to_self_expression()).unwrap_or(se);
        let claude = self.claude().filter(|_| !offline);
        if let Some(r) = self.selfx.job.poll() {
            match r {
                Ok(f) => {
                    let answer = self.selfx.answer.clone().unwrap_or_default();
                    self.record_score("self-expression", &se.question, &answer, f.overall as f32);
                    for c in &f.corrections {
                        self.store.add_correction("자기 표현", &c.original, &c.corrected, &c.explanation_ko);
                    }
                    self.selfx.feedback = Some(f);
                }
                Err(e) => self.selfx.error = Some(format!("{e:#}")),
            }
        }

        page_title(ui, "💬 자기 표현 · 영작해서 말하기", "질문에 대한 내 답을 영어로 만들어(영작) 30~60초 동안 말해요. 완벽하지 않아도 괜찮아요 — 말한 뒤 더 자연스러운 표현으로 다시 읽습니다.");
        ui.add_space(8.0);
        if offline {
            let mut other = false;
            self.offline_banner(ui, "레벨에 맞게 미리 준비한 질문과 이어지는 질문으로 연습해요.", |ui| other = ui.button("🔀 다른 질문").clicked());
            if other {
                let qs = crate::offline_talk::questions(self.store.profile.level);
                let i = qs.iter().position(|q| q.question == se.question).map(|i| i + 1).unwrap_or(0);
                if let Some(q) = qs.get(i % qs.len().max(1)) {
                    self.selfx = SelfExprState { offline_pick: Some(q.question.clone()), ..Default::default() };
                }
                return;
            }
            ui.add_space(6.0);
        }
        // The question, asked by the mascot in a speech bubble.
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(56.0), egui::Sense::hover());
                crate::art::mascot(ui.painter(), r.center(), 46.0, crate::art::Mood::Talking, ui.input(|i| i.time) as f32);
                egui::Frame::new().fill(ACCENT.gamma_multiply(0.14)).corner_radius(14.0).inner_margin(12.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Q.").size(22.0).strong().color(ACCENT));
                        ui.label(RichText::new(&se.question).size(22.0).strong());
                        speaker_button(ui, &mut self.voice, &se.question);
                    });
                    crate::grammar::ko_label(ui, &se.question_ko, 16.0, muted());
                });
            });
        });
        ui.add_space(8.0);
        // 📝 영작 설계도: answer → details → wrap-up, with the useful phrases as building blocks.
        let target = crate::offline_talk::target_words(self.store.profile.level);
        let mut insert: Option<String> = None;
        let focus = self.focus_words();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("📝 영작 설계도").size(18.0).strong());
                ui.label(RichText::new(format!("이 순서로 내 답을 영어로 만들어 보세요 · 목표 약 {target}단어 ({}) · 30~60초", cefr_of(self.store.profile.level))).color(muted()));
            });
            ui.add_space(6.0);
            let steps: [(&str, &str, &[&str]); 3] = [
                ("① 답하기", "질문에 한 문장으로 바로 답해요", &["I …", "Yes, I …", "My answer is …"]),
                ("② 이유·예시", "왜 그런지, 예를 들어 2문장", &["Because …", "For example, …", "Also, …"]),
                ("③ 마무리", "한 문장으로 정리해요", &["So, …", "That's why …", "Anyway, …"]),
            ];
            ui.columns(3, |cols| {
                for (i, (title, desc, frames)) in steps.iter().enumerate() {
                    let ui = &mut cols[i];
                    egui::Frame::new().fill(subtle_fill()).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(*title).strong().color([ACCENT, crate::art::VIOLET, GOOD][i]));
                        ui.label(RichText::new(*desc).small().color(muted()));
                        ui.add_space(4.0);
                        // The lesson's useful phrase for this step first, then general frames.
                        let phrase = se.useful_phrases.get(i);
                        if let Some(p) = phrase {
                            let r = ui.add(egui::Label::new(RichText::new(&p.text).strong()).sense(egui::Sense::click())).on_hover_text(format!("{} · 누르면 영작 노트에 넣어요", p.ko));
                            if r.clicked() {
                                insert = Some(p.text.clone());
                            }
                            crate::grammar::ko_label(ui, &p.ko, 13.0, muted());
                        }
                        ui.horizontal_wrapped(|ui| {
                            for f in frames.iter() {
                                if ui.add(egui::Button::new(RichText::new(*f).small()).small()).on_hover_text("누르면 영작 노트에 넣어요").clicked() {
                                    insert = Some(f.to_string());
                                }
                            }
                        });
                    });
                }
            });
            ui.add_space(8.0);
            // ✏ 영작 노트: write first (optional), then say it looking at it.
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("✏ 영작 노트 (선택)").strong());
                let n = self.selfx.draft.split_whitespace().count();
                ui.label(RichText::new(format!("{n} / {target}단어")).color(if n >= target { GOOD } else { muted() }));
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.selfx.draft)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::proportional(17.0))
                    .hint_text("말하기 전에 키워드나 문장을 영어로 적어 보세요. 말할 때 보면서 말해도 좋아요."),
            );
            ui.horizontal_wrapped(|ui| {
                if !self.selfx.draft.trim().is_empty() && ui.button("🔊 내 영작 들어보기").clicked() {
                    self.voice.say(&self.selfx.draft);
                }
                ui.checkbox(&mut self.selfx.show_model, "모범 답안 보기");
            });
            if self.selfx.show_model {
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &se.model_answer);
                    ui.label(RichText::new(&se.model_answer).italics());
                });
                // Sentences with today's words, to borrow into my own answer.
                let examples: Vec<LessonWord> = focus.iter().filter(|w| !w.example.is_empty()).take(3).cloned().collect();
                if !examples.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("💡 오늘 단어가 들어간 문장 {}개 · 누르면 영작 노트에 넣어요", examples.len())).color(warn()).strong());
                    for w in &examples {
                        ui.horizontal_wrapped(|ui| {
                            speaker_button(ui, &mut self.voice, &w.example);
                            chip(ui, &format!("💬 {}", w.word), ACCENT);
                            let r = ui.add(egui::Label::new(RichText::new(&w.example).size(16.0)).sense(egui::Sense::click())).on_hover_text("누르면 영작 노트에 넣어요");
                            if r.clicked() {
                                insert = Some(w.example.clone());
                            }
                            if !w.example_ko.is_empty() {
                                crate::grammar::ko_label(ui, &w.example_ko, 14.0, muted());
                            }
                        });
                    }
                }
            }
        });
        if let Some(t) = insert {
            let d = &mut self.selfx.draft;
            if !d.is_empty() && !d.ends_with(' ') {
                d.push(' ');
            }
            d.push_str(&t);
        }
        ui.add_space(6.0);
        self.focus_words_hint(ui);
        ui.add_space(8.0);

        let mut spoken: Option<String> = None;
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                if self.selfx.job.running() {
                    spinner_label(ui, "Claude가 답변을 분석하고 있어요…");
                } else if self.selfx.local.is_some() {
                    // Offline: answered; the follow-up questions below take the mic now.
                    if ui.button("↻ 처음 질문에 다시 답하기").clicked() {
                        let pick = self.selfx.offline_pick.take();
                        self.selfx = SelfExprState { offline_pick: pick, ..Default::default() };
                    }
                } else if let Some(t) = {
                    ui.label(RichText::new("🎤 영작한 내 답을 말해요 (30~60초)").size(17.0).strong());
                    self.selfx.capture.ui(ui, &mut self.voice, 90.0)
                } {
                    self.selfx.answer = Some(t.clone());
                    spoken = Some(t.clone());
                    self.selfx.feedback = None;
                    self.selfx.error = None;
                    match &claude {
                        Some(c) => {
                            let (c, q, lvl) = (c.clone(), se.question.clone(), self.store.profile.level);
                            self.selfx.job.spawn(move || c.speech_feedback(&q, &t, lvl));
                        }
                        None => {
                            // Offline: what can be measured on the device.
                            self.store.log_attempt("self-expression", &se.question, &t, 0.0);
                            let said = format!(" {} ", normalize_words(&t));
                            let words = t.split_whitespace().count();
                            let secs = self.selfx.capture.last_secs.max(0.5);
                            let used = se.useful_phrases.iter().filter(|p| key_words_used(&said, &p.text)).map(|p| p.text.clone()).collect();
                            self.selfx.local = Some(LocalFeedback { words, wpm: words as f32 * 60.0 / secs, used });
                        }
                    }
                }
                if let Some(a) = &self.selfx.answer {
                    let n = a.split_whitespace().count();
                    ui.label(RichText::new(format!("내가 한 말 ({n}단어 / 목표 {target}): \"{a}\"")).color(if n >= target { GOOD } else { muted() }));
                }
                if let Some(e) = &self.selfx.error {
                    error_label(ui, e);
                }
            });
        });

        if let Some(t) = spoken {
            self.note_words_used(&t);
        }
        let mut drill_text: Option<(String, Vec<Phrase>)> = None;
        if let Some(f) = self.selfx.feedback.clone() {
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, v) in [("종합", f.overall), ("유창성", f.fluency), ("문법", f.grammar), ("어휘", f.vocabulary)] {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{v}")).size(26.0).strong().color(if v >= 70 { GOOD } else { warn() }));
                            ui.label(RichText::new(label).color(muted()));
                        });
                        ui.add_space(14.0);
                    }
                });
                ui.label(RichText::new(&f.feedback_ko).size(16.0));
                if !f.corrections.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("교정").color(ACCENT));
                    for c in &f.corrections {
                        ui.label(RichText::new(format!("✖ {}", c.original)).color(BAD));
                        ui.label(RichText::new(format!("✓ {}", c.corrected)).color(GOOD));
                        ui.label(RichText::new(&c.explanation_ko).color(muted()).small());
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("원어민처럼 말하면").color(ACCENT));
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &f.natural_version);
                    ui.label(RichText::new(&f.natural_version).size(17.0));
                });
            });
            ui.add_space(8.0);
            if ui.add(egui::Button::new(RichText::new("▶ 자연스러운 버전 소리 내어 읽기").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                drill_text = Some((f.natural_version.clone(), f.new_expressions.clone()));
            }
        } else if let (Some(lf), Some(q)) = (self.selfx.local.clone(), oq) {
            ui.add_space(8.0);
            let target = crate::offline_talk::target_words(self.store.profile.level);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, v, good) in [
                        ("말한 단어", format!("{}", lf.words), lf.words >= target),
                        ("말하기 속도", format!("{:.0} WPM", lf.wpm), lf.wpm >= 80.0),
                        ("쓴 표현", format!("{}/{}", lf.used.len(), se.useful_phrases.len()), !lf.used.is_empty()),
                    ] {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(v).size(26.0).strong().color(if good { GOOD } else { warn() }));
                            ui.label(RichText::new(label).color(muted()));
                        });
                        ui.add_space(14.0);
                    }
                });
                ui.label(RichText::new(if lf.words >= target {
                    "👏 충분히 길게 말했어요! 모범 답안과 비교해 보고, 아래 이어지는 질문에도 답해 보세요.".to_string()
                } else {
                    format!("조금 더 길게 말해 봐요 (이 레벨 목표 약 {target}단어). '쓸 수 있는 표현'을 넣어 이유나 예를 덧붙이면 쉬워요.")
                }));
                ui.add_space(6.0);
                ui.label(RichText::new("모범 답안").color(ACCENT));
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &q.model_answer);
                    ui.label(RichText::new(&q.model_answer).size(17.0));
                });
                crate::grammar::ko_label(ui, &q.model_answer_ko, 15.0, muted());
            });
            ui.add_space(8.0);
            if ui.add(egui::Button::new(RichText::new("▶ 모범 답안 소리 내어 읽기").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                drill_text = Some((q.model_answer.clone(), se.useful_phrases.clone()));
            }
            ui.add_space(8.0);
            self.follow_up_ui(ui, q);
        }
        if let Some((text, expressions)) = drill_text {
            let mut phrases: Vec<Phrase> = split_sentences(&text).into_iter().map(|s| Phrase { text: s, ko: String::new() }).collect();
            phrases.extend(expressions);
            self.start_phrase_queue("자기 표현 · 다시 말하기", "self-expression", &phrases);
            if let Some(q) = self.queue.as_mut() {
                q.step = "speaking";
            }
            let pick = self.selfx.offline_pick.take();
            self.selfx = SelfExprState { offline_pick: pick, ..Default::default() };
        }
    }

    /// The built-in question for today offline: one picked with 🔀, the lesson's own if it is
    /// a built-in one, else one for the level.
    fn offline_question(&mut self, lesson_q: &str) -> Option<&'static crate::offline_talk::TalkQuestion> {
        if let Some(q) = self.selfx.offline_pick.as_deref().and_then(crate::offline_talk::question_by_text) {
            return Some(q);
        }
        let q = crate::offline_talk::question_by_text(lesson_q).or_else(|| crate::offline_talk::pick_question(self.store.profile.level, |_| false, today().ordinal() as usize))?;
        self.selfx.offline_pick = Some(q.question.clone());
        Some(q)
    }

    /// "Offline scripts" notice, with a way back to Claude when a key is set.
    fn offline_banner(&mut self, ui: &mut Ui, what: &str, extra: impl FnOnce(&mut Ui)) {
        let has_key = self.store.settings.effective_api_key().is_some();
        let mut retry = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("📴 오프라인 대본 모드").strong().color(warn()));
                ui.label(RichText::new(format!(
                    "{} {what} ({} 레벨)",
                    if has_key { "Claude API를 지금 쓸 수 없어서(크레딧·네트워크)" } else { "Claude API 키가 없어서" },
                    cefr_of(self.store.profile.level)
                )));
            });
            ui.horizontal_wrapped(|ui| {
                extra(ui);
                if has_key && ui.button("🔄 Claude 다시 연결").clicked() {
                    retry = true;
                }
            });
        });
        if retry {
            self.api_down.store(false, std::sync::atomic::Ordering::Relaxed);
            self.lesson_error = None;
        }
    }

    /// Offline follow-up questions on the same topic, asked one at a time.
    fn follow_up_ui(&mut self, ui: &mut Ui, q: &'static crate::offline_talk::TalkQuestion) {
        let n = q.follow_ups.len();
        let i = self.selfx.follow;
        let t = crate::art::since_event(ui, egui::Id::new("cheer_follow_ups"), i as u64);
        let Some(f) = q.follow_ups.get(i) else {
            if n > 0 {
                crate::art::cheer_strip(ui, t, &format!("🎉 이어지는 질문 {n}개에 모두 답했어요!"));
                card(ui, |ui| {
                    ui.label(RichText::new(format!("✅ 이어지는 질문 {n}개 완료")).strong().color(GOOD));
                });
                if !self.step_done("speaking") {
                    self.complete_step("speaking");
                }
            }
            return;
        };
        if self.selfx.follow_said != Some(i) {
            self.selfx.follow_said = Some(i);
            self.voice.say(&f.question);
        }
        let mut next = false;
        let mut follow_said: Option<String> = None;
        card(ui, |ui| {
            ui.label(RichText::new(format!("💬 이어지는 질문 {}/{n}", i + 1)).color(ACCENT).strong());
            ui.horizontal_wrapped(|ui| {
                speaker_button(ui, &mut self.voice, &f.question);
                ui.label(RichText::new(&f.question).size(20.0).strong());
            });
            crate::grammar::ko_label(ui, &f.question_ko, 15.0, muted());
            ui.add_space(4.0);
            match self.selfx.follow_answer.clone() {
                None => {
                    ui.vertical_centered(|ui| {
                        if let Some(a) = self.selfx.follow_capture.ui(ui, &mut self.voice, 60.0) {
                            self.store.log_attempt("self-expression", &f.question, &a, 0.0);
                            self.selfx.follow_answer = Some(a.clone());
                            follow_said = Some(a);
                        }
                    });
                }
                Some(a) => {
                    ui.label(RichText::new(format!("내가 한 말: \"{a}\"")).color(muted()));
                    ui.label(RichText::new("예시 답변").color(ACCENT));
                    ui.horizontal_wrapped(|ui| {
                        speaker_button(ui, &mut self.voice, &f.sample_answer);
                        ui.label(RichText::new(&f.sample_answer).size(17.0));
                    });
                    next = ui.add(egui::Button::new(RichText::new(if i + 1 < n { "다음 질문 ▶" } else { "완료 ▶" }).size(16.0)).fill(ACCENT)).clicked();
                }
            }
        });
        if let Some(a) = follow_said {
            self.note_words_used(&a);
        }
        if next {
            self.selfx.follow += 1;
            self.selfx.follow_answer = None;
        }
    }

    fn scenario(&self) -> BusinessScenario {
        let offline = self.talk_offline();
        if self.roleplay.mode == 2 && offline {
            let ss = crate::offline_talk::scenarios(self.store.profile.level);
            return ss.get(self.roleplay.collection.min(ss.len().saturating_sub(1))).map(|s| s.to_business()).unwrap_or_default();
        }
        if self.roleplay.mode == 0 && offline {
            // Today's scene: the lesson's own when it has a script, else one for the level.
            let lesson = self.lesson.as_ref().map(|l| l.business.clone()).unwrap_or_default();
            if crate::offline_talk::scenario_by_title(&lesson.title).is_some() {
                return lesson;
            }
            let day = today().ordinal() as usize;
            return crate::offline_talk::pick_scenario(self.store.profile.level, |_| false, day).map(|s| s.to_business()).unwrap_or(lesson);
        }
        if self.roleplay.mode == 2 {
            let (title, ko, ai, usr) = crate::content::ROLEPLAYS[self.roleplay.collection.min(crate::content::ROLEPLAYS.len() - 1)];
            return BusinessScenario { title: title.into(), scenario_ko: ko.into(), ai_role: ai.into(), user_role: usr.into(), ..Default::default() };
        }
        if self.roleplay.mode == 3 {
            let (sit, ai, usr) = &self.roleplay.custom;
            return BusinessScenario {
                title: "My scene".into(),
                scenario_ko: sit.clone(),
                ai_role: if ai.trim().is_empty() { "a conversation partner".into() } else { ai.clone() },
                user_role: if usr.trim().is_empty() { "myself".into() } else { usr.clone() },
                ..Default::default()
            };
        }
        if self.roleplay.mode == 1 {
            let wk = curriculum::week(curriculum::week_number(&self.store.profile));
            BusinessScenario {
                title: format!("Free talk: {}", wk.theme),
                scenario_ko: format!("'{}' 주제로 원어민 친구와 자유롭게 대화합니다.", wk.theme_ko),
                ai_role: "Alex, a friendly native English-speaking friend".into(),
                user_role: "themselves".into(),
                opening_line: format!("Hey! Good to see you. So, I wanted to ask you — {}?", wk.theme.to_lowercase()),
                key_phrases: Vec::new(),
                guide: Vec::new(),
            }
        } else {
            self.lesson.as_ref().map(|l| l.business.clone()).unwrap_or_default()
        }
    }

    /// ☕ 자유 회화: category tabs, then the category's 50 scenarios (coloured by finished
    /// conversations, easy ones first); a click starts the conversation — the app asks, the
    /// learner answers in English.
    fn free_talk_picker(&mut self, ui: &mut Ui) {
        let cats = crate::freetalk::CATEGORIES;
        let done_in = |cat: &str, me: &Self| crate::freetalk::by_category(cat).iter().filter(|s| me.store.word_stat(&Self::scene_key(&s.title)).passes > 0).count();
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new("☕ 자유 회화 · 상황을 고르면 상대가 질문하고, 나는 영어로 대답해요").size(18.0).strong());
            ui.label(RichText::new("카테고리마다 50개 상황 · 질문 5개씩 · 쉬움 → 보통 → 도전 순서").color(muted()));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (i, (id, name, icon)) in cats.iter().enumerate() {
                    let n = crate::freetalk::by_category(id).len();
                    let label = RichText::new(format!("{icon} {name} {}/{n}", done_in(id, self))).size(15.5);
                    if ui.selectable_label(self.roleplay.free_cat == i, if self.roleplay.free_cat == i { label.strong() } else { label }).clicked() {
                        self.roleplay.free_cat = i;
                    }
                }
            });
        });
        let (id, name, icon) = cats[self.roleplay.free_cat.min(cats.len() - 1)];
        let list = crate::freetalk::by_category(id);
        let mut pick: Option<&'static crate::freetalk::FreeScenario> = None;
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("{icon} {name} · {}개 상황", list.len())).size(17.0).strong());
            ui.label(RichText::new("상황을 클릭하면 바로 대화가 시작돼요 (5개 질문에 모두 답하면 1회 완료)").color(muted()).small());
            ui.add_space(4.0);
            for (i, s) in list.iter().enumerate() {
                let (color, mark) = crate::screens::practice::word_mark(self.store.word_stat(&Self::scene_key(&s.title)).passes);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{:>2}.", i + 1)).color(muted()).monospace());
                    let lc = match s.level {
                        1 => GOOD,
                        2 => ACCENT,
                        _ => crate::art::VIOLET,
                    };
                    chip(ui, crate::freetalk::level_label(s.level), lc);
                    let r = ui.add(egui::Label::new(RichText::new(&s.title).size(16.0).strong().color(color)).sense(egui::Sense::click())).on_hover_text(format!("{} · {}", s.situation_ko, s.ai_role));
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        pick = Some(*s);
                    }
                    ui.label(RichText::new(&s.title_ko).color(muted()));
                    ui.label(RichText::new(mark).small().color(color));
                });
            }
        });
        if let Some(s) = pick {
            crate::applog!("free-talk: {} 시작", s.id);
            self.start_conversation(s.to_business());
        }
    }

    /// Key in word_stats.json counting finished conversations of a scene.
    fn scene_key(title: &str) -> String {
        format!("대화: {title}")
    }

    /// Opens a conversation for `sc` right away (the AI speaks first).
    fn start_conversation(&mut self, sc: BusinessScenario) {
        if self.talk_offline() {
            // A 자유 회화 scenario (it has its questions as a guide) or a built-in conversation;
            // titles can repeat between the two libraries.
            let free = || crate::freetalk::by_title(&sc.title).map(|f| f.to_script());
            let built_in = || crate::offline_talk::scenario_by_title(&sc.title).cloned();
            let script = if sc.guide.is_empty() { built_in().or_else(free) } else { free().or_else(built_in) };
            let Some(script) = script else {
                self.roleplay.error = Some("이 대화는 Claude가 필요해요. 오프라인에서는 '오늘의 비즈니스'나 '상황 모음'의 대본 대화로 연습하세요.".into());
                return;
            };
            crate::applog!("business: 오프라인 대본 대화 시작 · {}", sc.title);
            let first = &script.script[0];
            self.roleplay.started = true;
            self.roleplay.offline = true;
            self.roleplay.scenario = Some(sc);
            self.voice.say(&first.ai);
            self.roleplay.lines.push(ChatLine { user: false, text: first.ai.clone(), ko: first.ai_ko.clone(), correction: String::new(), correction_ko: String::new() });
            self.roleplay.suggestions = first.replies.clone();
            self.roleplay.script = Some(script);
            return;
        }
        let Some(c) = self.claude() else {
            self.roleplay.error = Some("AI 회화 상대는 Claude API 키가 필요합니다. 설정에서 키를 입력하세요.".into());
            return;
        };
        let level = self.store.profile.level;
        crate::applog!("business: 대화 시작 · {}", sc.title);
        self.roleplay.started = true;
        self.roleplay.scenario = Some(sc.clone());
        if sc.opening_line.trim().is_empty() {
            // The AI opens the scene itself.
            self.roleplay.job.spawn(move || c.roleplay_turn(&sc, &[], level));
        } else {
            self.voice.say(&sc.opening_line);
            // A 자유 회화 scenario's first question comes with its translation and a sample answer.
            let free = crate::freetalk::by_title(&sc.title).filter(|_| !sc.guide.is_empty()).and_then(|f| f.questions.first());
            let ko = free.map(|q| q.q_ko.clone()).unwrap_or_default();
            if let Some(q) = free {
                self.roleplay.suggestions = vec![Phrase { text: q.sample.clone(), ko: q.sample_ko.clone() }];
            }
            self.roleplay.lines.push(ChatLine { user: false, text: sc.opening_line.clone(), ko, correction: String::new(), correction_ko: String::new() });
        }
    }

    /// A conversation reached 4 turns (or its end): counts once toward the scene's colour.
    fn count_conversation(&mut self) {
        if self.roleplay.counted {
            return;
        }
        self.roleplay.counted = true;
        if let Some(sc) = &self.roleplay.scenario {
            let key = Self::scene_key(&sc.title);
            self.store.record_word_practice(&key, true);
        }
    }

    fn reset_roleplay(&mut self) {
        let old = std::mem::take(&mut self.roleplay);
        self.roleplay = RoleplayState { mode: old.mode, collection: old.collection, custom: old.custom, free_cat: old.free_cat, ..Default::default() };
    }

    /// The other person's next line (from Claude or the offline script).
    fn apply_turn(&mut self, t: RoleplayTurn) {
        if let Some(last) = self.roleplay.lines.iter_mut().rev().find(|l| l.user) {
            last.correction = t.correction.clone();
            last.correction_ko = t.correction_ko.clone();
            let original = last.text.clone();
            // A script's hint is another way to say it, not a mistake.
            if !self.roleplay.offline {
                self.store.add_correction("회화", &original, &t.correction, &t.correction_ko);
            }
        }
        self.voice.say(&t.reply);
        self.roleplay.lines.push(ChatLine { user: false, text: t.reply, ko: t.reply_ko, correction: String::new(), correction_ko: String::new() });
        self.roleplay.suggestions = t.suggestions;
        if t.finished {
            self.roleplay.finished = true;
            self.roleplay.summary = t.summary_ko;
        }
        let user_turns = self.roleplay.lines.iter().filter(|l| l.user).count();
        if self.roleplay.finished || user_turns >= 4 {
            self.count_conversation();
            if !self.step_done("business") {
                self.complete_step("business");
            }
        }
    }

    pub fn business_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) || !self.lesson_ready_ui(ui) {
            return;
        }
        let claude = self.claude();
        let level = self.store.profile.level;
        if let Some(r) = self.roleplay.job.poll() {
            match r {
                Ok(t) => self.apply_turn(t),
                Err(e) => self.roleplay.error = Some(format!("{e:#}")),
            }
        }
        let offline = self.talk_offline();

        page_title(ui, "💼 회화 · 비즈니스 영어", "");
        ui.horizontal_wrapped(|ui| {
            let was = self.roleplay.mode;
            ui.selectable_value(&mut self.roleplay.mode, 0, "💼 오늘의 비즈니스");
            ui.selectable_value(&mut self.roleplay.mode, 1, "☕ 자유 회화");
            ui.selectable_value(&mut self.roleplay.mode, 2, "🎭 상황 모음");
            ui.selectable_value(&mut self.roleplay.mode, 3, "✏ 나만의 상황");
            if was != self.roleplay.mode {
                self.reset_roleplay();
            }
        });
        if offline {
            ui.add_space(4.0);
            self.offline_banner(ui, "레벨에 맞게 미리 써 둔 대본으로 상대가 질문하고 대화를 이어 가요.", |_| {});
            if self.roleplay.mode == 3 {
                card(ui, |ui| {
                    ui.label("✏ 나만의 상황은 Claude가 필요해요. 오프라인에서는 ☕ 자유 회화, 💼 오늘의 비즈니스, 🎭 상황 모음의 대본 대화로 연습하세요.");
                });
                return;
            }
        }
        if !self.roleplay.started && self.roleplay.mode == 1 {
            self.free_talk_picker(ui);
            return;
        }
        if !self.roleplay.started && self.roleplay.mode == 2 {
            // Scenes coloured by finished conversations (like the 새 단어 list); a click starts one.
            let mut pick = None;
            // Offline: the level's scripted conversations instead of Claude's scenes.
            let scenes: Vec<(String, String)> = if offline {
                crate::offline_talk::scenarios(self.store.profile.level).iter().map(|s| (s.title.clone(), s.scenario_ko.clone())).collect()
            } else {
                crate::content::ROLEPLAYS.iter().map(|r| (r.0.to_string(), r.1.to_string())).collect()
            };
            card(ui, |ui| {
                let counts: Vec<u32> = scenes.iter().map(|r| self.store.word_stat(&Self::scene_key(&r.0)).passes).collect();
                ui.horizontal_wrapped(|ui| {
                    let done = counts.iter().filter(|c| **c >= WORD_DONE_PASSES).count();
                    ui.label(RichText::new(format!("완료한 상황 {done}/{}", counts.len())).strong());
                    ui.label(RichText::new("· 상황을 클릭하면 바로 대화가 시작돼요 (대화 4번 이상 = 1회)").color(muted()));
                });
                ui.add_space(6.0);
                for (i, (title, ko)) in scenes.iter().enumerate() {
                    let (color, label) = crate::screens::practice::word_mark(counts[i]);
                    ui.horizontal_wrapped(|ui| {
                        let r = ui.add(egui::Label::new(RichText::new(title).size(16.5).strong().color(color)).sense(egui::Sense::click())).on_hover_text("클릭하면 바로 대화 시작");
                        if r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            pick = Some(i);
                        }
                        ui.label(RichText::new(label).small().color(color));
                        ui.label(RichText::new(ko).color(muted()));
                    });
                }
                ui.add_space(4.0);
                if ui.button("🎲 랜덤으로 시작").clicked() {
                    pick = Some(rand::random_range(0..scenes.len().max(1)));
                }
            });
            if let Some(i) = pick {
                self.roleplay.collection = i;
                let sc = self.scenario();
                self.start_conversation(sc);
                return;
            }
        }
        if !self.roleplay.started && self.roleplay.mode == 3 {
            card(ui, |ui| {
                ui.label("연습하고 싶은 상황을 적어 주세요 (한국어/영어 모두 가능)");
                ui.add(egui::TextEdit::singleline(&mut self.roleplay.custom.0).hint_text("예: 해외 출장 중 호텔 예약이 누락되어 항의하기").desired_width(f32::INFINITY));
                ui.horizontal_wrapped(|ui| {
                    ui.label("상대 역할");
                    ui.add(egui::TextEdit::singleline(&mut self.roleplay.custom.1).hint_text("예: hotel manager"));
                    ui.label("내 역할");
                    ui.add(egui::TextEdit::singleline(&mut self.roleplay.custom.2).hint_text("예: business traveler"));
                });
            });
            if self.roleplay.custom.0.trim().is_empty() {
                return;
            }
        }
        let sc = self.roleplay.scenario.clone().unwrap_or_else(|| self.scenario());
        let mut start_now = false;
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(&sc.title).size(20.0).strong());
            ui.label(RichText::new(&sc.scenario_ko).color(muted()));
            ui.label(RichText::new(format!("상대: {} · 나: {}", sc.ai_role, sc.user_role)).color(muted()).small());
            if self.roleplay.mode != 2 {
                let (color, label) = crate::screens::practice::word_mark(self.store.word_stat(&Self::scene_key(&sc.title)).passes);
                ui.label(RichText::new(format!("이 상황 대화: {label}")).small().color(color));
            }
            if !sc.key_phrases.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("핵심 표현 · 클릭하면 바로 대화 시작 (따라 말하기·대화에서 쓸 때마다 횟수가 올라요)").color(ACCENT));
                for p in &sc.key_phrases {
                    let (color, label) = crate::screens::practice::word_mark(self.store.word_stat(&p.text).passes);
                    ui.horizontal_wrapped(|ui| {
                        speaker_button(ui, &mut self.voice, &p.text);
                        let r = ui.add(egui::Label::new(RichText::new(&p.text).strong().color(color)).sense(egui::Sense::click()));
                        if !self.roleplay.started {
                            let r = r.on_hover_text("클릭하면 바로 대화 시작");
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                start_now = true;
                            }
                        }
                        ui.label(RichText::new(label).small().color(color));
                        crate::grammar::ko_label(ui, &p.ko, 16.0, muted());
                    });
                }
            }
        });
        self.focus_words_hint(ui);
        if start_now && !self.roleplay.started {
            self.start_conversation(sc.clone());
            return;
        }
        if !sc.key_phrases.is_empty() && !self.roleplay.started && ui.button("🔁 핵심 표현 먼저 따라 말하기").clicked() {
            self.start_phrase_queue("핵심 표현 연습", "business", &sc.key_phrases);
            return;
        }

        let c = claude.filter(|_| !offline);
        if c.is_none() && !offline {
            ui.add_space(8.0);
            error_label(ui, "AI 회화 상대는 Claude API 키가 필요합니다. 설정에서 키를 입력하세요. (핵심 표현 연습은 가능해요)");
            return;
        }
        if let Some(e) = self.roleplay.error.clone().filter(|_| !self.roleplay.started) {
            error_label(ui, &e);
        }

        if !self.roleplay.started {
            ui.add_space(8.0);
            if ui.add(egui::Button::new(RichText::new("▶ 대화 시작").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                self.start_conversation(sc.clone());
            }
            return;
        }

        ui.add_space(8.0);
        ui.checkbox(&mut self.roleplay.show_ko, "번역 보기");
        let show_ko = self.roleplay.show_ko;
        let mut replay: Option<String> = None;
        for l in &self.roleplay.lines {
            let (fill, align) = if l.user { (ACCENT.gamma_multiply(0.5), egui::Align::Max) } else { (card_bg(), egui::Align::Min) };
            ui.with_layout(egui::Layout::top_down(align), |ui| {
                egui::Frame::new().fill(fill).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.horizontal_wrapped(|ui| {
                        if !l.user && ui.small_button("🔊").clicked() {
                            replay = Some(l.text.clone());
                        }
                        ui.label(RichText::new(&l.text).size(16.0));
                    });
                    if show_ko && !l.ko.is_empty() {
                        crate::grammar::ko_label(ui, &l.ko, 12.5, muted());
                    }
                    if !l.correction.is_empty() {
                        ui.label(RichText::new(format!("✓ 더 자연스럽게: {}", l.correction)).color(themed(Color32::from_rgb(150, 255, 190), Color32::from_rgb(20, 125, 70))));
                        ui.label(RichText::new(&l.correction_ko).color(muted()).small());
                    }
                });
            });
            ui.add_space(4.0);
        }
        if let Some(t) = replay {
            self.voice.say(&t);
        }

        if self.roleplay.finished {
            card(ui, |ui| {
                ui.label(RichText::new("🎉 대화 완료!").size(20.0).strong());
                ui.label(&self.roleplay.summary);
            });
            let corrections: Vec<Phrase> = self
                .roleplay
                .lines
                .iter()
                .filter(|l| l.user && !l.correction.is_empty())
                .map(|l| Phrase { text: l.correction.clone(), ko: l.correction_ko.clone() })
                .collect();
            ui.horizontal_wrapped(|ui| {
                if !corrections.is_empty() && ui.button(format!("🔁 교정된 문장 {}개 따라 말하기", corrections.len())).clicked() {
                    self.start_phrase_queue("교정 문장 연습", "business", &corrections);
                }
                if ui.button("↻ 새 대화").clicked() {
                    self.reset_roleplay();
                }
            });
            return;
        }

        if !self.roleplay.suggestions.is_empty() {
            ui.label(RichText::new("💡 이렇게 말해 보세요").color(warn()));
            for p in self.roleplay.suggestions.clone() {
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &p.text);
                    ui.label(RichText::new(&p.text).strong());
                    crate::grammar::ko_label(ui, &p.ko, 16.0, muted());
                });
            }
        }
        let mut turn_now = None;
        let mut said_now: Option<String> = None;
        ui.vertical_centered(|ui| {
            if self.roleplay.job.running() {
                spinner_label(ui, "상대가 답하는 중…");
            } else if let Some(t) = self.roleplay.capture.ui(ui, &mut self.voice, 45.0) {
                // A key phrase used in the conversation counts as practice too.
                let said = normalize_words(&t);
                for p in &sc.key_phrases {
                    let phrase = normalize_words(&p.text);
                    if !phrase.is_empty() && format!(" {said} ").contains(&format!(" {phrase} ")) {
                        self.store.record_word_practice(&p.text, true);
                    }
                }
                self.store.log_attempt(["business", "free-talk", "roleplay", "custom-roleplay"][self.roleplay.mode.min(3)], "", &t, 0.0);
                self.roleplay.lines.push(ChatLine { user: true, text: t.clone(), ko: String::new(), correction: String::new(), correction_ko: String::new() });
                said_now = Some(t.clone());
                self.roleplay.error = None;
                let user_turns = self.roleplay.lines.iter().filter(|l| l.user).count();
                match (&c, self.roleplay.script.clone().filter(|_| self.roleplay.offline)) {
                    // Offline: the script answers at once.
                    (_, Some(script)) => turn_now = Some(script.turn(user_turns, &t)),
                    (Some(c), None) => {
                        let history: Vec<(bool, String)> = self.roleplay.lines.iter().map(|l| (l.user, l.text.clone())).collect();
                        let (c, scc) = (c.clone(), sc.clone());
                        self.roleplay.job.spawn(move || c.roleplay_turn(&scc, &history, level));
                    }
                    (None, None) => self.roleplay.error = Some("Claude를 쓸 수 없어 대화를 이어 갈 수 없어요. 새 대화로 대본 대화를 시작하세요.".into()),
                }
            }
            if let Some(e) = &self.roleplay.error {
                error_label(ui, e);
                if !self.roleplay.offline && self.claude().is_some() && ui.button("다시 시도").clicked() {
                    let history: Vec<(bool, String)> = self.roleplay.lines.iter().map(|l| (l.user, l.text.clone())).collect();
                    let c2 = self.claude().unwrap();
                    self.roleplay.job.spawn(move || c2.roleplay_turn(&sc, &history, level));
                    self.roleplay.error = None;
                }
            }
        });
        if let Some(t) = said_now {
            self.note_words_used(&t);
        }
        if let Some(t) = turn_now {
            self.apply_turn(t);
        }
        if ui.button("대화 끝내기").clicked() {
            self.roleplay.finished = true;
            self.roleplay.summary = "대화를 직접 마쳤어요. 교정된 문장을 따라 말하며 정리해 보세요.".into();
            if self.roleplay.lines.iter().filter(|l| l.user).count() >= 4 {
                self.count_conversation();
                if !self.step_done("business") {
                    self.complete_step("business");
                }
            }
        }
    }
}

/// Lower-case words without punctuation, single-spaced ("We're on track." → "we're on track").
fn normalize_words(t: &str) -> String {
    t.to_lowercase()
        .replace('’', "'")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_paragraph() {
        let s = split_sentences("I work at a bank. I love it! Do you? Yes.");
        assert_eq!(s, vec!["I work at a bank.", "I love it!", "Do you? Yes."]);
    }
}

/// The content words of a phrase (frames like "I usually ..." → "i usually") all appear in what
/// was said.
fn key_words_used(said: &str, phrase: &str) -> bool {
    let words: Vec<String> = normalize_words(phrase).split(' ').filter(|w| w.len() > 1 && !w.contains("...")).map(|w| w.to_string()).collect();
    !words.is_empty() && words.iter().all(|w| said.contains(&format!(" {w} ")))
}
