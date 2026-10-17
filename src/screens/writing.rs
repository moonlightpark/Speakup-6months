//! ✏ Writing (20% of the daily plan): dictation of learned sentences, Korean → English
//! translation, and a short diary that Claude corrects — the corrected text is then read aloud,
//! so writing feeds back into speaking.

use crate::app::App;
use crate::claude::{TranslationCheck, WritingFeedback};
use crate::models::*;
use crate::scoring::{self, ScoreResult};
use crate::widgets::*;
use egui::{RichText, Ui};

/// Exercises in one daily writing set (dictation + translation; the diary is a bonus).
pub const WRITING_GOAL: u32 = 6;

const DIARY_PROMPTS: &[(&str, &str)] = &[
    ("What did you do today? Write 3-5 sentences.", "오늘 무엇을 했나요?"),
    ("Describe your favorite place to relax.", "가장 좋아하는 휴식 장소를 소개해 보세요."),
    ("Write a short message inviting a friend to dinner.", "친구를 저녁 식사에 초대하는 짧은 메시지를 써 보세요."),
    ("What's something you want to get better at this year, and why?", "올해 더 잘하고 싶은 것과 그 이유는?"),
    ("Write an email to a coworker asking to move a meeting.", "동료에게 회의 시간을 바꾸자는 이메일을 써 보세요."),
    ("Tell me about a movie or show you enjoyed recently.", "최근 재미있게 본 영화나 드라마를 소개해 보세요."),
    ("What does your perfect weekend look like?", "완벽한 주말은 어떤 모습인가요?"),
];

#[derive(Default)]
pub struct WritingState {
    pub mode: usize,
    /// (English, Korean) practice sentences for dictation / translation.
    queue: Vec<(String, String)>,
    idx: usize,
    pub input: String,
    checked: Option<ScoreResult>,
    translation: Pending<TranslationCheck>,
    translation_result: Option<Result<TranslationCheck, String>>,
    diary: Pending<WritingFeedback>,
    pub diary_result: Option<Result<WritingFeedback, String>>,
    read_aloud: Option<Drill>,
    played: bool,
}

impl WritingState {
    #[cfg(test)]
    pub fn translation_done(&self) -> bool {
        self.translation_result.is_some()
    }
}

impl App {
    /// Sentences the learner has met: saved sentences first (they need review anyway), then today's lesson.
    fn writing_sentences(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = Vec::new();
        let mut push = |en: &str, ko: &str| {
            let words = en.split_whitespace().count();
            if !ko.is_empty() && (3..=16).contains(&words) && !v.iter().any(|x| x.0 == en) {
                v.push((en.to_string(), ko.to_string()));
            }
        };
        for i in self.store.items.iter().rev() {
            match i.kind {
                ItemKind::Sentence => push(&i.text, &i.meaning_ko),
                ItemKind::Word => push(&i.example, &i.example_ko),
            }
        }
        if let Some(l) = &self.lesson {
            for s in &l.sentences {
                push(&s.text, &s.ko);
            }
            for w in &l.words {
                push(&w.example, &w.example_ko);
            }
        }
        // Mix the day in so consecutive days don't start with the same sentences.
        let shift = (today().ordinal() as usize) % v.len().max(1);
        v.rotate_left(shift);
        v
    }

