//! Application state, navigation, dashboard, onboarding/level test and settings.
//! The practice screens live in `screens/`.

use crate::claude::{Claude, LessonRequest};
use crate::content;
use crate::curriculum::{self, TOTAL_DAYS};
use crate::language::{self, LanguagePack};
use crate::models::*;
use crate::screens::{translator::TranslatorState, library::LibraryState, writing::WritingState, audio_check::AudioCheckState, feedback::FeedbackState, speed::SpeedState, listening::ListeningState, practice::DrillQueue, speaking::{RoleplayState, SelfExprState}, homework::HomeworkState, test::LevelTest};
use crate::srs;
use crate::storage::Store;
use crate::stt::{self, Stt, SttStatus};
use crate::voice::{Speaker, Voice};
use crate::widgets::*;
use chrono::Duration as CDuration;
use egui::{Color32, RichText, Vec2};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Onboarding,
    Home,
    Homework,
    Review,
    Words,
    Reading,
    Speed,
    Listening,
    SelfExpression,
    Business,
    Library,
    Writing,
    Translator,
    Vocabulary,
    Feedback,
    History,
    Progress,
    LevelTest,
    Settings,
}

/// The daily routine, in order. Keys are stored in `profile.today.done`.
/// Width of the main content column; the window is sized to fit it next to the sidebar.
pub const CONTENT_WIDTH: f32 = 900.0;
/// Sidebar width.
pub const SIDEBAR_WIDTH: f32 = 232.0;

/// Narrower than this (a phone, or a tablet held upright) the menu folds into a ☰ top bar and
/// the page takes the whole width.
pub const COMPACT_WIDTH: f32 = 760.0;

pub const STEPS: &[(&str, &str, Screen)] = &[
    ("homework", "📝 숙제 확인", Screen::Homework),
    ("review", "🔁 복습 (배운 단어·문장)", Screen::Review),
    ("words", "📚 새 단어", Screen::Words),
    ("reading", "📖 문장 소리내어 읽기", Screen::Reading),
    ("speed", "⚡ 스피드 리딩·말하기", Screen::Speed),
    ("listening", "🎧 리스닝 랩 · 듣고 따라 말하기", Screen::Listening),
    ("speaking", "💬 자기 표현 말하기", Screen::SelfExpression),
    ("business", "💼 비즈니스 회화", Screen::Business),
    ("library", "📘 리딩 (동화·소설)", Screen::Library),
    ("writing", "✏ 쓰기 (받아쓰기·작문)", Screen::Writing),
];

/// Activities shown on the 진척율 card (Order.md: 새단어, 문장 소리내어 읽기, 스피드 리딩·말하기,
/// 자기표현 말하기, 비즈니스 회화, 리딩, 쓰기).
pub const PROGRESS_ACTIVITIES: [&str; 7] = ["words", "reading", "speed", "speaking", "business", "library", "writing"];

/// Daily time split by skill (Order: 말하기 40 · 듣기 30 · 읽기 15 · 쓰기 15).
pub const SKILL_WEIGHTS: [(&str, f32); 4] = [("🎤 말하기", 0.4), ("🎧 듣기", 0.3), ("📘 읽기", 0.15), ("✏ 쓰기", 0.15)];

/// Which skill a screen trains (index into `SKILL_WEIGHTS`; None = not study time).
pub fn skill_of(s: Screen) -> Option<usize> {
    match s {
        Screen::Listening => Some(1),
        Screen::Library => Some(2),
        Screen::Writing => Some(3),
        Screen::Settings | Screen::Onboarding | Screen::Progress | Screen::History | Screen::Vocabulary => None,
        _ => Some(0),
    }
}

