//! Persistent data structures. Everything here is serialized to JSON files in the data directory.

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Target language code (see language.rs). Only "en" for now.
    pub target_language: String,
    pub api_key: String,
    pub model: String,
    pub tts_rate: f32,
    pub tts_voice_id: String,
    pub whisper_model: String,
    pub pass_score: f32,
    pub reps_new: u32,
    pub reps_review: u32,
    pub reps_weak: u32,
    pub new_words_per_day: u32,
    pub ytdlp_path: String,
    pub daily_goal_minutes: u32,
    /// Microphone / speaker names; empty = system default.
    pub input_device: String,
    pub output_device: String,
    /// Speak with the natural neural voice (Kokoro) when its model is installed.
    pub neural_tts: bool,
    pub neural_voice: String,
    /// Multilingual whisper model for Korean speech (translator): "small" or "base".
    pub korean_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target_language: "en".into(),
            api_key: String::new(),
            model: "claude-opus-5-5".into(),
            tts_rate: 1.0,
            tts_voice_id: String::new(),
            whisper_model: "base.en".into(),
            pass_score: 80.0,
            reps_new: 3,
            reps_review: 2,
            reps_weak: 5,
            new_words_per_day: 0, // 0 = follow the curriculum phase
            ytdlp_path: String::new(),
            daily_goal_minutes: 30,
            input_device: String::new(),
            output_device: String::new(),
            neural_tts: true,
            neural_voice: "af_heart".into(),
            korean_model: "small".into(),
        }
    }
}

impl Settings {
    /// The key from settings, falling back to the ANTHROPIC_API_KEY environment variable.
    pub fn effective_api_key(&self) -> Option<String> {
        if !self.api_key.trim().is_empty() {
            return Some(self.api_key.trim().to_string());
        }
        std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LevelRecord {
    pub date: NaiveDate,
    pub level: f32,
    pub cefr: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct DayProgress {
    pub date: Option<NaiveDate>,
    pub done: Vec<String>,
    pub scores: Vec<f32>,
    pub listening_passes: u32,
    pub speed_rounds: u32,
    pub study_seconds: u64,
    pub wrapped_up: bool,
    /// Time per skill today (target split 말하기 60 : 읽기 20 : 쓰기 20).
    #[serde(default)]
    pub speaking_seconds: u64,
    #[serde(default)]
    pub reading_seconds: u64,
    #[serde(default)]
    pub writing_seconds: u64,
    /// Library paragraphs read and writing exercises done today.
    #[serde(default)]
    pub paragraphs_read: u32,
    #[serde(default)]
    pub writings_done: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub start_date: NaiveDate,
    pub placement_done: bool,
    /// 1.0 (beginner) ..= 10.0 (advanced). See `cefr_of`.
    pub level: f32,
    pub level_history: Vec<LevelRecord>,
    pub streak: u32,
    pub last_study_date: Option<NaiveDate>,
    pub study_seconds: u64,
    pub days_studied: u32,
    pub today: DayProgress,
    pub last_checkpoint_week: u32,
    pub speed_history: Vec<SpeedRecord>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            start_date: today(),
            placement_done: false,
            level: 2.0,
            level_history: Vec::new(),
            streak: 0,
            last_study_date: None,
            study_seconds: 0,
            days_studied: 0,
            today: DayProgress::default(),
            last_checkpoint_week: 0,
            speed_history: Vec::new(),
        }
    }
}

/// Level names used in the UI (the same 6-step ladder Learna uses).
/// One table for every level label so the Korean name, CEFR grade and vocabulary target
/// always agree: (upper bound of the 1–10 score, name, CEFR, words a learner at this level
/// is expected to know — commonly cited CEFR vocabulary sizes).
pub const LEVEL_BANDS: &[(f32, &str, &str, u32)] = &[
    (2.0, "초급", "A1", 500),
    (3.5, "초중급", "A2", 1000),
    (5.0, "중급", "B1", 2000),
    (6.5, "중상급", "B1+", 3000),
    (8.0, "고급", "B2", 4000),
    (9.0, "고급+", "B2+", 5000),
    (f32::MAX, "능숙", "C1", 8000),
];

fn band(level: f32) -> &'static (f32, &'static str, &'static str, u32) {
    LEVEL_BANDS.iter().find(|b| level < b.0).unwrap_or(&LEVEL_BANDS[LEVEL_BANDS.len() - 1])
}

