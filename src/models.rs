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
pub fn level_name_of(level: f32) -> &'static str {
    match level {
        l if l < 2.0 => "초급",
        l if l < 3.5 => "초중급",
        l if l < 5.0 => "중급",
        l if l < 6.5 => "중상급",
        l if l < 8.5 => "고급",
        _ => "능숙",
    }
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

pub fn cefr_of(level: f32) -> &'static str {
    match level {
        l if l < 2.0 => "A1",
        l if l < 3.5 => "A2",
        l if l < 5.0 => "B1",
        l if l < 6.5 => "B1+",
        l if l < 8.0 => "B2",
        l if l < 9.0 => "B2+",
        _ => "C1",
    }
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
}
