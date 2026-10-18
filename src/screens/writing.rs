//! ✏ Writing (20% of the daily plan): dictation of learned sentences, Korean → English
//! translation, and a short diary that Claude corrects — the corrected text is then read aloud,
//! so writing feeds back into speaking.

use crate::app::App;
use crate::claude::{TranslationCheck, WritingFeedback};
use crate::models::*;
use crate::scoring::{self, ScoreResult};
use crate::widgets::*;
use egui::{RichText, Stroke, Ui, Vec2};

/// Exercises in one daily writing set (dictation + translation; the diary is a bonus).
pub const WRITING_GOAL: u32 = 6;

/// Easy diary sentences (elementary-school level) shown as examples to A2–B1 learners.
pub const DIARY_EXAMPLES: &[(&str, &str)] = &[
    ("I got up at seven o'clock this morning.", "나는 오늘 아침 7시에 일어났다."),
    ("I washed my face and brushed my teeth.", "세수를 하고 이를 닦았다."),
    ("I had toast and milk for breakfast.", "아침으로 토스트와 우유를 먹었다."),
    ("The weather was sunny and warm.", "날씨가 맑고 따뜻했다."),
    ("It rained all day, so I stayed home.", "하루 종일 비가 와서 집에 있었다."),
    ("I went to school with my friend.", "친구와 함께 학교에 갔다."),
    ("We had a math test today.", "오늘 수학 시험을 봤다."),
    ("The test was easier than I thought.", "시험은 생각보다 쉬웠다."),
    ("I ate lunch with my classmates.", "반 친구들과 점심을 먹었다."),
    ("My favorite food is fried chicken.", "내가 제일 좋아하는 음식은 치킨이다."),
    ("After school, I played soccer in the park.", "방과 후에 공원에서 축구를 했다."),
    ("I was very tired, but I was happy.", "아주 피곤했지만 행복했다."),
    ("I helped my mom clean the kitchen.", "엄마를 도와 부엌을 청소했다."),
    ("My dad cooked dinner for us.", "아빠가 우리에게 저녁을 해 주셨다."),
    ("We ate spaghetti together.", "우리는 함께 스파게티를 먹었다."),
    ("I did my homework before dinner.", "저녁 먹기 전에 숙제를 했다."),
    ("I read a book about animals.", "동물에 관한 책을 읽었다."),
    ("I learned that dolphins are very smart.", "돌고래가 아주 똑똑하다는 것을 알게 되었다."),
    ("My little brother and I watched a movie.", "남동생과 영화를 봤다."),
    ("The movie was funny and exciting.", "영화는 재미있고 신났다."),
    ("I called my grandma on the phone.", "할머니께 전화를 드렸다."),
    ("She was glad to hear my voice.", "할머니는 내 목소리를 듣고 기뻐하셨다."),
    ("I went shopping with my sister.", "언니(누나)와 쇼핑을 갔다."),
    ("I bought a new pencil case.", "새 필통을 샀다."),
    ("I took my dog for a walk.", "강아지를 산책시켰다."),
    ("He ran around and wagged his tail.", "강아지는 뛰어다니며 꼬리를 흔들었다."),
    ("I rode my bike along the river.", "강을 따라 자전거를 탔다."),
    ("I saw many beautiful flowers.", "예쁜 꽃을 많이 봤다."),
    ("I felt a little sad today.", "오늘은 조금 슬펐다."),
    ("My best friend moved to another city.", "제일 친한 친구가 다른 도시로 이사를 갔다."),
    ("I will write a letter to her.", "그 친구에게 편지를 쓸 것이다."),
    ("Today was my birthday!", "오늘은 내 생일이었다!"),
    ("My family gave me a big cake.", "가족이 큰 케이크를 주었다."),
    ("I made a wish and blew out the candles.", "소원을 빌고 촛불을 껐다."),
    ("I practiced the piano for thirty minutes.", "피아노를 30분 동안 연습했다."),
    ("I want to play better next time.", "다음에는 더 잘 치고 싶다."),
    ("I took a warm bath before bed.", "자기 전에 따뜻한 물로 목욕했다."),
    ("I went to bed at ten o'clock.", "10시에 잠자리에 들었다."),
    ("Tomorrow I will get up early.", "내일은 일찍 일어날 것이다."),
    ("It was a good day.", "좋은 하루였다."),
];

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
    /// 일기장: the month shown (its first day) and the day opened.
    pub journal_month: Option<chrono::NaiveDate>,
    pub journal_day: Option<chrono::NaiveDate>,
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
            for (i, label) in ["🎧 받아쓰기", "🔁 한→영 작문", "📝 영어 일기", "📅 일기장"].iter().enumerate() {
                if ui.selectable_label(self.writing.mode == i, *label).clicked() && self.writing.mode != i {
                    self.writing.mode = i;
                    // Today's diary, if already saved, comes back to keep writing.
                    self.writing.input = if i == 2 { self.diary_on(today()).map(|w| w.text.clone()).unwrap_or_default() } else { String::new() };
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
            2 => self.diary_ui(ui),
            _ => self.journal_ui(ui),
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
                    ui.label(RichText::new(format!("힌트: {}", ko)).color(muted()).small());
                });
            } else {
                ui.label(RichText::new("영어로 써 보세요.").color(muted()));
                crate::grammar::ko_label(ui, &ko, 22.0, gloss());
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
                    ui.label(RichText::new(format!("{:.0}점", r.score)).size(22.0).strong().color(if r.score >= 80.0 { GOOD } else { warn() }));
                    ui.label(RichText::new(if dictation { "정답과 비교 (초록 = 맞음, 빨강 = 다름)" } else { "모범 답안과 비교" }).color(muted()));
                });
                word_feedback(ui, r, 19.0);
                if st.translation.running() {
                    spinner_label(ui, "Claude가 다른 표현도 맞는지 확인 중…");
                }
                if let Some(t) = &st.translation_result {
                    match t {
                        Ok(t) => {
                            ui.label(RichText::new(format!("{} {}", if t.ok { "✅" } else { "💡" }, t.feedback_ko)).color(if t.ok { GOOD } else { warn() }));
                            if !t.better.is_empty() && t.better != en {
                                ui.label(RichText::new(format!("더 자연스럽게: {}", t.better)).color(gloss()));
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
        let saved_today = self.diary_on(today()).is_some();
        let mut save_diary = false;
        let st = &mut self.writing;
        card(ui, |ui| {
            ui.label(RichText::new(prompt).size(19.0).strong());
            crate::grammar::ko_label(ui, prompt_ko, 16.0, muted());
            ui.add(egui::TextEdit::multiline(&mut st.input).desired_rows(6).desired_width(f32::INFINITY).font(egui::FontId::proportional(18.0)).hint_text("영어로 3~5문장 써 보세요. 틀려도 괜찮아요!"));
            ui.horizontal_wrapped(|ui| {
                // Saved by date (writing.json) with or without Claude; see 📅 일기장.
                if ui.add_enabled(!st.input.trim().is_empty(), egui::Button::new(RichText::new("💾 일기 저장").strong())).clicked() {
                    save_diary = true;
                }
                match &claude {
                    None => {
                        ui.label(RichText::new("영어 일기 교정은 Claude API 키가 필요합니다 (설정).").color(warn()));
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
            if saved_today {
                ui.label(RichText::new("✅ 오늘 일기가 저장되어 있어요 (📅 일기장에서 날짜별로 다시 볼 수 있어요)").color(GOOD).small());
            }
        });
        if save_diary {
            self.save_diary_text(prompt);
        }
        // A2–B1: easy model sentences to borrow from while writing.
        if (2.0..5.0).contains(&level) {
            egui::CollapsingHeader::new(RichText::new(format!("📒 예제 일기 문장 {}개 보기 (초등학교 수준)", DIARY_EXAMPLES.len())).strong()).id_salt("diary_examples").show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (en, ko) in DIARY_EXAMPLES {
                        ui.horizontal_wrapped(|ui| {
                            speaker_button(ui, &mut self.voice, en);
                            ui.label(RichText::new(*en).size(17.0));
                            crate::grammar::ko_label(ui, ko, 15.0, muted());
                        });
                    }
                });
            });
        }
        let st = &mut self.writing;
        if let Some(r) = &st.diary_result {
            card(ui, |ui| match r {
                Ok(f) => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}점", f.score)).size(24.0).strong().color(if f.score >= 80 { GOOD } else { warn() }));
                        ui.label(&f.feedback_ko);
                    });
                    for c in &f.corrections {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&c.original).strikethrough().color(BAD));
                            ui.label("→");
                            ui.label(RichText::new(&c.corrected).color(GOOD).strong());
                            ui.label(RichText::new(&c.explanation_ko).color(muted()).small());
                        });
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("원어민 버전").strong());
                    ui.label(RichText::new(&f.natural_version).size(18.0));
                    crate::grammar::ko_label(ui, &f.natural_version_ko, 16.0, gloss());
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

    /// Today's diary prompt, rotating. Fixed for the whole day (only earlier days' diaries
    /// count), so saving and correcting today's diary keeps one entry.
    fn diary_prompt(&self) -> (&'static str, &'static str) {
        let earlier = self.store.writings.iter().filter(|w| w.kind == "diary" && w.date.is_some_and(|d| d < today())).count();
        DIARY_PROMPTS[(today().ordinal() as usize + earlier) % DIARY_PROMPTS.len()]
    }

    /// Opens 쓰기 › 📅 일기장 at a day's diary.
    pub fn open_journal(&mut self, date: chrono::NaiveDate) {
        self.screen = crate::app::Screen::Writing;
        self.writing.mode = 3;
        self.writing.journal_month = date.with_day(1);
        self.writing.journal_day = Some(date);
    }

    /// The diary written on a day (one per day).
    pub fn diary_on(&self, date: chrono::NaiveDate) -> Option<&WritingRecord> {
        self.store.writings.iter().rev().find(|w| w.kind == "diary" && w.date == Some(date))
    }

    /// Saves today's diary as written (no correction needed), replacing an earlier save today
    /// but keeping a correction already made for the same text.
    fn save_diary_text(&mut self, prompt: &str) {
        let text = self.writing.input.trim().to_string();
        let (corrected, score) = self.diary_on(today()).filter(|w| w.text == text).map(|w| (w.corrected.clone(), w.score)).unwrap_or_default();
        self.store.writings.retain(|w| !(w.kind == "diary" && w.date == Some(today())));
        self.store.writings.push(WritingRecord { date: Some(today()), kind: "diary".into(), prompt: prompt.into(), text, corrected, score });
        self.store.save_writings();
        crate::applog!("writing: 일기 저장 ({})", today());
        self.toast("💾 오늘 일기를 저장했어요");
    }

    /// 📅 일기장: a month calendar; days with a diary are marked, click one to read it again.
    fn journal_ui(&mut self, ui: &mut Ui) {
        let first_of = |d: chrono::NaiveDate| d.with_day(1).unwrap_or(d);
        let month = self.writing.journal_month.unwrap_or_else(|| first_of(today()));
        let next_month = first_of(month + chrono::Duration::days(32));
        let prev_month = first_of(month - chrono::Duration::days(1));
        let total = self.store.writings.iter().filter(|w| w.kind == "diary").count();
        let mut go = None;
        let mut picked = None;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("◀").clicked() {
                    go = Some(prev_month);
                }
                ui.label(RichText::new(format!("{}년 {}월", month.year(), month.month())).size(20.0).strong());
                if ui.button("▶").clicked() {
                    go = Some(next_month);
                }
                if ui.button("오늘").clicked() {
                    go = Some(first_of(today()));
                    picked = Some(today());
                }
                ui.label(RichText::new(format!("지금까지 쓴 일기 {total}편 · 📝 표시된 날을 클릭하면 그날 일기를 다시 봐요")).color(muted()).small());
            });
            ui.add_space(6.0);
            let cell = Vec2::new(64.0, 44.0);
            egui::Grid::new("diary_calendar").spacing(Vec2::splat(4.0)).show(ui, |ui| {
                for (i, d) in ["일", "월", "화", "수", "목", "금", "토"].iter().enumerate() {
                    let c = match i {
                        0 => BAD,
                        6 => ACCENT,
                        _ => muted(),
                    };
                    ui.add_sized(Vec2::new(cell.x, 20.0), egui::Label::new(RichText::new(*d).color(c)));
                }
                ui.end_row();
                let lead = month.weekday().num_days_from_sunday() as i64;
                let days = (next_month - month).num_days();
                for slot in 0..(lead + days + 6) / 7 * 7 {
                    let n = slot - lead;
                    if n < 0 || n >= days {
                        ui.allocate_space(cell);
                    } else {
                        let date = month + chrono::Duration::days(n);
                        let has = self.diary_on(date).is_some();
                        let label = if has { format!("{}\n📝", n + 1) } else { (n + 1).to_string() };
                        let mut b = egui::Button::new(RichText::new(label).color(if has { text_strong() } else { muted() })).min_size(cell);
                        if has {
                            b = b.fill(ACCENT.gamma_multiply(0.35));
                        }
                        if self.writing.journal_day == Some(date) {
                            b = b.stroke(Stroke::new(2.0_f32, warn()));
                        } else if date == today() {
                            b = b.stroke(Stroke::new(1.0_f32, warn()));
                        }
                        let r = ui.add_enabled(has, b);
                        if r.clicked() {
                            picked = Some(date);
                        }
                    }
                    if slot % 7 == 6 {
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(m) = go {
            self.writing.journal_month = Some(m);
        }
        if let Some(d) = picked {
            self.writing.journal_day = Some(d);
        }
        let Some(day) = self.writing.journal_day else { return };
        let Some(w) = self.diary_on(day).cloned() else { return };
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("📝 {} ({}) 일기", day.format("%Y년 %m월 %d일"), ["일", "월", "화", "수", "목", "금", "토"][day.weekday().num_days_from_sunday() as usize])).size(18.0).strong());
            if !w.prompt.is_empty() {
                ui.label(RichText::new(format!("주제: {}", w.prompt)).color(muted()));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("내가 쓴 일기").strong());
            ui.label(RichText::new(&w.text).size(17.0));
            if ui.button("🔊 내 일기 듣기").clicked() {
                self.voice.say(&w.text);
            }
            if !w.corrected.is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✨ Claude 교정본").strong());
                    if w.score > 0.0 {
                        ui.label(RichText::new(format!("{:.0}점", w.score)).color(if w.score >= 80.0 { GOOD } else { warn() }));
                    }
                });
                ui.label(RichText::new(&w.corrected).size(17.0).color(GOOD));
                if ui.button("🔊 교정본 듣기").clicked() {
                    self.voice.say(&w.corrected);
                }
            }
        });
    }

    fn finish_writing(&mut self, kind: &str, prompt: &str, text: &str, corrected: &str, score: f32) {
        if kind == "diary" {
            // One diary per day: the corrected one replaces a plain save from today.
            self.store.writings.retain(|w| !(w.kind == "diary" && w.date == Some(today())));
        }
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