pub struct App {
    pub store: Store,
    pub lang: &'static LanguagePack,
    pub voice: Voice,
    pub screen: Screen,
    pub lesson: Option<Lesson>,
    pub lesson_job: Pending<Lesson>,
    pub lesson_error: Option<String>,
    pub queue: Option<DrillQueue>,
    pub homework: HomeworkState,
    /// 시제 3문장 being written by Claude, and the day it was last asked for (not retried
    /// the same day after a failure).
    pub tense_job: Pending<Vec<crate::tenses::TenseSet>>,
    pub tense_tried: Option<chrono::NaiveDate>,
    /// The day the course days the app wasn't opened on were last given their lists.
    pub days_filled: Option<chrono::NaiveDate>,
    pub sentence_tense_job: Pending<Vec<crate::tenses::TenseSet>>,
    /// 받아쓰기 picture being looked up (sentence, result), sentences already tried this run,
    /// and the decoded pictures by file name.
    pub scene_job: Pending<(String, crate::scenes::SceneImage)>,
    pub scene_tried: std::collections::HashSet<String>,
    pub scene_tex: std::collections::HashMap<String, Option<egui::TextureHandle>>,
    pub sentence_tense_tried: Option<chrono::NaiveDate>,
    /// 오늘의 문장 만들기 in progress: (word, mission).
    pub word_mission: Option<(String, crate::screens::mission::Mission)>,
    /// A round of 히라가나·가타카나 cards (N5, top of 새 단어).
    pub kana_quiz: Option<crate::screens::kana::KanaQuiz>,
    /// Phone-sized screens: the menu is shown instead of the page (☰ in the top bar).
    pub nav_open: bool,
    /// Phone layout: a page that came out wider than the screen (screen, width) — logged once,
    /// and checked by the phone layout test.
    pub overflow: Option<(Screen, f32)>,
    pub selfx: SelfExprState,
    pub roleplay: RoleplayState,
    pub listening: ListeningState,
    pub wordle: crate::screens::wordle::WordleState,
    pub speed: SpeedState,
    pub feedback: FeedbackState,
    pub audio_check: AudioCheckState,
    pub library: LibraryState,
    pub writing: WritingState,
    pub translator: TranslatorState,
    pub coach: crate::screens::coach::CoachState,
    pub test: Option<LevelTest>,
    pub toast: Option<(String, Instant)>,
    pub notifier: crate::screens::notify::Notifier,
    /// The in-window web page and what a screen asked of it this frame.
    pub web: crate::web::WebPane,
    pub web_req: Option<crate::web::WebRequest>,
    pub vocab_filter: String,
    pub vocab_mode: usize,
    pub show_key: bool,
    edit_name: bool,
    edit_key: bool,
    prefetched: Option<chrono::NaiveDate>,
    pub api_url: String,
    /// The Claude API answered "can't be used" (no credit / key / network): speaking screens
    /// use the built-in offline scripts until an answer comes back.
    pub api_down: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_activity: Instant,
    last_tick: Instant,
    last_save: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let store = Store::open().expect("데이터 폴더를 만들 수 없습니다");
        let lang = language::by_code(&store.settings.target_language);
        let speaker = Speaker::new(lang, &store.settings.tts_voice_id);
        let stt = Stt::new(lang.code);
        let mut store = store;
        let mut model_file = stt::model_path(&store.models_dir(), &store.settings.whisper_model);
        // A multilingual model the English app already downloaded is linked, not downloaded again.
        if let Some(other) = crate::storage::english_app_models_dir().filter(|_| std::env::var("SPEAKUP_DATA_DIR").is_err()) {
            for (id, _) in lang.whisper_models {
                let (src, dst) = (stt::model_path(&other, id), stt::model_path(&store.models_dir(), id));
                if src.exists() && !dst.exists() && (std::fs::hard_link(&src, &dst).is_ok() || std::fs::copy(&src, &dst).is_ok()) {
                    crate::applog!("stt: 영어판 앱의 {id} 모델을 가져옴");
                }
            }
        }
        if !model_file.exists() {
            // A model downloaded earlier is reused even if a different one is selected.
            if let Some((id, path)) = lang.whisper_models.iter().map(|(id, _)| (id, stt::model_path(&store.models_dir(), id))).find(|(_, p)| p.exists()) {
                store.settings.whisper_model = id.to_string();
                store.save_settings();
                model_file = path;
            }
        }
        if model_file.exists() {
            stt.load(model_file);
        }
        let player = crate::audio::Player::new(store.settings.output_device.clone());
        let mut speaker = speaker;
        if crate::voicevox::supported() {
            let vv = crate::voicevox::VoiceVox::new(player.handle());
            if crate::voicevox::is_installed(&store.models_dir()) {
                vv.load(&store.models_dir());
            } else if store.settings.neural_tts && !cfg!(test) {
                // The natural voice matters most: fetch it once in the background on first run.
                vv.download(store.models_dir());
            }
            speaker.neural = Some(vv);
        }
        speaker.use_neural = store.settings.neural_tts;
        speaker.neural_voice = store.settings.voicevox_voice;
        // Korean speech (translator): the same multilingual model as Japanese when the files are
        // the same, so it's in memory once.
        let stt_ko = if store.settings.korean_model == store.settings.whisper_model { stt.with_language("ko") } else { Stt::new("ko") };
        let ko_model = stt::model_path(&store.models_dir(), &store.settings.korean_model);
        if ko_model.exists() && store.settings.korean_model != store.settings.whisper_model {
            stt_ko.load(ko_model);
        }
        let voice = Voice {
            speaker,
            recorder: crate::audio::Recorder::new(store.settings.input_device.clone()),
            player,
            stt,
            stt_ko,
            base_rate: store.settings.tts_rate,
            last_recording: Vec::new(),
        };
        Self::new_with(cc, store, voice)
    }

    /// Builds the app from an opened store and a voice bundle (tests pass simulated audio).
    pub fn new_with(cc: &eframe::CreationContext<'_>, mut store: Store, voice: Voice) -> Self {
        set_light(store.settings.light_theme);
        crate::theme::apply(&cc.egui_ctx);
        // The daily goal default went from 30 to 60 minutes; move learners still on the old default.
        if !store.settings.goal_60_applied {
            if store.settings.daily_goal_minutes == 30 {
                store.settings.daily_goal_minutes = 60;
            }
            store.settings.goal_60_applied = true;
            store.save_settings();
        }
        crate::grammar::load(&cc.egui_ctx, &store.dir);
        let lang = language::by_code(&store.settings.target_language);
        let screen = if store.profile.placement_done { Screen::Home } else { Screen::Onboarding };
        let mut app = Self {
            lesson: store.load_lesson(today()),
            store,
            lang,
            voice,
            screen,
            lesson_job: Pending::default(),
            lesson_error: None,
            queue: None,
            homework: HomeworkState::default(),
            word_mission: None,
            kana_quiz: None,
            nav_open: false,
            overflow: None,
            tense_job: Pending::default(),
            tense_tried: None,
            days_filled: None,
            sentence_tense_job: Pending::default(),
            scene_job: Pending::default(),
            scene_tried: Default::default(),
            scene_tex: Default::default(),
            sentence_tense_tried: None,
            selfx: SelfExprState::default(),
            roleplay: RoleplayState::default(),
            listening: ListeningState::default(),
            wordle: Default::default(),
            speed: SpeedState::default(),
            feedback: FeedbackState::default(),
            audio_check: AudioCheckState::default(),
            library: LibraryState::default(),
            writing: WritingState::default(),
            translator: TranslatorState::default(),
            coach: Default::default(),
            test: None,
            toast: None,
            notifier: Default::default(),
            web: Default::default(),
            web_req: None,
            vocab_filter: String::new(),
            vocab_mode: 0,
            show_key: false,
            edit_name: false,
            edit_key: false,
            prefetched: None,
            api_url: crate::claude::API_URL.to_string(),
            api_down: Default::default(),
            last_activity: Instant::now(),
            last_tick: Instant::now(),
            last_save: Instant::now(),
        };
        app.roll_day();
        app.assign_homework();
        // Ask for the microphone up front so the first speaking drill doesn't record silence.
        #[cfg(not(test))]
        app.audio_check.request_if_needed();
        app
    }

    /// Speaking screens use the built-in scripts: no key, or the API said it can't be used.
    pub fn talk_offline(&self) -> bool {
        self.store.settings.effective_api_key().is_none() || self.api_down.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn claude(&self) -> Option<Claude> {
        self.store.settings.effective_api_key().map(|k| Claude { url: self.api_url.clone(), api_key: k, model: self.store.settings.model.clone(), lang: self.lang, down: self.api_down.clone() })
    }

    /// Korean support fades over the program (see `curriculum::support_stage`).
    fn apply_support_stage(&mut self, ctx: &egui::Context) {
        let stage = curriculum::support_stage(curriculum::day_number(&self.store.profile));
        crate::widgets::set_support_stage(ctx, stage);
        crate::widgets::set_furigana(ctx, self.store.settings.furigana);
        if stage >= 1 && !self.store.settings.ko_colors_faded {
            self.store.settings.ko_colors_faded = true;
            if self.store.settings.ko_role_colors {
                self.store.settings.ko_role_colors = false;
                self.toast("3개월 차부터는 직독직해와 한글 색상을 기본으로 꺼요. 필요할 때만 켜세요 (설정에서 다시 켤 수 있어요).");
            }
            self.store.save_settings();
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn pass_score(&self) -> f32 {
        self.store.settings.pass_score
    }

    // ------------------------------------------------------------ daily bookkeeping

    /// Starts a fresh day's progress record when the date changes.
    fn roll_day(&mut self) {
        let t = today();
        if self.store.profile.today.date != Some(t) {
            self.update_history(); // close out the previous day
            self.store.profile.today = DayProgress { date: Some(t), ..Default::default() };
            self.store.save_profile();
            if self.lesson.as_ref().and_then(|l| l.date) != Some(t) {
                self.lesson = self.store.load_lesson(t);
            }
        }
    }

    /// Called whenever the learner finishes a step: marks it done and maintains the streak.
    pub fn complete_step(&mut self, key: &str) {
        let t = today();
        let p = &mut self.store.profile;
        if p.last_study_date != Some(t) {
            p.streak = if p.last_study_date == Some(t - CDuration::days(1)) { p.streak + 1 } else { 1 };
            p.last_study_date = Some(t);
            p.days_studied += 1;
        }
        if !p.today.done.iter().any(|d| d == key) {
            p.today.done.push(key.to_string());
            let label = STEPS.iter().find(|s| s.0 == key).map(|s| s.1).unwrap_or(key);
            // Point on to the next step in today's order.
            let msg = match self.next_step() {
                Some((n, next, _)) => format!("✅ 완료: {label} · 다음은 {n}. {next}"),
                None => format!("✅ 완료: {label} · 🎉 오늘 학습을 모두 마쳤어요!"),
            };
            self.toast(msg);
        }
        self.store.save_profile();
        self.update_history();
    }

    /// Writes the current day's record to history.json (called whenever progress changes).
    pub fn update_history(&mut self) {
        let p = &self.store.profile;
        let Some(date) = p.today.date else { return };
        let lesson = self.lesson.as_ref().filter(|l| l.date == Some(date));
        let added: Vec<&StudyItem> = self.store.items.iter().filter(|i| i.added == date).collect();
        let scores = &p.today.scores;
        let rec = DayRecord {
            date: Some(date),
            program_day: ((date - p.start_date).num_days() + 1).max(1),
            steps: p.today.done.clone(),
            minutes: (p.today.study_seconds / 60) as u32,
            attempts: scores.len() as u32,
            avg_score: if scores.is_empty() { 0.0 } else { scores.iter().sum::<f32>() / scores.len() as f32 },
            level: p.level,
            theme: curriculum::week(curriculum::week_number(p)).theme_ko.to_string(),
            new_words: added.iter().filter(|i| i.is_vocab_word()).map(|i| i.text.clone()).collect(),
            new_sentences: added.iter().filter(|i| i.kind == ItemKind::Sentence).count() as u32,
            reviewed: self.store.items.iter().filter(|i| i.added < date && i.attempts > 0 && i.due > date).count() as u32,
            question: lesson.map(|l| l.self_expression.question.clone()).unwrap_or_default(),
            scenario: lesson.map(|l| l.business.title.clone()).unwrap_or_default(),
            best_wpm: p.speed_history.iter().filter(|r| r.date == date && r.kind == "reading").map(|r| r.wpm).fold(0.0, f32::max),
            speaking_minutes: (p.today.speaking_seconds / 60) as u32,
            listening_minutes: (p.today.listening_seconds / 60) as u32,
            reading_minutes: (p.today.reading_seconds / 60) as u32,
            writing_minutes: (p.today.writing_seconds / 60) as u32,
            reading: self
                .store
                .library
                .iter()
                .filter(|(_, b)| b.last_read == Some(date))
                .filter_map(|(id, b)| crate::library::book(id).map(|bk| format!("{} ({}/{}쪽)", bk.title_ko, (b.page + 1).min(b.pages), b.pages)))
                .collect(),
            writings: self.store.writings.iter().filter(|w| w.date == Some(date)).count() as u32,
        };
        if rec.attempts > 0 || !rec.steps.is_empty() || rec.minutes > 0 {
            self.store.upsert_day(rec);
        }
    }

    pub fn record_score(&mut self, category: &str, target: &str, transcript: &str, score: f32) {
        self.store.profile.today.scores.push(score);
        self.store.log_attempt(category, target, transcript, score);
    }

    /// Records a drill attempt: score, log line, and any words that weren't recognized.
    pub fn record_attempt(&mut self, category: &str, target: &str, a: &Attempt) {
        self.record_score(category, target, &a.transcript, a.score);
        // 새 단어 entries (today's words/chunks/expression, or learned words) keep their own count.
        if category == "reading" {
            // 문장 읽기: today's idiom sentences and longer sentences keep a count too (list colours).
            let idioms = self.store.profile.today.idioms.clone().unwrap_or_default();
            let in_today = idioms.iter().filter_map(|n| crate::idioms::by_no(*n)).any(|i| i.ja == target)
                || self.lesson.as_ref().is_some_and(|l| l.sentences.iter().any(|s| s.text == target));
            if in_today {
                let pass = self.pass_score();
                self.store.record_word_practice(target, a.score >= pass);
            }
        }
        if category == "business" {
            // Key phrases / corrected sentences said in the 회화 phrase drills.
            let pass = self.pass_score();
            self.store.record_word_practice(target, a.score >= pass);
        }
        if category == "vocab" {
            let lower = target.to_lowercase();
            let in_set = self.store.profile.today.word_set.iter().any(|w| w.words.iter().chain(&w.phrases).chain(&w.expressions).any(|x| x.word.to_lowercase() == lower));
            let learned = self.store.items.iter().any(|i| i.kind == ItemKind::Word && i.text.to_lowercase() == lower);
            if in_set || learned {
                let pass = self.pass_score();
                self.store.record_word_practice(target, a.score >= pass);
            }
        }
        // Only count mispronunciations on attempts that were mostly right; a mostly-wrong
        // attempt usually means the learner said something else entirely.
        if a.score >= 40.0 {
            self.store.add_mistakes(&a.missed);
        }
    }

    /// Steps that apply today (homework only if there is homework from an earlier day,
    /// review only if something is due).
    pub fn todays_steps(&self) -> Vec<(&'static str, &'static str, Screen)> {
        let has_homework = self.store.homework.iter().any(|h| h.completed.is_none() && h.assigned < today());
        let done = &self.store.profile.today.done;
        let has_review = srs::due_count(&self.store.items) > 0 || done.iter().any(|d| d == "review");
        STEPS
            .iter()
            .copied()
            .filter(|(k, _, _)| match *k {
                "homework" => has_homework || done.iter().any(|d| d == "homework"),
                "review" => has_review,
                _ => true,
            })
            .collect()
    }

    /// Today's study order for the side menu tags: (screen, number in today's order, state).
    /// The first unfinished step is the one to do now ("시작" on the very first day).
    pub fn step_tags(&self) -> Vec<(Screen, usize, StepTag)> {
        let first_run = self.store.profile.days_studied == 0 && self.store.profile.today.done.is_empty();
        let mut next_given = false;
        self.todays_steps()
            .iter()
            .enumerate()
            .map(|(i, (key, _, screen))| {
                let tag = if self.step_done(key) {
                    StepTag::Done
                } else if !next_given {
                    next_given = true;
                    StepTag::Next { first_run }
                } else {
                    StepTag::Todo
                };
                (*screen, i + 1, tag)
            })
            .collect()
    }

    /// The step to do now: (number, label, screen).
    pub fn next_step(&self) -> Option<(usize, &'static str, Screen)> {
        let steps = self.todays_steps();
        self.step_tags().iter().find(|t| matches!(t.2, StepTag::Next { .. })).map(|t| (t.1, steps[t.1 - 1].1, t.0))
    }

    /// Today's 새 단어 set: words from the built-in 6,000-word database for the learner's level,
    /// plus the day's chunks and expression. Picked once a day and kept in the profile.
    pub fn word_set(&mut self) -> WordSet {
        self.fill_missed_days();
        if let Some(ws) = self.store.profile.today.word_set.clone() {
            self.store.record_word_day(today(), &ws);
            return ws;
        }
        let day_no = curriculum::day_number(&self.store.profile);
        let ws = self.pick_word_set(day_no, today());
        crate::applog!("words: 오늘의 단어 {}개 (레벨 {:.1}) · {:?}", ws.words.len(), self.store.profile.level, ws.words.iter().map(|w| &w.word).collect::<Vec<_>>());
        self.store.profile.today.word_set = Some(ws.clone());
        self.store.save_profile();
        self.store.record_word_day(today(), &ws);
        ws
    }

    /// A day's 새 단어 set for 일차 `day_no`. New words only: learned ones and those on another
    /// day's list before `until` (left unfinished there, they are in 지난 날 못 끝낸 새 단어) are skipped.
    fn pick_word_set(&self, day_no: i64, until: chrono::NaiveDate) -> WordSet {
        let n = if self.store.settings.new_words_per_day > 0 { self.store.settings.new_words_per_day as usize } else { crate::wordbank::NEW_PER_DAY };
        let items = &self.store.items;
        let earlier: std::collections::HashSet<String> = self
            .store
            .word_days
            .range(..until)
            .flat_map(|(_, s)| s.words.iter().chain(&s.phrases).chain(&s.expressions).map(|w| w.word.to_lowercase()))
            .collect();
        let knows = |t: &str| earlier.contains(&t.to_lowercase()) || items.iter().any(|i| i.text.eq_ignore_ascii_case(t));
        let lw = |e: crate::expressions::Expr| LessonWord { word: e.0.into(), reading: String::new(), meaning_ko: e.1.into(), example: e.2.into(), example_ko: e.3.into() };
        WordSet {
            words: crate::wordbank::pick(self.store.profile.level, n, knows).iter().map(|w| w.lesson_word()).collect(),
            phrases: crate::expressions::pick_chunks(content::CHUNKS_PER_DAY, knows).into_iter().map(lw).collect(),
            expressions: crate::expressions::pick_for_today(day_no, knows).into_iter().map(lw).collect(),
        }
    }

    /// Days of the course the app wasn't opened on still get their 새 단어 and 회화 문장, picked
    /// now, so every 일차 is in 📚 날짜별 전체 and its items in 지난 날 못 끝낸.
    fn fill_missed_days(&mut self) {
        let t = today();
        if !self.store.profile.placement_done || self.days_filled == Some(t) {
            return;
        }
        self.days_filled = Some(t);
        let start = self.store.profile.start_date;
        let mut d = start;
        while d < t && (d - start).num_days() < curriculum::TOTAL_DAYS {
            let day_no = (d - start).num_days() + 1;
            if !self.store.word_days.contains_key(&d) {
                // Skips every listed day's words, today's too, so no word is on two days.
                let ws = self.pick_word_set(day_no, chrono::NaiveDate::MAX);
                crate::applog!("words: {day_no}일차({d}) 단어 채움 {}개 · 앱을 열지 않은 날", ws.words.len());
                self.store.record_word_day(d, &ws);
            }
            if !self.store.idiom_days.contains_key(&d) {
                let nos = self.pick_idioms(chrono::NaiveDate::MAX);
                crate::applog!("reading: {day_no}일차({d}) 회화 문장 채움 {}개 · 앱을 열지 않은 날", nos.len());
                self.store.record_idiom_day(d, &nos);
            }
            d += CDuration::days(1);
        }
    }

    /// Asks Claude once a day for 시제 3문장 of today's words that have none yet, and keeps
    /// what comes back.
    pub fn ensure_tenses(&mut self) {
        if let Some(r) = self.tense_job.poll() {
            match r {
                Ok(sets) => {
                    crate::applog!("words: 시제 3문장 {}개 받음", sets.len());
                    self.store.put_tenses(&sets);
                }
                Err(e) => crate::applog!("words: 시제 3문장 생성 실패 · {e:#}"),
            }
        }
        if self.tense_job.running() || self.tense_tried == Some(today()) || self.talk_offline() {
            return;
        }
        let Some(c) = self.claude() else { return };
        let ws = self.word_set();
        let missing: Vec<(String, String)> = ws.phrases.iter().chain(&ws.words).chain(&ws.expressions).filter(|w| !self.store.tenses.contains_key(&w.word.to_lowercase())).map(|w| (w.word.clone(), w.meaning_ko.clone())).take(50).collect();
        self.tense_tried = Some(today());
        if missing.is_empty() {
            return;
        }
        let level = self.store.profile.level;
        self.tense_job.spawn(move || c.tense_sentences(&missing, level));
    }

    /// Asks Claude once a day for today's 문장 읽기 sentences in three tenses (those not asked
    /// before), and keeps what comes back.
    pub fn ensure_sentence_tenses(&mut self, sentences: &[(String, String)]) {
        if let Some(r) = self.sentence_tense_job.poll() {
            match r {
                Ok(sets) => {
                    crate::applog!("reading: 시제 문장 {}개 받음 (바꿀 수 있는 문장 {}개)", sets.len(), sets.iter().filter(|s| s.is_complete()).count());
                    self.store.put_sentence_tenses(&sets);
                }
                Err(e) => crate::applog!("reading: 시제 문장 생성 실패 · {e:#}"),
            }
        }
        if sentences.is_empty() || self.sentence_tense_job.running() || self.sentence_tense_tried == Some(today()) || self.talk_offline() {
            return;
        }
        let Some(c) = self.claude() else { return };
        let missing: Vec<(String, String)> = sentences.iter().filter(|(t, _)| !self.store.sentence_tenses.contains_key(&t.trim().to_lowercase())).take(50).cloned().collect();
        self.sentence_tense_tried = Some(today());
        if missing.is_empty() {
            return;
        }
        let level = self.store.profile.level;
        self.sentence_tense_job.spawn(move || c.sentence_tenses(&missing, level));
    }

    /// An entry's 시제 3문장: Claude's, else a frame for its part of speech.
    pub fn tense_for(&self, w: &LessonWord) -> crate::tenses::TenseSet {
        if let Some(t) = self.store.tenses.get(&w.word.to_lowercase()) {
            return t.clone();
        }
        // Chunks and expressions ("It depends on") have no part of speech: the quoted frame.
        let pos = if w.word.contains(' ') { String::new() } else { crate::dictionary::lookup(&w.word).map(|(b, _)| b.pos_ko.clone()).unwrap_or_default() };
        crate::tenses::template(&w.word, &pos, &w.meaning_ko)
    }

    /// Today's 5 focus words for 오늘의 문장 만들기 and for using while speaking: picked once a
    /// day from today's words, ones already passed first.
    pub fn focus_words(&mut self) -> Vec<LessonWord> {
        let ws = self.word_set();
        if self.store.profile.today.focus_words.is_empty() && !ws.words.is_empty() {
            let passed = |w: &LessonWord| self.store.items.iter().any(|i| i.text.eq_ignore_ascii_case(&w.word));
            let mut v: Vec<&LessonWord> = ws.words.iter().filter(|w| passed(w)).chain(ws.words.iter().filter(|w| !passed(w))).collect();
            v.truncate(5);
            self.store.profile.today.focus_words = v.iter().map(|w| w.word.clone()).collect();
            self.store.save_profile();
        }
        let focus = &self.store.profile.today.focus_words;
        focus.iter().filter_map(|f| ws.words.iter().find(|w| &w.word == f).cloned()).collect()
    }

    /// Counts today's focus words used in something said while speaking.
    pub fn note_words_used(&mut self, said: &str) {
        for w in self.focus_words() {
            if crate::dictionary::sentence_uses(said, &w.word) && !self.store.profile.today.words_used.contains(&w.word) {
                self.store.profile.today.words_used.push(w.word.clone());
                self.store.record_word_practice(&w.word, true);
                self.store.save_profile();
                self.toast(format!("👍 오늘 단어 '{}'을(를) 말하면서 썼어요!", w.word));
            }
        }
    }

    /// "💡 오늘 단어를 넣어 말해 보세요" chips (green once used).
    pub fn focus_words_hint(&mut self, ui: &mut egui::Ui) {
        let words = self.focus_words();
        if words.is_empty() {
            return;
        }
        let used = self.store.profile.today.words_used.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("💡 오늘 단어를 넣어 말해 보세요 ({}/{})", used.len().min(words.len()), words.len())).color(warn()));
            for w in &words {
                let on = used.contains(&w.word);
                chip(ui, &format!("{} {}", if on { "✓" } else { "💬" }, w.word), if on { GOOD } else { ACCENT });
            }
        });
    }

    /// Learned words for today's review in the 새 단어 menu (up to 50): due ones first, then weak
    /// ones, then the ones due soonest. Today's new words and words already reviewed today are left out.
    pub fn word_review_ids(&self) -> Vec<String> {
        let today_words: Vec<String> = self.store.profile.today.word_set.iter().flat_map(|w| w.words.iter().map(|x| x.word.to_lowercase())).collect();
        let done = &self.store.profile.today.words_reviewed;
        let t = today();
        let mut v: Vec<&StudyItem> = self
            .store
            .items
            .iter()
            .filter(|i| i.is_vocab_word() && !i.known && !today_words.contains(&i.text.to_lowercase()) && !done.contains(&i.id) && i.added < t)
            .collect();
        v.sort_by_key(|i| (i.due > t, !i.weak, i.due));
        v.into_iter().take(crate::wordbank::REVIEW_PER_DAY.saturating_sub(done.len())).map(|i| i.id.clone()).collect()
    }

    /// Today's idiom sentences for 문장 읽기: 30 from the built-in 1,300, picked once a day.
    pub fn idiom_set(&mut self) -> Vec<&'static crate::idioms::Idiom> {
        self.fill_missed_days();
        if self.store.profile.today.idioms.is_none() {
            let picked = self.pick_idioms(today());
            crate::applog!("reading: 오늘의 회화 문장 {}개 · {:?}…", picked.len(), picked.first());
            self.store.profile.today.idioms = Some(picked);
            self.store.save_profile();
        }
        let nos = self.store.profile.today.idioms.clone().unwrap_or_default();
        self.store.record_idiom_day(today(), &nos);
        nos.iter().filter_map(|n| crate::idioms::by_no(*n)).collect()
    }

    /// A day's 30 idiom sentences: new ones only (another day's before `until`, left unfinished,
    /// are in 지난 날 못 끝낸 문장).
    fn pick_idioms(&self, until: chrono::NaiveDate) -> Vec<u32> {
        let items = &self.store.items;
        let earlier: std::collections::HashSet<u32> = self.store.idiom_days.range(..until).flat_map(|(_, v)| v.iter().copied()).collect();
        crate::idioms::pick(crate::idioms::PER_DAY, |t| {
            items.iter().any(|i| i.text.eq_ignore_ascii_case(t)) || crate::idioms::all().iter().any(|i| i.ja == t && earlier.contains(&i.no))
        })
    }

    /// Makes today's lesson (sentences, writing, prefetch) use the same words as the 새 단어 menu.
    fn sync_lesson_words(&mut self) {
        if !self.store.profile.placement_done || !self.lesson.as_ref().is_some_and(|l| l.date == Some(today())) {
            return;
        }
        let ws = self.word_set();
        if let Some(l) = &mut self.lesson {
            if l.words != ws.words || l.phrases != ws.phrases || l.expressions != ws.expressions {
                l.words = ws.words;
                l.phrases = ws.phrases;
                l.expressions = ws.expressions;
                self.store.save_lesson(l);
            }
        }
    }

    /// Days (out of the 6-month program) on which a routine step was completed, today included.
    pub fn activity_days(&self, key: &str) -> u32 {
        let t = today();
        let past = self.store.history.iter().filter(|r| r.date.is_some_and(|d| d < t && d >= self.store.profile.start_date) && r.steps.iter().any(|s| s == key)).count();
        past as u32 + self.step_done(key) as u32
    }

    /// Item-based progress for 새 단어 (words learned of the 6,000-word database) and 문장 읽기
    /// (idiom sentences learned of 1,300): (learned, total, memorized, unit). Other steps: None.
    pub fn item_progress(&self, key: &str) -> Option<(usize, usize, usize, &'static str)> {
        let memorized = |text: &str| self.store.word_stat(text).passes >= WORD_DONE_PASSES;
        match key {
            "words" => {
                let learned: Vec<&StudyItem> = self.store.items.iter().filter(|i| i.is_vocab_word()).collect();
                let done = learned.iter().filter(|i| memorized(&i.text)).count();
                Some((learned.len(), crate::wordbank::all().len(), done, "단어"))
            }
            "reading" => {
                let learned: Vec<&StudyItem> = self.store.items.iter().filter(|i| i.category == "idiom").collect();
                let done = learned.iter().filter(|i| memorized(&i.text)).count();
                Some((learned.len(), crate::idioms::all().len(), done, "문장"))
            }
            _ => None,
        }
    }

    /// 진척율 card: progress of each daily activity over the 6-month program.
    fn activity_progress_card(&self, ui: &mut egui::Ui) {
        let colors = [crate::art::PINK, crate::art::CYAN, warn(), GOOD, Color32::from_rgb(190, 140, 255), Color32::from_rgb(110, 220, 200), Color32::from_rgb(255, 160, 64)];
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("📊 진척율").size(17.0).strong());
                ui.label(RichText::new(format!("새 단어·문장 읽기는 배운 개수, 나머지는 마친 날 / 6개월 {TOTAL_DAYS}일")).color(muted()));
            });
            ui.add_space(4.0);
            let narrow = ui.available_width() < 600.0;
            egui::Grid::new("activity_progress").num_columns(if narrow { 1 } else { 3 }).spacing([14.0, 6.0]).show(ui, |ui| {
                for (i, key) in PROGRESS_ACTIVITIES.iter().enumerate() {
                    let Some((_, label, _)) = STEPS.iter().find(|s| s.0 == *key) else { continue };
                    let today_mark = if self.step_done(key) { " · 오늘 ✓" } else { "" };
                    let (frac, detail) = match self.item_progress(key) {
                        // Counted per item, so every word or sentence learned moves the bar.
                        Some((n, total, done, unit)) => (n as f32 / total.max(1) as f32, format!("{n}/{}{unit} · 기억 완료 {done}", thousands(total as u64))),
                        None => {
                            let n = self.activity_days(key);
                            (n as f32 / TOTAL_DAYS as f32, format!("{n}/{TOTAL_DAYS}일"))
                        }
                    };
                    let bar = egui::ProgressBar::new(frac.min(1.0)).desired_height(10.0).corner_radius(5.0).fill(colors[i % colors.len()]);
                    let detail = RichText::new(format!("{:.1}%  ({detail}){today_mark}", frac * 100.0)).color(muted());
                    if narrow {
                        // A phone: name and numbers, then the bar the whole width.
                        ui.vertical(|ui| {
                            ui.label(RichText::new(*label).strong());
                            ui.label(detail.small());
                            ui.add(bar.desired_width(ui.available_width()));
                        });
                    } else {
                        ui.label(RichText::new(*label).strong());
                        ui.add(bar.desired_width(320.0));
                        ui.label(detail);
                    }
                    ui.end_row();
                }
            });
        });
    }

    pub fn step_done(&self, key: &str) -> bool {
        self.store.profile.today.done.iter().any(|d| d == key)
    }

    /// Creates/extends homework from weak items (things the learner keeps getting wrong).
    pub fn assign_homework(&mut self) {
        let in_open: Vec<String> = self
            .store
            .homework
            .iter()
            .filter(|h| h.completed.is_none())
            .flat_map(|h| h.tasks.iter().map(|t| t.item_id.clone()))
            .collect();
        let weak: Vec<StudyItem> = self.store.items.iter().filter(|i| i.weak && !in_open.contains(&i.id)).take(6).cloned().collect();
        if weak.is_empty() {
            return;
        }
        let has_claude = self.claude().is_some();
        let reps = self.store.settings.reps_weak;
        let mut tasks = Vec::new();
        for i in &weak {
            match i.kind {
                ItemKind::Sentence => tasks.push(HomeworkTask {
                    kind: TaskKind::ReadAloud,
                    item_id: i.id.clone(),
                    text: i.text.clone(),
                    meaning_ko: i.meaning_ko.clone(),
                    instruction_ko: format!("소리 내어 {reps}번 정확하게 읽기"),
                    reps,
                    ..Default::default()
                }),
                ItemKind::Word => {
                    let practice = if i.example.is_empty() { i.text.clone() } else { i.example.clone() };
                    tasks.push(HomeworkTask {
                        kind: TaskKind::ReadAloud,
                        item_id: i.id.clone(),
                        text: practice,
                        meaning_ko: if i.example_ko.is_empty() { i.meaning_ko.clone() } else { i.example_ko.clone() },
                        instruction_ko: format!("'{}' 예문을 {reps}번 읽기", i.text),
                        reps,
                        ..Default::default()
                    });
                    if has_claude {
                        tasks.push(HomeworkTask {
                            kind: TaskKind::UseWord,
                            item_id: i.id.clone(),
                            text: i.text.clone(),
                            meaning_ko: i.meaning_ko.clone(),
                            instruction_ko: format!("'{}'({})를 넣어 나만의 문장 말하기", i.text, i.meaning_ko),
                            reps: 1,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if let Some(open) = self.store.homework.iter_mut().find(|h| h.completed.is_none() && h.assigned == today()) {
            open.tasks.extend(tasks);
        } else {
            self.store.homework.push(Homework { id: format!("hw-{}", today()), assigned: today(), tasks, completed: None });
        }
        self.store.save_homework();
    }

    /// End-of-day: assigns homework and nudges the level by today's average score.
    pub fn wrap_up(&mut self) {
        self.assign_homework();
        let scores = self.store.profile.today.scores.clone();
        if scores.len() >= 10 && !self.store.profile.today.wrapped_up {
            let avg = scores.iter().sum::<f32>() / scores.len() as f32;
            let delta = ((avg - 75.0) / 25.0 * 0.05).clamp(-0.05, 0.05);
            if delta.abs() >= 0.01 {
                let lvl = self.store.profile.level + delta;
                self.store.record_level(lvl, &format!("일일 학습 평균 {avg:.0}점 ({}회 발화)", scores.len()));
            }
        }
        self.store.profile.today.wrapped_up = true;
        self.store.save_profile();
        self.update_history();
        self.toast("오늘 학습을 마무리했어요. 숙제를 확인하세요!");
    }

    // ------------------------------------------------------------ today's lesson

    pub fn ensure_lesson(&mut self) {
        // After a failure, wait for 다시 시도 / 내장 레슨 instead of asking again every frame
        // (an exhausted API credit once sent ~5,000 failing requests overnight).
        if self.lesson.as_ref().and_then(|l| l.date) == Some(today()) || self.lesson_job.running() || self.lesson_error.is_some() {
            return;
        }
        let p = &self.store.profile;
        let week = curriculum::week_number(p);
        let day = curriculum::day_number(p);
        let phase = curriculum::phase_of_week(week);
        match self.claude() {
            Some(c) => {
                let wk = curriculum::week(week);
                let past = self.store.past_lessons(60);
                let mut avoid_sentences: Vec<String> =
                    self.store.items.iter().rev().filter(|i| i.kind == ItemKind::Sentence).take(80).map(|i| i.text.clone()).collect();
                avoid_sentences.extend(past.iter().flat_map(|l| l.sentences.iter().map(|s| s.text.clone())).take(40));
                avoid_sentences.dedup();
                let req = LessonRequest {
                    day,
                    week,
                    phase: phase.name_ko.to_string(),
                    theme: wk.theme.to_string(),
                    business: wk.business.to_string(),
                    grammar: wk.grammar.to_string(),
                    level: p.level,
                    known: self.store.items.iter().filter(|i| i.kind == ItemKind::Word).map(|i| i.text.clone()).collect(),
                    known_phrases: self.store.items.iter().filter(|i| i.is_chunk()).map(|i| i.text.clone()).collect(),
                    weak: self.store.items.iter().filter(|i| i.weak).take(8).map(|i| i.text.clone()).collect(),
                    // Words come from the built-in database (`word_set`), not from Claude.
                    n_words: 0,
                    n_sentences: phase.sentences,
                    avoid_sentences,
                    avoid_questions: past.iter().map(|l| l.self_expression.question.clone()).filter(|q| !q.is_empty()).collect(),
                    avoid_scenarios: past.iter().map(|l| l.business.title.clone()).filter(|q| !q.is_empty()).collect(),
                };
                self.lesson_error = None;
                self.lesson_job.spawn(move || c.generate_lesson(&req));
            }
            None => self.use_builtin_lesson(),
        }
    }

    pub fn use_builtin_lesson(&mut self) {
        let p = &self.store.profile;
        let week = curriculum::week_number(p);
        let phase = curriculum::phase_of_week(week);
        let n_words = if self.store.settings.new_words_per_day > 0 { self.store.settings.new_words_per_day } else { phase.new_words };
        let past = self.store.past_lessons(400);
        let day = curriculum::day_number(p);
        let mut l = content::builtin_lesson(week, day, &self.store.items, &past, n_words as usize, phase.sentences as usize);
        // Self-expression and the day's conversation from the level's offline scripts (with
        // useful phrases, follow-up questions and a turn-by-turn script).
        let used_q = |q: &str| past.iter().any(|x| x.self_expression.question == q);
        if let Some(q) = crate::offline_talk::pick_question(p.level, used_q, day.max(0) as usize) {
            l.self_expression = q.to_self_expression();
        }
        let used_s = |t: &str| past.iter().any(|x| x.business.title == t);
        if let Some(sc) = crate::offline_talk::pick_scenario(p.level, used_s, day.max(0) as usize) {
            l.business = sc.to_business();
        }
        self.store.save_lesson(&l);
        self.lesson = Some(l);
    }

    fn poll_jobs(&mut self) {
        if let Some(r) = self.lesson_job.poll() {
            match r {
                Ok(l) => {
                    for s in &l.sentences {
                        self.store.put_gloss(&s.text, &s.chunks);
                    }
                    self.store.save_lesson(&l);
                    self.lesson = Some(l);
                }
                Err(e) if self.api_down.load(std::sync::atomic::Ordering::Relaxed) => {
                    // No credit / key / network: go on with the built-in lesson and scripts.
                    crate::applog!("lesson: Claude를 쓸 수 없어 내장 레슨으로 진행 · {e:#}");
                    self.use_builtin_lesson();
                    self.toast("Claude API를 쓸 수 없어 내장 레슨·대본으로 진행해요");
                }
                Err(e) => {
                    crate::applog!("lesson: 레슨 생성 실패, 다시 시도를 기다림 · {e:#}");
                    self.lesson_error = Some(format!("{e:#}"));
                }
            }
        }
    }

    /// Shows a spinner / error while the lesson is being prepared. Returns true when ready.
    pub fn lesson_ready_ui(&mut self, ui: &mut egui::Ui) -> bool {
        self.ensure_lesson();
        if self.lesson.as_ref().and_then(|l| l.date) == Some(today()) {
            return true;
        }
        card(ui, |ui| {
            if self.lesson_job.running() {
                spinner_label(ui, "Claude가 오늘의 레슨을 만들고 있어요… (10~40초)");
            }
            if let Some(e) = self.lesson_error.clone() {
                error_label(ui, &e);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("다시 시도").clicked() {
                        self.lesson_error = None;
                        self.ensure_lesson();
                    }
                    if ui.button("내장 레슨으로 진행").clicked() {
                        self.lesson_error = None;
                        self.use_builtin_lesson();
                    }
                });
            }
        });
        false
    }

    // ------------------------------------------------------------ navigation

    pub fn go(&mut self, s: Screen) {
        if self.voice.recorder.is_recording() {
            self.voice.recorder.stop();
        }
        self.voice.player.stop();
        self.voice.speaker.stop();
        if s != self.screen {
            self.queue = None;
            crate::applog!("screen: {:?} → {s:?}", self.screen);
        }
        self.screen = s;
    }

    fn nav_ui(&mut self, ui: &mut egui::Ui, view_h: f32) {
        let top = ui.cursor().top();
        // The scroll area clips to the panel's inner width; the selected item's bar sits in the margin.
        let clip = ui.clip_rect();
        ui.set_clip_rect(egui::Rect::from_x_y_ranges(clip.left() - 14.0..=clip.right() + 14.0, clip.y_range()));
        ui.add_space(18.0);
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(44.0), egui::Sense::hover());
            crate::art::gradient_rect(ui.painter(), r, 13.0, ACCENT, crate::art::VIOLET);
            let t = ui.input(|i| i.time) as f32;
            crate::art::mascot(ui.painter(), r.center() - Vec2::new(0.0, 2.0), 28.0, crate::art::Mood::Happy, t);
            ui.vertical(|ui| {
                ui.label(RichText::new("SpeakUp").size(19.0).strong().color(text_strong()));
                ui.label(RichText::new(format!("{} 듣기·말하기 6개월", self.lang.name_ko)).size(12.5).color(muted()));
            });
        });
        ui.add_space(16.0);
        let groups: &[(&str, &[(Screen, &str)])] = &[
            ("오늘의 학습", &[(Screen::Home, "🏠  홈"), (Screen::Homework, "📝  숙제"), (Screen::Review, "🔁  복습")]),
            (
                "연습",
                &[
                    (Screen::Words, "📚  새 단어"),
                    (Screen::Reading, "📖  문장 읽기"),
                    (Screen::Speed, "⚡  스피드 트레이닝"),
                    (Screen::Listening, "🎧  리스닝 랩"),
                    (Screen::SelfExpression, "💬  자기 표현"),
                    (Screen::Business, "💼  회화 · 비즈니스"),
                    (Screen::Library, "📘  리딩 라이브러리"),
                    (Screen::Writing, "✏  쓰기"),
                    (Screen::Translator, "🌐  자동 번역기"),
                ],
            ),
            ("내 학습", &[(Screen::Feedback, "🔬  피드백 센터"), (Screen::Vocabulary, "📒  내 단어장"), (Screen::History, "📅  학습 기록"), (Screen::Progress, "📈  레벨 · 평가"), (Screen::Settings, "⚙  설정")]),
        ];
        let tags = self.step_tags();
        for (group, items) in groups {
            ui.label(RichText::new(*group).size(12.0).color(muted()).strong());
            ui.add_space(2.0);
            for (s, label) in *items {
                let selected = self.screen == *s;
                let text = RichText::new(*label).size(15.5).color(if selected { Color32::WHITE } else { themed(Color32::from_rgb(200, 208, 224), Color32::from_rgb(52, 60, 78)) });
                let bg = ui.painter().add(egui::Shape::Noop);
                let btn = egui::Button::new(if selected { text.strong() } else { text })
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(10.0)
                    .min_size(Vec2::new(ui.available_width(), 36.0));
                let resp = ui.add(btn);
                if selected {
                    let r = resp.rect;
                    ui.painter().set(bg, crate::art::gradient_rect_shape(r, 10.0, ACCENT.gamma_multiply(0.75), crate::art::VIOLET.gamma_multiply(0.45)));
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min + Vec2::new(-14.0, 8.0), Vec2::new(4.0, r.height() - 16.0)), 2.0, crate::art::CYAN);
                } else if resp.hovered() {
                    ui.painter().set(bg, egui::Shape::rect_filled(resp.rect, 10.0, Color32::from_white_alpha(10)));
                }
                if let Some((_, n, tag)) = tags.iter().find(|t| t.0 == *s) {
                    paint_step_tag(ui, resp.rect, *n, *tag);
                }
                if resp.clicked() {
                    self.go(*s);
                    self.nav_open = false;
                }
            }
            ui.add_space(12.0);
        }
        // Status lines at the bottom, or right after the menu when the window is short.
        let status_h = 76.0;
        let left = view_h - (ui.cursor().top() - top);
        if left > status_h {
            ui.add_space(left - status_h);
        }
        ui.vertical(|ui| {
            let (txt, col) = match self.voice.stt.status() {
                SttStatus::Ready => (if self.voice.stt.loaded.load(std::sync::atomic::Ordering::Relaxed) { "● 음성 인식 준비됨" } else { "● 음성 인식 대기 (마이크 누르면 켜짐)" }, GOOD),
                SttStatus::Downloading => ("● 음성 인식 모델 받는 중", warn()),
                SttStatus::Missing => ("● 음성 인식 모델 없음", BAD),
                SttStatus::Error(_) => ("● 음성 인식 오류", BAD),
            };
            ui.label(RichText::new(txt).color(col).size(12.5));
            // Which Japanese voice is speaking (or that none is installed).
            let neural = self.voice.speaker.neural.as_ref().map(|n| n.status());
            let (vt, vc) = if self.voice.speaker.neural_active() {
                ("● AI 음성 (VOICEVOX Nemo)", GOOD)
            } else if neural == Some(crate::voicevox::Status::Downloading) {
                ("● AI 음성 받는 중", warn())
            } else if self.voice.speaker.voices.is_empty() {
                ("● 일본어 음성 없음 — 설치 방법 ›", BAD)
            } else if self.voice.speaker.natural {
                ("● 일본어 음성 (고품질)", GOOD)
            } else {
                ("● 일본어 음성 (기본) — 고품질 받기 ›", warn())
            };
            if ui.add(egui::Label::new(RichText::new(vt).color(vc).size(12.5)).sense(egui::Sense::click())).on_hover_text("설정에서 일본어 음성을 고를 수 있어요").clicked() {
                self.go(Screen::Settings);
            }
            let api = if self.claude().is_some() { ("● Claude 연결됨", GOOD) } else { ("● Claude API 키 없음", BAD) };
            ui.label(RichText::new(api.0).color(api.1).size(12.5));
        });
    }

    // ------------------------------------------------------------ home / dashboard

    fn home_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.store.profile.clone();
        let day = curriculum::day_number(&p);
        let week = curriculum::week_number(&p);
        let phase = curriculum::phase_of_week(week);
        let wk = curriculum::week(week);
        let steps = self.todays_steps();
        let done = steps.iter().filter(|s| self.step_done(s.0)).count();

        // Hero: greeting, program progress, daily goal ring — over a gradient with the
        // six-month journey drawn in the background.
        let hero_bg = ui.painter().add(egui::Shape::Noop);
        let hero = egui::Frame::new()
            .corner_radius(22.0)
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 150.0).max(140.0));
                        let hello = if p.name.is_empty() { "오늘도 일본어로 말해 볼까요?".to_string() } else { format!("{}님, 오늘도 일본어로 말해 볼까요?", p.name) };
                        ui.label(RichText::new(hello).size(26.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new(format!("Day {day} / {TOTAL_DAYS}  ·  {week}주차  ·  {}", phase.name_ko)).size(15.0).color(Color32::from_rgb(200, 214, 255)));
                        ui.add_space(8.0);
                        ui.add(egui::ProgressBar::new(day as f32 / TOTAL_DAYS as f32).desired_height(10.0).fill(Color32::from_rgb(120, 170, 255)).corner_radius(5.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("🎯 {}", phase.goal_ko)).color(Color32::from_rgb(200, 214, 255)));
                        ui.add_space(10.0);
                        ui.horizontal_wrapped(|ui| {
                            if let Some((n, label, s)) = self.next_step() {
                                let what = if done == 0 { "▶  오늘의 학습 시작" } else { "▶  오늘의 학습 이어하기" };
                                if primary_button(ui, &format!("{what} · {n}. {label}")).on_hover_text("왼쪽 메뉴의 번호 태그 순서대로 하면 돼요").clicked() {
                                    self.go(s);
                                }
                            } else {
                                chip(ui, "🎉 오늘 학습 완료!", GOOD);
                            }
                            if !self.store.profile.today.wrapped_up && done > 0 && ui.add(egui::Button::new(RichText::new("🌙 오늘 학습 마무리").size(17.0)).min_size(crate::widgets::BIG_BUTTON).corner_radius(12.0)).clicked() {
                                self.wrap_up();
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        let all_done = done == steps.len();
                        crate::art::mascot_widget(ui, 54.0, if all_done { crate::art::Mood::Cheer } else { crate::art::Mood::Talking });
                        ring(ui, done as f32 / steps.len().max(1) as f32, &format!("{done}/{}", steps.len()), 120.0);
                    });
                });
            });
        {
            let r = hero.response.rect;
            let art = egui::Rect::from_min_max(egui::pos2(r.left() + r.width() * 0.40, r.top() + r.height() * 0.42), egui::pos2(r.right() - 190.0, r.bottom()));
            let bg = crate::art::gradient_rect_shape(r, 22.0, Color32::from_rgb(40, 70, 190), Color32::from_rgb(98, 52, 170));
            ui.painter().set(hero_bg, egui::Shape::Vec(vec![bg, crate::art::journey(art, day as f32 / TOTAL_DAYS as f32)]));
        }
        if curriculum::program_finished(&p) {
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.label(RichText::new("🎉 6개월 과정을 모두 마쳤어요! 최종 레벨 평가를 보고 복습을 계속하세요.").size(17.0));
            });
        }
        ui.add_space(12.0);

        // Stat tiles
        let goal = self.store.settings.daily_goal_minutes.max(1) as u64;
        let mins = p.today.study_seconds / 60;
        let total_mins = p.study_seconds / 60;
        let program_mins = TOTAL_DAYS as u64 * goal;
        let words = self.store.items.iter().filter(|i| i.is_vocab_word()).count();
        let chunks = self.store.items.iter().filter(|i| i.is_chunk()).count();
        let sentences = self.store.items.iter().filter(|i| i.kind == ItemKind::Sentence).count() - chunks;
        let level_words = level_words(p.level);
        let mut retest = false;
        ui.horizontal_wrapped(|ui| {
            use crate::screens::history::{stat_card, stat_card_centered};
            let gap = ui.spacing().item_spacing.x;
            // Five cards across the content column (each adds 32 px of margin + 2 px of border);
            // two per row on a phone.
            let per_row = if ui.available_width() < 600.0 { 2.0 } else { 5.0 };
            let w = ((ui.available_width().min(CONTENT_WIDTH) - gap * (per_row - 1.0)) / per_row - 34.0).max(100.0);
            stat_card_centered(ui, w, &format!("🔥 {}일", p.streak), "연속 학습", warn());
            stat_card(ui, w, &fmt_minutes(total_mins), &format!("6개월 목표 {}분", thousands(program_mins)), themed(Color32::from_rgb(110, 220, 200), Color32::from_rgb(15, 140, 125)), |ui| {
                let frac = total_mins as f32 / program_mins.max(1) as f32;
                ui.add(egui::ProgressBar::new(frac.min(1.0)).desired_height(8.0).corner_radius(4.0).fill(Color32::from_rgb(110, 220, 200)));
                ui.label(RichText::new(format!("{:.1}% 완료", frac * 100.0)).small().color(muted()));
            });
            stat_card(ui, w, &format!("{mins} / {goal}분"), "오늘 학습 시간", if mins >= goal { GOOD } else { text_strong() }, |_| {});
            stat_card(ui, w, &format!("{} {}", level_name_of(p.level), cefr_of(p.level)), &format!("레벨 {:.1} / 10", p.level), ACCENT, |ui| {
                // Taking the test often keeps the level (and so the content) right: make it inviting.
                retest = shiny_button(ui, "📝 레벨 테스트 ›", ACCENT, crate::art::VIOLET).on_hover_text("자주 볼수록 좋아요! 지금 실력을 다시 측정해 맞는 레벨로 이동합니다").clicked();
            });
            stat_card(ui, w, &format!("{} / {}", words, thousands(level_words as u64)), &format!("배운 단어 / {} 필수", cefr_of(p.level)), themed(Color32::from_rgb(190, 140, 255), Color32::from_rgb(120, 70, 220)), |ui| {
                ui.add(egui::ProgressBar::new((words as f32 / level_words as f32).min(1.0)).desired_height(8.0).corner_radius(4.0).fill(Color32::from_rgb(190, 140, 255)));
                ui.label(RichText::new(format!("덩어리 표현 {chunks}개 · 문장 {sentences}개")).small().color(muted()));
            });
        });
        if retest {
            self.test = Some(LevelTest::retest());
            self.go(Screen::LevelTest);
        }
        ui.add_space(12.0);
        self.activity_progress_card(ui);
        ui.add_space(12.0);

        // Skill balance: 말하기 40 · 듣기 30 · 읽기 15 · 쓰기 15 of the daily goal.
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("오늘의 학습 비중").size(17.0).strong());
                ui.label(RichText::new("말하기 40 · 듣기 30 · 읽기 15 · 쓰기 15 — 말은 들은 만큼 나와요").color(muted()));
            });
            ui.label(
                RichText::new({
                    let g = band_index_of_cefr(&self.store.settings.goal_cefr).unwrap_or(2);
                    format!("🎯 6개월 목표: {} — {} · 하루 권장: 앱 60분 + 유튜브·팟캐스트 듣기 30분", LEVEL_BANDS[g].2, GOAL_DESC[g])
                })
                    .small()
                    .color(muted()),
            );
            let t = &p.today;
            let secs = [t.speaking_seconds, t.listening_seconds, t.reading_seconds, t.writing_seconds];
            let colors = [crate::art::PINK, GOOD, crate::art::CYAN, warn()];
            // Four across, or two rows of two on a phone.
            let per_row = if ui.available_width() < 600.0 { 2 } else { 4 };
            for row in (0..4).collect::<Vec<_>>().chunks(per_row) {
                ui.columns(per_row, |cols| {
                    for (col, &i) in cols.iter_mut().zip(row) {
                        let (label, w) = SKILL_WEIGHTS[i];
                        let target = (goal as f32 * w).round().max(1.0);
                        let mins = secs[i] as f32 / 60.0;
                        col.label(RichText::new(format!("{label}  {:.0} / {target:.0}분", mins.floor())).strong());
                        col.add(egui::ProgressBar::new((mins / target).min(1.0)).desired_height(10.0).corner_radius(5.0).fill(colors[i]));
                    }
                });
            }
        });
        ui.add_space(12.0);

        self.coach_reminders_ui(ui);

        // Today's routine
        let mut next: Option<Screen> = None;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("오늘의 루틴").size(19.0).strong());
                chip(ui, &format!("{} · {}", wk.theme_ko, wk.theme), ACCENT);
            });
            ui.label(RichText::new(format!("비즈니스: {}  ·  문법: {}", wk.business, wk.grammar)).color(muted()));
            ui.add_space(8.0);
            for (key, label, screen) in &steps {
                let is_done = self.step_done(key);
                let extra = match *key {
                    "review" => format!("{}개", srs::due_count(&self.store.items)),
                    "listening" => format!("따라 말하기 {}/5", self.store.profile.today.listening_passes.min(5)),
                    "speed" => format!("{}/3 라운드", self.store.profile.today.speed_rounds.min(3)),
                    "library" => format!("{}/{} 문단", self.store.profile.today.paragraphs_read.min(crate::screens::library::READ_GOAL), crate::screens::library::READ_GOAL),
                    "writing" => format!("{}/{}개", self.store.profile.today.writings_done.min(crate::screens::writing::WRITING_GOAL), crate::screens::writing::WRITING_GOAL),
                    _ => String::new(),
                };
                let row = egui::Frame::new()
                    .fill(if is_done { GOOD.gamma_multiply(0.08) } else { subtle_fill() })
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(*label).size(16.5).color(if is_done { muted() } else { text_strong() }));
                            if !extra.is_empty() {
                                ui.label(RichText::new(extra).color(muted()));
                            }
                            at_right(ui, 90.0, |ui| {
                                if is_done {
                                    chip(ui, "✓ 완료", GOOD);
                                } else {
                                    chip(ui, "시작 ›", ACCENT);
                                }
                            });
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if row.clicked() {
                    next = Some(*screen);
                }
                ui.add_space(4.0);
            }
        });
        if let Some(s) = next {
            self.go(s);
        }

        let open_hw: usize = self.store.homework.iter().filter(|h| h.completed.is_none()).map(|h| h.progress().1 - h.progress().0).sum();
        let weak = self.store.items.iter().filter(|i| i.weak).count();
        let mistakes = self.store.mistakes.iter().filter(|m| m.fixed_streak < 3).count();
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            chip(ui, &format!("약점 {weak}개"), if weak > 0 { warn() } else { muted() });
            chip(ui, &format!("남은 숙제 {open_hw}개"), if open_hw > 0 { warn() } else { muted() });
            if let Some(w) = curriculum::checkpoint_due(&p) {
                if ui.button(format!("📋 {w}주차 레벨 평가 보기")).clicked() {
                    self.test = Some(LevelTest::new(false, w));
                    self.go(Screen::LevelTest);
                }
            } else if let Some(w) = curriculum::next_checkpoint(&p) {
                chip(ui, &format!("다음 레벨 평가 {w}주차"), muted());
            }
            if mistakes > 0 && ui.link(RichText::new(format!("발음 실수 {mistakes}개 연습하기 ›")).color(warn())).clicked() {
                self.go(Screen::Feedback);
            }
        });
        if self.claude().is_none() {
            ui.add_space(6.0);
            ui.label(RichText::new("⚠ Claude API 키가 없어 내장 콘텐츠로 학습 중입니다. AI 피드백·회화·레슨 생성을 쓰려면 설정에서 키를 입력하세요.").color(warn()));
        }
    }

    // ------------------------------------------------------------ onboarding

    fn onboarding_ui(&mut self, ui: &mut egui::Ui) {
        crate::art::onboarding_hero(ui);
        ui.add_space(16.0);
        // Anything entered here is saved immediately and never asked again on the next launch.
        let has_name = !self.store.profile.name.trim().is_empty();
        let has_key = self.claude().is_some();
        card(ui, |ui| {
            ui.label(RichText::new("1. 이름 (선택)").strong());
            if has_name && !self.edit_name {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("✅ {}", self.store.profile.name)).color(GOOD));
                    if ui.button("변경").clicked() {
                        self.edit_name = true;
                    }
                });
            } else if ui.text_edit_singleline(&mut self.store.profile.name).changed() {
                self.store.save_profile();
            }
            ui.add_space(8.0);
            ui.label(RichText::new("2. Claude API 키").strong());
            if has_key && !self.edit_key {
                ui.horizontal_wrapped(|ui| {
                    let from_env = self.store.settings.api_key.trim().is_empty();
                    ui.label(RichText::new(if from_env { "✅ 환경변수 ANTHROPIC_API_KEY 사용" } else { "✅ 저장됨" }).color(GOOD));
                    if ui.button("변경").clicked() {
                        self.edit_key = true;
                    }
                });
            } else {
                ui.label(RichText::new("레슨 생성·말하기 피드백·AI 회화 상대에 사용됩니다. 환경변수 ANTHROPIC_API_KEY 도 사용할 수 있어요.").color(muted()));
                if ui.add(egui::TextEdit::singleline(&mut self.store.settings.api_key).password(true).hint_text("sk-ant-...")).changed() {
                    self.store.save_settings();
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new("3. 음성 인식 모델 (오프라인, 최초 1회 다운로드)").strong());
            self.stt_setup_ui(ui);
            ui.add_space(8.0);
            ui.label(RichText::new("4. 일본어 음성").strong());
            if self.voice_setup_ui(ui) {
                self.store.save_settings();
            }
            ui.add_space(8.0);
            ui.label(RichText::new("5. 마이크·스피커 점검").strong());
            if self.audio_check_ui(ui) {
                self.store.save_settings();
            }
        });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            let ready = self.voice.stt.is_ready();
            if !ready {
                ui.label(RichText::new("음성 인식 모델이 준비되면 레벨 테스트를 시작할 수 있어요.").color(muted()));
            }
            if ui.add_enabled(ready, egui::Button::new(RichText::new("레벨 테스트 시작 ▶").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                self.store.save_settings();
                self.store.save_profile();
                self.test = Some(LevelTest::new(true, 0));
                self.screen = Screen::LevelTest;
            }
            if ui.button("테스트 없이 처음부터(N5) 시작").clicked() {
                self.store.save_settings();
                self.store.profile.placement_done = true;
                self.store.profile.start_date = today();
                self.store.record_level(1.5, "시작 (레벨 테스트 생략)");
                self.screen = Screen::Home;
            }
        });
    }

    /// Japanese voices: the natural neural one (VOICEVOX Nemo — download once, choose a voice,
    /// test), then the OS voice it falls back to.
    pub fn voice_setup_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::voicevox::{Status, VOICES};
        let mut changed = false;
        if let Some(n) = self.voice.speaker.neural.as_ref() {
            ui.label(RichText::new("🎤 자연스러운 AI 음성 (VOICEVOX Nemo, 오프라인)").strong().color(text_strong()));
            match n.status() {
                Status::Ready => {
                    ui.label(RichText::new("✅ 준비 완료 — 사람처럼 자연스러운 신경망 음성으로 읽어 줘요.").color(GOOD).small());
                }
                Status::Downloading => {
                    let d = n.downloaded.load(std::sync::atomic::Ordering::Relaxed) as f32;
                    let t = n.total.load(std::sync::atomic::Ordering::Relaxed).max(1) as f32;
                    ui.add(egui::ProgressBar::new(d / t).text(format!("AI 음성 받는 중 {:.0} / {:.0} MB", d / 1e6, t / 1e6)));
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                }
                status => {
                    if let Status::Error(e) = status {
                        error_label(ui, &e);
                    }
                    ui.label(RichText::new("한 번만 받으면(약 100MB) 인터넷 없이 계속 사용합니다.").color(muted()).small());
                    if ui.add(egui::Button::new(RichText::new("⬇ 자연스러운 AI 음성 받기 (권장)").strong()).fill(ACCENT)).clicked() {
                        n.download(self.store.models_dir());
                    }
                }
            }
            if n.is_ready() {
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.checkbox(&mut self.store.settings.neural_tts, "AI 음성 사용").changed();
                    let current = crate::voicevox::voice_label(self.store.settings.voicevox_voice);
                    egui::ComboBox::from_id_salt("voicevox_voice").width(140.0).selected_text(current).show_ui(ui, |ui| {
                        for (id, _, label) in VOICES {
                            if ui.selectable_label(self.store.settings.voicevox_voice == *id, *label).clicked() {
                                self.store.settings.voicevox_voice = *id;
                                changed = true;
                            }
                        }
                    });
                    if ui.button("🔊 들어보기").clicked() {
                        self.voice.speaker.use_neural = true;
                        self.voice.speaker.neural_voice = self.store.settings.voicevox_voice;
                        self.voice.say("こんにちは。一緒に日本語を話す練習をしましょう。");
                    }
                });
                ui.label(RichText::new("음성: VOICEVOX Nemo").color(muted()).small());
            }
            ui.add_space(6.0);
        }
        self.voice.speaker.use_neural = self.store.settings.neural_tts;
        self.voice.speaker.neural_voice = self.store.settings.voicevox_voice;
        ui.label(RichText::new(if self.voice.speaker.neural.is_some() { "시스템 음성 (AI 음성을 쓰지 않을 때)" } else { "시스템 음성" }).color(muted()));
        if let Some(e) = &self.voice.speaker.error {
            error_label(ui, e);
        }
        let voices = self.voice.speaker.voices.clone();
        let current = voices.iter().find(|v| v.0 == self.store.settings.tts_voice_id).map(|v| v.1.clone()).unwrap_or("기본".into());
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("tts_voice").width(ui.available_width().clamp(160.0, 280.0)).selected_text(current).show_ui(ui, |ui| {
                for (id, label) in &voices {
                    if ui.selectable_label(self.store.settings.tts_voice_id == *id, label).clicked() {
                        self.store.settings.tts_voice_id = id.clone();
                        self.voice.speaker.set_voice(id);
                        changed = true;
                    }
                }
            });
        });
        if !self.voice.speaker.natural && !self.voice.speaker.neural_active() {
            ui.label(RichText::new(format!("💡 {}", crate::voice::INSTALL_VOICE_HELP)).color(muted()).small());
        }
        changed
    }

    /// Synthesizes today's lesson in the background so every 🔊 plays instantly.
    pub fn prefetch_lesson(&mut self) {
        let Some(lesson) = &self.lesson else { return };
        if !self.voice.speaker.neural_active() || self.prefetched == lesson.date {
            return;
        }
        self.prefetched = lesson.date;
        let r = self.voice.base_rate;
        for w in lesson.phrases.iter().chain(&lesson.words).chain(&lesson.expressions) {
            self.voice.speaker.prefetch(&w.word, r);
        }
        for s in &lesson.sentences {
            self.voice.speaker.prefetch(&s.text, r);
        }
        for w in lesson.phrases.iter().chain(&lesson.words) {
            self.voice.speaker.prefetch(&w.example, r);
        }
    }

    pub fn stt_setup_ui(&mut self, ui: &mut egui::Ui) {
        let models = self.lang.whisper_models;
        ui.horizontal_wrapped(|ui| {
            for (id, label) in models {
                ui.radio_value(&mut self.store.settings.whisper_model, id.to_string(), format!("{id} ({label})"));
            }
        });
        let path = stt::model_path(&self.store.models_dir(), &self.store.settings.whisper_model);
        match self.voice.stt.status() {
            SttStatus::Downloading => {
                let d = self.voice.stt.downloaded.load(std::sync::atomic::Ordering::Relaxed) as f32;
                let t = self.voice.stt.total.load(std::sync::atomic::Ordering::Relaxed).max(1) as f32;
                ui.add(egui::ProgressBar::new(d / t).text(format!("{:.0} / {:.0} MB", d / 1e6, t / 1e6)));
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            SttStatus::Ready if path.exists() => {
                ui.label(RichText::new("✅ 음성 인식 준비 완료").color(GOOD));
            }
            status => {
                if let SttStatus::Error(e) = status {
                    error_label(ui, &e);
                }
                if path.exists() {
                    if ui.button("이 모델 불러오기").clicked() {
                        self.store.save_settings();
                        self.voice.stt.load(path);
                    }
                } else if ui.button("⬇ 모델 다운로드").clicked() {
                    self.store.save_settings();
                    self.voice.stt.download(self.store.models_dir(), self.store.settings.whisper_model.clone());
                }
            }
        }
    }

    // ------------------------------------------------------------ settings

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "⚙ 설정", "API 키, 음성, 학습 강도, 유튜브 도구를 설정합니다.");
        let mut changed = false;
        // Screen theme: applied at once and remembered in settings.json.
        card(ui, |ui| {
            ui.label(RichText::new("🎨 테마").strong());
            ui.horizontal_wrapped(|ui| {
                let mut light = self.store.settings.light_theme;
                ui.radio_value(&mut light, false, "🌙 다크");
                ui.radio_value(&mut light, true, "☀ 라이트");
                if light != self.store.settings.light_theme {
                    self.store.settings.light_theme = light;
                    set_light(light);
                    crate::theme::apply_visuals(ui.ctx());
                    crate::applog!("settings: 테마 {}", if light { "라이트" } else { "다크" });
                    changed = true;
                }
            });
            ui.label(RichText::new("화면 색을 어둡게(다크) 또는 밝게(라이트) 바꿉니다. 바로 적용돼요.").color(muted()).small());
        });
        card(ui, |ui| {
            ui.label(RichText::new("🔔 알림").strong());
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.notify_minutes, 0..=30).text("분마다 알림 (0 = 끄기)")).changed();
            ui.label(RichText::new("화면 오른쪽 위에 못 끝낸 단어·문장, 진행 중인 주차와 진척, 배운 내용, 다음 레벨을 앞당기는 방법을 번갈아 알려 줘요. 말하는 중에는 뜨지 않아요.").color(muted()).small());
        });
        // Japanese in the 새 단어 / 문장 읽기 lists, big enough to copy into a notebook.
        card(ui, |ui| {
            ui.label(RichText::new("🔠 단어·문장 리스트 글자 크기").strong());
            let range = crate::models::Settings::LIST_TEXT_SIZES;
            let size = &mut self.store.settings.list_text_size;
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(*size > *range.start(), egui::Button::new("가 −")).clicked() {
                    *size = (*size - 2.0).max(*range.start());
                    changed = true;
                }
                changed |= ui.add(egui::Slider::new(size, range.clone()).step_by(1.0).suffix(" pt")).changed();
                if ui.add_enabled(*size < *range.end(), egui::Button::new("가 +")).clicked() {
                    *size = (*size + 2.0).min(*range.end());
                    changed = true;
                }
                if ui.button("기본(17)").clicked() {
                    *size = 17.0;
                    changed = true;
                }
            });
            let s = self.store.settings.list_size();
            egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                crate::widgets::ja_text(ui, "締め切り", s, text_strong());
                crate::widgets::ja_text(ui, "今週は仕事がたくさんあって大変です。", s, text_strong());
            });
            ui.label(RichText::new("📚 새 단어 · 📖 문장 읽기 목록의 일본어 단어와 문장만 커집니다 (노트에 옮겨 적을 때). 바로 적용돼요.").color(muted()).small());
        });
        card(ui, |ui| {
            ui.label(RichText::new("Claude API").strong());
            ui.horizontal_wrapped(|ui| {
                changed |= ui.add(egui::TextEdit::singleline(&mut self.store.settings.api_key).password(!self.show_key).desired_width(360.0).hint_text("sk-ant-...")).changed();
                ui.checkbox(&mut self.show_key, "표시");
            });
            ui.label(RichText::new("키는 이 컴퓨터의 settings.json 에만 저장됩니다. 비워두면 ANTHROPIC_API_KEY 환경변수를 사용합니다.").color(muted()).small());
            egui::ComboBox::from_label("모델")
                .selected_text(self.store.settings.model.clone())
                .show_ui(ui, |ui| {
                    for (id, label) in crate::claude::MODELS {
                        changed |= ui.selectable_value(&mut self.store.settings.model, id.to_string(), *label).changed();
                    }
                });
        });
        card(ui, |ui| {
            ui.label(RichText::new("음성").strong());
            ui.label(RichText::new("일본어 음성 (시스템 음성)").color(text_strong()));
            changed |= self.voice_setup_ui(ui);
            if ui.add(egui::Slider::new(&mut self.store.settings.tts_rate, 0.6..=1.4).text("말하기 속도")).changed() {
                self.voice.base_rate = self.store.settings.tts_rate;
                changed = true;
            }
            ui.add_space(6.0);
            // 받아쓰기: the level's pace, a little faster by default so it isn't too easy.
            let level = self.store.profile.level;
            ui.label(RichText::new(format!("✏ 받아쓰기 읽기 속도 (내 레벨 {} 기준)", cefr_of(level))).color(text_strong()));
            ui.horizontal_wrapped(|ui| {
                let extra = ["레벨 속도", "레벨 속도 +3%", "레벨 속도 +15%"];
                for (i, (name, _)) in DICTATION_SPEEDS.iter().enumerate() {
                    if ui.radio_value(&mut self.store.settings.dictation_speed, i, *name).on_hover_text(extra[i]).changed() {
                        changed = true;
                    }
                }
                if ui.button("🔊 들어 보기").clicked() {
                    let r = dictation_rate(level, self.store.settings.dictation_speed);
                    self.voice.say_at("いつもは地下鉄で会社に行きますが、今日は歩きました。", r);
                }
            });
            ui.label(RichText::new("느리게 = 레벨에 맞는 속도 · 보통 = 레벨 속도보다 3% 빠르게 (기본) · 빠르게 = 15% 빠르게. 레벨이 오르면 속도도 함께 올라가요.").color(muted()).small());
            ui.add_space(6.0);
            ui.label("음성 인식 모델");
            self.stt_setup_ui(ui);
        });
        card(ui, |ui| {
            ui.label(RichText::new("오디오 장치 · 마이크 점검").strong());
            changed |= self.audio_check_ui(ui);
        });
        card(ui, |ui| {
            ui.label(RichText::new("학습").strong());
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.daily_goal_minutes, 10..=120).text("일일 목표 학습 시간(분)")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.pass_score, 60.0..=95.0).text("통과 점수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.reps_new, 1..=6).text("새 항목 반복 횟수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.reps_weak, 2..=10).text("약점 항목 반복 횟수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.new_words_per_day, 0..=50).text("하루 새 단어 (0 = 35개)")).changed();
            changed |= ui.checkbox(&mut self.store.settings.furigana, "한자 위에 후리가나(읽는 법) 표시").changed();
            changed |= ui.checkbox(&mut self.store.settings.ko_role_colors, "한글 지문에 문장 성분 색상 표시 (주어·동사·목적어…)").changed();
            ui.label(RichText::new("눈이 피로하면 끄세요. 직독직해 색상은 📖 직독직해 버튼으로 따로 켜고 끕니다.").color(muted()).small());
            ui.horizontal_wrapped(|ui| {
                ui.label("학습 언어:");
                for l in language::LANGUAGES {
                    let _ = ui.radio(self.lang.code == l.code, l.name_ko);
                }
                ui.label(RichText::new("(다른 언어는 추후 지원)").color(muted()));
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("데이터").strong());
            ui.add(egui::Label::new(format!("저장 위치: {}", self.store.dir.display())).wrap_mode(egui::TextWrapMode::Wrap));
            ui.label(RichText::new("profile.json(레벨·기록), level_log.csv, items.json(단어·문장), homework.json(숙제), attempts.jsonl(발화 기록)").color(muted()).small());
            if !cfg!(target_os = "android") && ui.button("📂 폴더 열기").clicked() {
                let _ = open::that(&self.store.dir);
            }
        });
        crate::credits::credits_ui(ui);
        // App version at the bottom (from Cargo.toml at build time).
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(app_version_label()).color(muted()));
        });
        if changed {
            self.store.save_settings();
        }
    }

    // ------------------------------------------------------------ frame

    fn track_time(&mut self, ctx: &egui::Context) {
        let active = ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving()) || self.voice.recorder.is_recording();
        if active {
            self.last_activity = Instant::now();
        }
        let now = Instant::now();
        let dt = now - self.last_tick;
        if dt >= Duration::from_secs(1) {
            self.last_tick = now;
            let studying = !matches!(self.screen, Screen::Settings | Screen::Onboarding | Screen::Progress | Screen::History);
            if studying && now - self.last_activity < Duration::from_secs(120) {
                let secs = dt.as_secs();
                let t = &mut self.store.profile.today;
                self.store.profile.study_seconds += secs;
                t.study_seconds += secs;
                match skill_of(self.screen) {
                    Some(1) => t.listening_seconds += secs,
                    Some(2) => t.reading_seconds += secs,
                    Some(3) => t.writing_seconds += secs,
                    Some(_) => t.speaking_seconds += secs,
                    None => {}
                }
            }
        }
        if now - self.last_save > Duration::from_secs(30) {
            self.last_save = now;
            self.store.save_profile();
            self.update_history();
        }
    }
}