    pub fn writing_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "✏ 쓰기", "들은 문장 받아쓰기 · 한국어를 영어로 쓰기 · 짧은 영어 일기 (Claude 교정 후 소리 내어 읽기)");
        let done = self.store.profile.today.writings_done;
        ui.horizontal_wrapped(|ui| {
            chip(ui, &format!("오늘 {done}/{WRITING_GOAL}개"), if done >= WRITING_GOAL { GOOD } else { ACCENT });
            for (i, label) in ["🎧 받아쓰기", "🔁 한→영 작문", "📝 영어 일기"].iter().enumerate() {
                if ui.selectable_label(self.writing.mode == i, *label).clicked() && self.writing.mode != i {
                    self.writing.mode = i;
                    self.writing.input.clear();
                    self.writing.checked = None;
                    self.writing.translation_result = None;
                    self.writing.played = false;
                    self.writing.read_aloud = None;
                }
            }
        });
        ui.add_space(6.0);
        if self.writing.queue.is_empty() {
            self.writing.queue = self.writing_sentences();
        }
        match self.writing.mode {
            0 | 1 => self.sentence_writing_ui(ui),
            _ => self.diary_ui(ui),
        }
    }

    fn sentence_writing_ui(&mut self, ui: &mut Ui) {
        let dictation = self.writing.mode == 0;
        if self.writing.queue.is_empty() {
            card(ui, |ui| {
                ui.label("아직 배운 문장이 없어요. 오늘의 레슨(새 단어·문장 읽기)을 먼저 해 보세요.");
            });
            return;
        }
        let claude = self.claude();
        if let Some(r) = self.writing.translation.poll() {
            let ok = r.as_ref().map(|c| c.ok).unwrap_or(false);
            self.writing.translation_result = Some(r.map_err(|e| format!("{e:#}")));
            if ok {
                if let Some(c) = self.writing.checked.as_mut() {
                    c.score = c.score.max(90.0);
                }
            }
        }
        let st = &mut self.writing;
        let (en, ko) = st.queue[st.idx % st.queue.len()].clone();
        let mut next = false;
        let mut finished: Option<(f32, String)> = None;
        card(ui, |ui| {
            if dictation {
                ui.label(RichText::new("문장을 듣고 그대로 써 보세요.").size(17.0));
                if !st.played {
                    st.played = true;
                    self.voice.say(&en);
                }
                ui.horizontal(|ui| {
                    if ui.button("🔊 다시 듣기").clicked() {
                        self.voice.say(&en);
                    }
                    if ui.button("🐢 천천히").clicked() {
                        self.voice.say_slow(&en);
                    }
                    ui.label(RichText::new(format!("힌트: {}", ko)).color(MUTED).small());
                });
            } else {
                ui.label(RichText::new("영어로 써 보세요.").color(MUTED));
                ui.label(RichText::new(&ko).size(22.0).strong().color(GLOSS));
            }
            ui.add_space(4.0);
            let resp = ui.add(egui::TextEdit::singleline(&mut st.input).desired_width(f32::INFINITY).font(egui::FontId::proportional(19.0)).hint_text("여기에 영어로 입력"));
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if st.checked.is_none() {
                if (ui.add_enabled(!st.input.trim().is_empty(), egui::Button::new(RichText::new("확인").strong()).fill(ACCENT)).clicked() || enter) && !st.input.trim().is_empty() {
                    let r = scoring::score(&en, &st.input);
                    if !dictation && r.score < 90.0 {
                        // Another wording can be just as right; let Claude judge.
                        if let Some(c) = claude.clone() {
                            let (k, e, a) = (ko.clone(), en.clone(), st.input.clone());
                            st.translation.spawn(move || c.check_translation(&k, &e, &a));
                        }
                    }
                    st.checked = Some(r);
                }
            }
            if let Some(r) = &st.checked {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:.0}점", r.score)).size(22.0).strong().color(if r.score >= 80.0 { GOOD } else { WARN }));
                    ui.label(RichText::new(if dictation { "정답과 비교 (초록 = 맞음, 빨강 = 다름)" } else { "모범 답안과 비교" }).color(MUTED));
                });
                word_feedback(ui, r, 19.0);
                if st.translation.running() {
                    spinner_label(ui, "Claude가 다른 표현도 맞는지 확인 중…");
                }
                if let Some(t) = &st.translation_result {
                    match t {
                        Ok(t) => {
                            ui.label(RichText::new(format!("{} {}", if t.ok { "✅" } else { "💡" }, t.feedback_ko)).color(if t.ok { GOOD } else { WARN }));
                            if !t.better.is_empty() && t.better != en {
                                ui.label(RichText::new(format!("더 자연스럽게: {}", t.better)).color(GLOSS));
                            }
                        }
                        Err(e) => error_label(ui, e),
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("🔊 정답 듣기").clicked() {
                        self.voice.say(&en);
                    }
                    if ui.button("🎤 말해 보기").clicked() {
                        st.read_aloud = Some(Drill::new(&en, &ko, "", 1, false, false).without_autoplay());
                    }
                    if primary_button(ui, "다음 ▶").clicked() {
                        finished = Some((r.score, st.input.clone()));
                        next = true;
                    }
                });
            }
        });
        let mut drill_event = DrillEvent::None;
        if let Some(d) = self.writing.read_aloud.as_mut() {
            let pass = self.store.settings.pass_score;
            drill_event = card(ui, |ui| d.ui(ui, &mut self.voice, pass, claude.as_ref()));
        }
        match drill_event {
            DrillEvent::Attempt(a) => self.record_attempt("writing", &en, &a),
            DrillEvent::Completed => self.writing.read_aloud = None,
            DrillEvent::None => {}
        }
        if let Some((score, text)) = finished {
            let kind = if dictation { "dictation" } else { "translation" };
            self.finish_writing(kind, if dictation { "(듣기)" } else { &ko }, &text, &en, score);
        }
        if next {
            let st = &mut self.writing;
            st.idx += 1;
            st.input.clear();
            st.checked = None;
            st.translation_result = None;
            st.played = false;
            st.read_aloud = None;
        }
    }

    fn diary_ui(&mut self, ui: &mut Ui) {
        let claude = self.claude();
        let level = self.store.profile.level;
        if let Some(r) = self.writing.diary.poll() {
            match r {
                Ok(f) => {
                    for c in &f.corrections {
                        self.store.add_correction("writing", &c.original, &c.corrected, &c.explanation_ko);
                    }
                    let text = self.writing.input.clone();
                    let prompt = self.diary_prompt().0.to_string();
                    self.finish_writing("diary", &prompt, &text, &f.natural_version, f.score as f32);
                    self.writing.read_aloud = Some(Drill::new(&f.natural_version, &f.natural_version_ko, "교정된 글을 소리 내어 읽어 보세요", 1, false, false).without_autoplay());
                    self.writing.diary_result = Some(Ok(f));
                }
                Err(e) => self.writing.diary_result = Some(Err(format!("{e:#}"))),
            }
        }
        let (prompt, prompt_ko) = self.diary_prompt();
        let st = &mut self.writing;
        card(ui, |ui| {
            ui.label(RichText::new(prompt).size(19.0).strong());
            ui.label(RichText::new(prompt_ko).color(MUTED));
            ui.add(egui::TextEdit::multiline(&mut st.input).desired_rows(6).desired_width(f32::INFINITY).font(egui::FontId::proportional(18.0)).hint_text("영어로 3~5문장 써 보세요. 틀려도 괜찮아요!"));
            match &claude {
                None => {
                    ui.label(RichText::new("영어 일기 교정은 Claude API 키가 필요합니다 (설정).").color(WARN));
                }
                Some(c) => {
                    if st.diary.running() {
                        spinner_label(ui, "Claude가 교정하고 있어요…");
                    } else if ui.add_enabled(st.input.split_whitespace().count() >= 3, egui::Button::new(RichText::new("✨ Claude 교정 받기").strong()).fill(ACCENT)).clicked() {
                        let (c, p, t) = (c.clone(), prompt.to_string(), st.input.clone());
                        st.diary_result = None;
                        st.diary.spawn(move || c.writing_feedback(&p, &t, level));
                    }
                }
            }
        });
        if let Some(r) = &st.diary_result {
            card(ui, |ui| match r {
                Ok(f) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}점", f.score)).size(24.0).strong().color(if f.score >= 80 { GOOD } else { WARN }));
                        ui.label(&f.feedback_ko);
                    });
                    for c in &f.corrections {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&c.original).strikethrough().color(BAD));
                            ui.label("→");
                            ui.label(RichText::new(&c.corrected).color(GOOD).strong());
                            ui.label(RichText::new(&c.explanation_ko).color(MUTED).small());
                        });
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("원어민 버전").strong());
                    ui.label(RichText::new(&f.natural_version).size(18.0));
                    ui.label(RichText::new(&f.natural_version_ko).color(GLOSS));
                    if ui.button("🔊 원어민 버전 듣기").clicked() {
                        self.voice.say(&f.natural_version);
                    }
                }
                Err(e) => error_label(ui, e),
            });
        }
        let mut ev = DrillEvent::None;
        if let Some(d) = self.writing.read_aloud.as_mut() {
            let pass = self.store.settings.pass_score;
            ev = card(ui, |ui| d.ui(ui, &mut self.voice, pass, claude.as_ref()));
        }
        match ev {
            DrillEvent::Attempt(a) => {
                let t = self.writing.read_aloud.as_ref().map(|d| d.target.clone()).unwrap_or_default();
                self.record_attempt("writing", &t, &a);
            }
            DrillEvent::Completed => {
                self.writing.read_aloud = None;
                self.toast("👏 교정된 글을 소리 내어 읽었어요!");
            }
            DrillEvent::None => {}
        }
    }

    /// Today's diary prompt: the lesson's self-expression question, else a rotating prompt.
    fn diary_prompt(&self) -> (&'static str, &'static str) {
        DIARY_PROMPTS[(today().ordinal() as usize + self.store.writings.iter().filter(|w| w.kind == "diary").count()) % DIARY_PROMPTS.len()]
    }

    fn finish_writing(&mut self, kind: &str, prompt: &str, text: &str, corrected: &str, score: f32) {
        self.store.writings.push(WritingRecord { date: Some(today()), kind: kind.into(), prompt: prompt.into(), text: text.into(), corrected: corrected.into(), score });
        self.store.save_writings();
        self.store.log_attempt(&format!("writing-{kind}"), corrected, text, score);
        self.store.profile.today.scores.push(score);
        self.store.profile.today.writings_done += 1;
        if self.store.profile.today.writings_done >= WRITING_GOAL {
            self.complete_step("writing");
        }
        self.store.save_profile();
    }
}

use chrono::Datelike;