pub fn level_name_of(level: f32) -> &'static str {
    band(level).1
}

/// Vocabulary size expected at this level.
pub fn level_words(level: f32) -> u32 {
    band(level).3
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpeedRecord {
    pub date: NaiveDate,
    /// "reading" (read a passage fast) or "response" (answer a Korean prompt quickly)
    pub kind: String,
    pub wpm: f32,
    pub accuracy: f32,
    /// Response drills: seconds from prompt to finishing the answer.
    #[serde(default)]
    pub seconds: f32,
    #[serde(default)]
    pub title: String,
}

/// One day of study, kept forever in history.json so progress can be checked later.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct DayRecord {
    pub date: Option<NaiveDate>,
    pub program_day: i64,
    pub steps: Vec<String>,
    pub minutes: u32,
    pub attempts: u32,
    pub avg_score: f32,
    pub level: f32,
    pub theme: String,
    pub new_words: Vec<String>,
    pub new_sentences: u32,
    pub reviewed: u32,
    pub question: String,
    pub scenario: String,
    pub best_wpm: f32,
    #[serde(default)]
    pub speaking_minutes: u32,
    #[serde(default)]
    pub reading_minutes: u32,
    #[serde(default)]
    pub writing_minutes: u32,
    /// Books read that day ("title (page)").
    #[serde(default)]
    pub reading: Vec<String>,
    #[serde(default)]
    pub writings: u32,
}

/// A word the learner mispronounced (missed by speech recognition) during practice.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Mistake {
    pub word: String,
    pub count: u32,
    pub last: NaiveDate,
    /// Consecutive correct pronunciations in pronunciation practice; 3 clears the mistake.
    #[serde(default)]
    pub fixed_streak: u32,
}

/// A correction from Claude (self-expression feedback or role-play), for the feedback center.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CorrectionRecord {
    pub date: NaiveDate,
    pub source: String,
    pub original: String,
    pub corrected: String,
    pub explanation_ko: String,
    #[serde(default)]
    pub practiced: bool,
}

/// Place and person names are not vocabulary: "Seoul", "Daegu", "Minji", "New York".
/// Lesson and dictionary words are stored in lower case, so a capitalized entry is a name
/// (except "I"); well-known places are also caught in any case, and so are Korean meanings
/// that say it's a name.
pub fn is_proper_noun(text: &str, meaning_ko: &str) -> bool {
    const PLACES: &[&str] = &[
        "seoul", "busan", "daegu", "incheon", "gwangju", "daejeon", "ulsan", "sejong", "jeju", "suwon", "pohang", "gyeongju", "jeonju", "gangnam",
        "gyeonggi", "gangwon", "korea", "japan", "tokyo", "osaka", "china", "beijing", "shanghai", "america", "usa", "london", "paris",
        "new york", "los angeles", "california", "canada", "australia", "england", "france", "germany", "vietnam", "thailand",
    ];
    const NAME_MARKS: &[&str] = &["지명", "인명", "고유명사", "도시 이름", "나라 이름", "사람 이름", "(도시)", "(나라)", "(국가)", "(이름)", "(지역)", "(도시명)"];
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    if PLACES.contains(&lower.as_str()) || NAME_MARKS.iter().any(|m| meaning_ko.contains(m)) {
        return true;
    }
    // Every word capitalized ("Daegu", "New York", "Bank of America"); a phrase that just
    // starts with a capital ("My bad", "Netflix and chill") is an expression.
    let small = ["of", "the", "and", "de"];
    let words: Vec<&str> = t.split_whitespace().filter(|w| !small.contains(w)).collect();
    words.iter().all(|w| {
        let capitalized = w.chars().next().is_some_and(|c| c.is_uppercase());
        // "IT", "CEO": acronyms are words; "I", "I'm" are not names.
        let acronym = w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
        capitalized && !acronym && *w != "I" && !w.starts_with("I'")
    }) && !words.is_empty()
}