impl App {
    fn save_all(&mut self) {
        self.update_history();
        self.store.save_profile();
        self.store.save_items();
        self.store.save_homework();
        self.store.save_settings();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.web_req = None;
        self.roll_day();
        self.poll_jobs();
        self.apply_support_stage(ctx);
        crate::grammar::pump(ctx, self.claude(), self.store.settings.ko_role_colors);
        self.sync_lesson_words();
        self.prefetch_lesson();
        self.track_time(ctx);
        if self.lesson_job.running() || self.voice.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Keep a slow heartbeat so background results show up even without input.
        ctx.request_repaint_after(Duration::from_millis(500));

        let compact = ctx.screen_rect().width() < COMPACT_WIDTH;
        let with_nav = !matches!(self.screen, Screen::Onboarding | Screen::LevelTest);
        if compact && with_nav {
            // Phone: a top bar with the menu button and where we are.
            // Android draws the app under the status bar: leave it room.
            let top = if cfg!(target_os = "android") { 30 } else { 6 };
            egui::TopBottomPanel::top("phone_bar").frame(egui::Frame::new().fill(sidebar()).inner_margin(egui::Margin { left: 10, right: 10, top, bottom: 6 })).show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = if self.nav_open { "✖ 닫기" } else { "☰ 메뉴" };
                    if ui.add(egui::Button::new(RichText::new(label).size(16.0)).min_size(Vec2::new(84.0, 40.0))).clicked() {
                        self.nav_open = !self.nav_open;
                    }
                    ui.label(RichText::new(format!("SpeakUp · {}", self.lang.name_ko)).size(15.0).strong().color(text_strong()));
                });
            });
        } else {
            self.nav_open = false;
            if cfg!(target_os = "android") {
                // Room for the status bar the app is drawn under.
                egui::TopBottomPanel::top("status_bar_inset").exact_height(24.0).show_separator_line(false).frame(egui::Frame::new().fill(bg())).show(ctx, |_| {});
            }
        }
        if !compact && with_nav {
            egui::SidePanel::left("nav")
                .exact_width(SIDEBAR_WIDTH)
                .resizable(false)
                .frame(egui::Frame::new().fill(sidebar()).inner_margin(egui::Margin::symmetric(14, 0)).stroke(egui::Stroke::new(1.0_f32, border())))
                .show(ctx, |ui| {
                    // Scrolls when the window is too short for the whole menu (small screens).
                    let view_h = ui.available_height();
                    egui::ScrollArea::vertical().id_salt("nav_scroll").auto_shrink([false, false]).show(ui, |ui| self.nav_ui(ui, view_h));
                });
        }
        self.mic_permission_banner(ctx);
        if self.voice.player.output_stuck() {
            egui::TopBottomPanel::top("audio_warn").show(ctx, |ui| {
                let fix = if cfg!(target_os = "macos") {
                    "터미널에서 `sudo killall coreaudiod` 를 실행하면 오디오가 재시작됩니다."
                } else {
                    "출력 장치(스피커·헤드폰) 연결과 시스템 사운드 설정을 확인하세요."
                };
                ui.label(RichText::new(format!("🔇 시스템 오디오가 응답하지 않아 소리가 나지 않습니다. {fix}")).color(warn()).strong());
            });
        }
        let margin = if compact { egui::Margin { left: 12, right: 12, top: 12, bottom: if cfg!(target_os = "android") { 28 } else { 10 } } } else { egui::Margin { left: 32, right: 24, top: 22, bottom: 10 } };
        if compact && self.nav_open {
            // Phone menu: the whole page; picking a screen closes it.
            egui::CentralPanel::default().frame(egui::Frame::new().fill(sidebar()).inner_margin(egui::Margin::symmetric(24, 0))).show(ctx, |ui| {
                let view_h = ui.available_height();
                egui::ScrollArea::vertical().id_salt("nav_scroll").auto_shrink([false, false]).show(ui, |ui| self.nav_ui(ui, view_h));
            });
            self.notifications_ui(ctx);
            self.web.sync(frame, None, ctx, self.store.dir.join("webview"));
            return;
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(bg()).inner_margin(margin)).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // Content column of a fixed width, centered when the window is wider; on a phone
                // the whole width, never wider (rows that don't fit are clipped, not widening it).
                let extra = ((ui.available_width() - CONTENT_WIDTH) / 2.0).max(0.0);
                let width = ui.available_width().min(CONTENT_WIDTH);
                ui.horizontal_top(|ui| {
                    ui.add_space(extra);
                    ui.vertical(|ui| {
                        ui.set_max_width(width);
                        if compact {
                            ui.set_clip_rect(ui.clip_rect().intersect(egui::Rect::everything_left_of(ui.max_rect().right() + 1.0)));
                        }
                        match self.screen {
                            Screen::Onboarding => self.onboarding_ui(ui),
                            Screen::Home => self.home_ui(ui),
                            Screen::Homework => self.homework_ui(ui),
                            Screen::Review => self.review_ui(ui),
                            Screen::Words => self.words_ui(ui),
                            Screen::Reading => self.reading_ui(ui),
                            Screen::Speed => self.speed_ui(ui),
                            Screen::Listening => self.listening_ui(ui),
                            Screen::SelfExpression => self.self_expression_ui(ui),
                            Screen::Business => self.business_ui(ui),
                            Screen::Library => self.library_ui(ui),
                            Screen::Writing => self.writing_ui(ui),
                            Screen::Translator => self.translator_ui(ui),
                            Screen::Feedback => self.feedback_ui(ui),
                            Screen::Vocabulary => self.vocabulary_ui(ui),
                            Screen::History => self.history_ui(ui),
                            Screen::Progress => self.progress_ui(ui),
                            Screen::LevelTest => self.level_test_ui(ui),
                            Screen::Settings => self.settings_ui(ui),
                        }
                        let used = ui.min_rect().width();
                        if compact && used > width + 1.0 && self.overflow.map(|o| o.0) != Some(self.screen) {
                            crate::applog!("layout: {:?} is {used:.0} wide on a {width:.0} screen", self.screen);
                            self.overflow = Some((self.screen, used));
                        }
                    });
                });
            });
        });
        self.notifications_ui(ctx);
        // The web page is a native view over the window: shown only while a screen asks for it.
        self.web.sync(frame, self.web_req.as_ref(), ctx, self.store.dir.join("webview"));
        if let Some((msg, at)) = &self.toast {
            if at.elapsed() < Duration::from_secs(3) {
                // Every alert in the app: bottom right, wrapped so a long one stays on screen.
                egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0)).show(ctx, |ui| {
                    ui.set_max_width(420.0);
                    egui::Frame::new().fill(subtle_fill()).stroke(egui::Stroke::new(1.0, muted().gamma_multiply(0.5))).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
                        ui.label(RichText::new(msg).size(16.0));
                    });
                });
            } else {
                self.toast = None;
            }
        }
    }

    #[cfg(not(target_os = "ios"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_all();
    }

    /// iOS draws with wgpu, where eframe's exit hook has no OpenGL context.
    #[cfg(target_os = "ios")]
    fn on_exit(&mut self) {
        self.save_all();
    }
}