impl StudyItem {
    /// Counts toward the learner's vocabulary (a word or phrase, not a name).
    pub fn is_vocab_word(&self) -> bool {
        self.kind == ItemKind::Word && !is_proper_noun(&self.text, &self.meaning_ko)
    }
}

pub fn cefr_of(level: f32) -> &'static str {
    band(level).2
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Word,
    Sentence,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StudyItem {
    pub id: String,
    pub kind: ItemKind,
    pub text: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
    /// vocab | reading | self-expression | business | youtube | homework
    pub category: String,
    pub added: NaiveDate,
    pub due: NaiveDate,
    pub interval_days: u32,
    pub ease: f32,
    pub reps: u32,
    pub lapses: u32,
    pub attempts: u32,
    pub last_score: f32,
    pub best_score: f32,
    pub pass_streak: u32,
    pub weak: bool,
}

impl Default for StudyItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: ItemKind::Sentence,
            text: String::new(),
            meaning_ko: String::new(),
            example: String::new(),
            example_ko: String::new(),
            category: String::new(),
            added: today(),
            due: today(),
            interval_days: 0,
            ease: 2.5,
            reps: 0,
            lapses: 0,
            attempts: 0,
            last_score: 0.0,
            best_score: 0.0,
            pass_streak: 0,
            weak: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// Read the text aloud `reps` times; checked locally with speech recognition.
    ReadAloud,
    /// Say an original sentence using the word; checked by Claude.
    UseWord,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct HomeworkTask {
    pub kind: TaskKind,
    pub item_id: String,
    pub text: String,
    pub meaning_ko: String,
    pub instruction_ko: String,
    pub reps: u32,
    pub passed: u32,
    pub done: bool,
    pub feedback: String,
}

impl Default for HomeworkTask {
    fn default() -> Self {
        Self {
            kind: TaskKind::ReadAloud,
            item_id: String::new(),
            text: String::new(),
            meaning_ko: String::new(),
            instruction_ko: String::new(),
            reps: 5,
            passed: 0,
            done: false,
            feedback: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Homework {
    pub id: String,
    pub assigned: NaiveDate,
    pub tasks: Vec<HomeworkTask>,
    pub completed: Option<NaiveDate>,
}

impl Homework {
    pub fn progress(&self) -> (usize, usize) {
        (self.tasks.iter().filter(|t| t.done).count(), self.tasks.len())
    }
}

// ---------- Daily lesson (generated by Claude or from built-in content) ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LessonWord {
    pub word: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
}

/// One unit of 직독직해 (direct reading): English in sentence order with its Korean meaning.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Chunk {
    pub en: String,
    pub ko: String,
    /// Grammatical role (`grammar::ROLES`) for colouring; empty in glosses saved before roles.
    #[serde(default)]
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LessonSentence {
    pub text: String,
    pub ko: String,
    pub tip_ko: String,
    #[serde(default)]
    pub chunks: Vec<Chunk>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Phrase {
    pub text: String,
    pub ko: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SelfExpression {
    pub question: String,
    pub question_ko: String,
    pub useful_phrases: Vec<Phrase>,
    pub model_answer: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BusinessScenario {
    pub title: String,
    pub scenario_ko: String,
    pub ai_role: String,
    pub user_role: String,
    pub opening_line: String,
    pub key_phrases: Vec<Phrase>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ListeningPlan {
    pub search_queries: Vec<String>,
    pub focus_ko: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Lesson {
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub source: String,
    pub words: Vec<LessonWord>,
    pub sentences: Vec<LessonSentence>,
    pub self_expression: SelfExpression,
    pub business: BusinessScenario,
    pub listening: ListeningPlan,
    /// Everyday native expressions / slang of the day (one classic, one recent).
    #[serde(default)]
    pub expressions: Vec<LessonWord>,
}

/// Where the learner is in a library book.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BookProgress {
    pub page: usize,
    pub pages: usize,
    pub finished: bool,
    pub last_read: Option<NaiveDate>,
    pub paragraphs_read: u32,
}

/// One writing exercise (dictation, translation or free writing) and its result.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct WritingRecord {
    pub date: Option<NaiveDate>,
    pub kind: String,
    pub prompt: String,
    pub text: String,
    pub corrected: String,
    pub score: f32,
}