/// "45분", "3시간 5분".
fn fmt_minutes(m: u64) -> String {
    if m < 60 { format!("{m}분") } else { format!("{}시간 {}분", m / 60, m % 60) }
}

/// 5460 → "5,460".
/// "SpeakUp 버전 1.5.2 (macos aarch64)" for the settings screen.
pub fn app_version_label() -> String {
    format!("SpeakUp 버전 {} ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod format_tests {
    #[test]
    fn formats_minutes_and_thousands() {
        assert_eq!(super::fmt_minutes(45), "45분");
        assert_eq!(super::fmt_minutes(185), "3시간 5분");
        assert_eq!(super::thousands(5460), "5,460");
        assert_eq!(super::thousands(999), "999");
        assert_eq!(super::thousands(1234567), "1,234,567");
    }
}

/// Where a step of today's order stands, for the side menu tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepTag {
    Done,
    /// The one to do now; `first_run` on the very first day of study.
    Next { first_run: bool },
    Todo,
}

impl StepTag {
    pub fn text(self, n: usize) -> String {
        match self {
            StepTag::Done => format!("✓ {n}"),
            StepTag::Next { first_run: true } => format!("{n} 시작"),
            StepTag::Next { first_run: false } => format!("{n} 지금"),
            StepTag::Todo => n.to_string(),
        }
    }
}

/// A small pill in the top-right corner of a menu item: today's order number, "지금"/"시작"
/// on the step to do now, ✓ when done.
fn paint_step_tag(ui: &egui::Ui, item: egui::Rect, n: usize, tag: StepTag) {
    let (fill, text_color) = match tag {
        StepTag::Done => (GOOD.gamma_multiply(0.85), Color32::WHITE),
        StepTag::Next { .. } => (crate::art::PINK, Color32::WHITE),
        StepTag::Todo => (themed(Color32::from_rgb(58, 66, 92), Color32::from_rgb(214, 220, 236)), themed(Color32::from_rgb(210, 216, 232), Color32::from_rgb(60, 68, 90))),
    };
    let size = if matches!(tag, StepTag::Next { .. }) { 12.0 } else { 11.0 };
    let galley = ui.painter().layout_no_wrap(tag.text(n), egui::FontId::proportional(size), text_color);
    let pad = Vec2::new(6.0, 2.0);
    let pill = egui::Rect::from_min_size(egui::pos2(item.right() - galley.size().x - 2.0 * pad.x - 4.0, item.top() + 3.0), galley.size() + 2.0 * pad);
    let p = ui.painter();
    if matches!(tag, StepTag::Next { .. }) {
        p.rect_filled(pill.expand(2.5), 9.0, crate::art::PINK.gamma_multiply(0.3));
    }
    p.rect_filled(pill, 8.0, fill);
    p.galley(pill.min + pad, galley, text_color);
}
