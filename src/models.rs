//! Persistent data structures. Everything here is serialized to JSON files in the data directory.

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Target language code (see language.rs): "ja".
    pub target_language: String,
    pub api_key: String,
    pub model: String,
    pub tts_rate: f32,
    pub tts_voice_id: String,
    pub whisper_model: String,
    pub pass_score: f32,
    pub reps_new: u32,
    pub reps_weak: u32,
    pub new_words_per_day: u32,
    pub daily_goal_minutes: u32,
    /// Microphone / speaker names; empty = system default.
    pub input_device: String,
    pub output_device: String,
    /// Multilingual whisper model for Korean speech (translator): "small" or "base". When it is
    /// the same file as `whisper_model`, one model in memory serves both languages.
    pub korean_model: String,
    /// Colour Korean passages by grammatical role (주어·동사·…). 직독직해 keeps its colours.
    pub ko_role_colors: bool,
    /// Readings (furigana) above kanji (설정 > 후리가나).
    pub furigana: bool,
    /// Speak with the natural neural voice (VOICEVOX Nemo) once it is installed.
    pub neural_tts: bool,
    /// VOICEVOX style id of the chosen Nemo voice (voicevox::VOICES).
    pub voicevox_voice: u32,
    /// The 30 → 60 minute default change has been applied (missing in older settings files).
    #[serde(default)]
    pub goal_60_applied: bool,
    /// Light screen theme (설정 > 테마); dark by default.
    #[serde(default)]
    pub light_theme: bool,
    /// The learner's own 6-month goal (JLPT grade of a level band: "N4" … "N1").
    pub goal_cefr: String,
    /// From month 3 the Korean colours were switched off once (the learner may turn them back on).
    pub ko_colors_faded: bool,
    /// Size of the Japanese words and sentences in the 새 단어 / 문장 읽기 lists (설정 > 글자 크기),
    /// big enough to copy into a notebook.
    pub list_text_size: f32,
    /// Minutes between notifications (top right); 0 = off.
    pub notify_minutes: u32,
    /// 받아쓰기 reading speed: index in `DICTATION_SPEEDS` (0 느리게, 1 보통, 2 빠르게).
    pub dictation_speed: usize,
}

impl Settings {
    pub const LIST_TEXT_SIZES: std::ops::RangeInclusive<f32> = 14.0..=40.0;

    pub fn list_size(&self) -> f32 {
        self.list_text_size.clamp(*Self::LIST_TEXT_SIZES.start(), *Self::LIST_TEXT_SIZES.end())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target_language: "ja".into(),
            api_key: String::new(),
            model: "claude-opus-5-5".into(),
            tts_rate: 1.0,
            tts_voice_id: String::new(),
            whisper_model: "small".into(),
            pass_score: 80.0,
            reps_new: 3,
            reps_weak: 5,
            new_words_per_day: 0, // 0 = 35 a day (wordbank::NEW_PER_DAY)
            // 60 min in the app (+30 min of podcasts/YouTube) ≈ 200-300 h in 6 months.
            daily_goal_minutes: 60,
            input_device: String::new(),
            output_device: String::new(),
            korean_model: "small".into(),
            ko_role_colors: true,
            furigana: true,
            neural_tts: true,
            voicevox_voice: 10005,
            goal_60_applied: true,
            light_theme: false,
            goal_cefr: "N3".into(),
            ko_colors_faded: false,
            list_text_size: 17.0,
            notify_minutes: 5,
            dictation_speed: 1,
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
    /// Time per skill today (target split 말하기 40 : 듣기 30 : 읽기 15 : 쓰기 15).
    #[serde(default)]
    pub speaking_seconds: u64,
    #[serde(default)]
    pub listening_seconds: u64,
    #[serde(default)]
    pub reading_seconds: u64,
    #[serde(default)]
    pub writing_seconds: u64,
    /// Library paragraphs read and writing exercises done today.
    #[serde(default)]
    pub paragraphs_read: u32,
    #[serde(default)]
    pub writings_done: u32,
    /// Today's 새 단어 set, picked once a day from the built-in word database (and the
    /// chunk/expression lists) so it stays the same after a restart.
    #[serde(default)]
    pub word_set: Option<WordSet>,
    /// Learned words already reviewed today in the 새 단어 menu (not offered again after a restart).
    #[serde(default)]
    pub words_reviewed: Vec<String>,
    /// Today's idiom sentences for 문장 읽기 (numbers in `assets/idioms1300.json`).
    #[serde(default)]
    pub idioms: Option<Vec<u32>>,
    /// 오늘의 문장 만들기: today's focus words (picked once), those made into a sentence, and those
    /// used while speaking (자기 표현 · 회화).
    pub focus_words: Vec<String>,
    pub sentences_made: Vec<String>,
    pub words_used: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct WordSet {
    pub words: Vec<LessonWord>,
    pub phrases: Vec<LessonWord>,
    pub expressions: Vec<LessonWord>,
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
    /// 스피드 리딩 book position: page index by book title (passages folder).
    pub speed_book_pages: std::collections::HashMap<String, usize>,
    /// One answer to the same question each week, to hear progress (주간 녹음 비교).
    pub weekly_recordings: Vec<WeeklyRecording>,
    /// Days with a lesson from a human tutor (e.g. italki), logged by the learner.
    pub tutor_sessions: Vec<NaiveDate>,
    /// Mock JLPT / JPT / OPIc 일본어 results, logged by the learner every two weeks.
    pub external_tests: Vec<ExternalTest>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct WeeklyRecording {
    pub week: u32,
    pub date: Option<NaiveDate>,
    /// WAV file name in the `recordings` folder.
    pub file: String,
    pub transcript: String,
    pub words: u32,
    pub wpm: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ExternalTest {
    pub date: Option<NaiveDate>,
    /// "JLPT", "JPT" or "OPIc 일본어"
    pub test: String,
    /// Grade or score as the learner got it (e.g. "N3 합격", "650", "IM2").
    pub result: String,
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
            weekly_recordings: Vec::new(),
            tutor_sessions: Vec::new(),
            speed_book_pages: Default::default(),
            external_tests: Vec::new(),
        }
    }
}

/// Level names used in the UI (the same 6-step ladder Learna uses).
/// One table for every level label so the Korean name, JLPT grade and vocabulary target
/// always agree: (upper bound of the 1–10 score, name, JLPT, words a learner at this level
/// is expected to know — commonly cited JLPT vocabulary sizes; "+" = between two grades).
pub const LEVEL_BANDS: &[(f32, &str, &str, u32)] = &[
    (2.0, "초급", "N5", 800),
    (3.5, "초중급", "N4", 1500),
    (5.0, "중급", "N3", 3000),
    (6.5, "중상급", "N3+", 4500),
    (8.0, "고급", "N2", 6000),
    (9.0, "고급+", "N2+", 8000),
    (f32::MAX, "능숙", "N1", 10000),
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
    pub listening_minutes: u32,
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

/// Place and person names are not vocabulary: "ソウル", "大邱", "田中", "東京". The analyser
/// marks them 固有名詞; well-known places are also caught, and so are Korean meanings that
/// say it's a name.
pub fn is_proper_noun(text: &str, meaning_ko: &str) -> bool {
    const PLACES: &[&str] = &[
        "ソウル", "釜山", "プサン", "大邱", "テグ", "仁川", "済州", "韓国", "日本", "東京", "大阪", "京都", "北海道", "沖縄", "福岡", "名古屋", "横浜",
        "中国", "アメリカ", "ニューヨーク", "ロンドン", "パリ",
    ];
    const NAME_MARKS: &[&str] = &["지명", "인명", "고유명사", "도시 이름", "나라 이름", "사람 이름", "(도시)", "(나라)", "(국가)", "(이름)", "(지역)", "(도시명)"];
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if PLACES.contains(&t) || NAME_MARKS.iter().any(|m| meaning_ko.contains(m)) {
        return true;
    }
    // One word that the dictionary knows as a name (地域 / 人名 / 組織).
    let tokens = crate::ja::tokenize(t);
    let content: Vec<&crate::ja::Token> = tokens.iter().filter(|w| !w.is_punct() && w.surface != "さん").collect();
    !content.is_empty() && content.iter().all(|w| w.pos1 == "固有名詞")
}

impl StudyItem {
    /// Counts toward the learner's vocabulary (a word or phrase, not a name).
    pub fn is_vocab_word(&self) -> bool {
        self.kind == ItemKind::Word && !is_proper_noun(&self.text, &self.meaning_ko)
    }

    /// A high-frequency spoken chunk ("It depends on"), saved as a sentence-type item.
    pub fn is_chunk(&self) -> bool {
        self.category == "phrase"
    }
}

/// Estimated cumulative study hours to reach each band (commonly cited JLPT figures for learners
/// who already read kanji, as Korean speakers largely do; a rough guide, people differ a lot),
/// same order as `LEVEL_BANDS`.
pub const CUM_HOURS: [(u32, u32); 7] = [(60, 100), (150, 200), (300, 350), (380, 450), (450, 550), (550, 650), (700, 900)];

/// What a goal level means in practice.
pub const GOAL_DESC: [&str; 7] = [
    "자기소개·간단한 질문에 짧게 답하기",
    "익숙한 일상 주제로 짧게 묻고 답하기",
    "일상·업무 주제로 10분 동안 막히지 않고 대화하기",
    "낯선 주제도 의견과 이유를 길게, 막힘을 줄여 말하기",
    "원어민과 업무 회의·토론을 자연스럽게 하기",
    "복잡한 협상·발표를 유창하게 하기",
    "어떤 주제든 정확하고 유연하게 말하기",
];

/// Speech speed that suits a level (× the normal voice speed): slower for beginners, natural
/// from about N2 — level 1 → 0.72, level 3 → 0.80, level 5 → 0.88, level 8+ → 1.0.
pub fn level_speech_rate(level: f32) -> f32 {
    (0.68 + 0.04 * level).clamp(0.72, 1.0)
}

/// 받아쓰기 speeds over the level's pace: 느리게 = the level's, 보통 = 3% faster (a little
/// stretch, so it isn't too easy), 빠르게 = 15% faster.
pub const DICTATION_SPEEDS: [(&str, f32); 3] = [("느리게", 1.0), ("보통", 1.03), ("빠르게", 1.15)];

/// The 받아쓰기 speed for a level and the chosen setting.
pub fn dictation_rate(level: f32, setting: usize) -> f32 {
    level_speech_rate(level) * DICTATION_SPEEDS[setting.min(DICTATION_SPEEDS.len() - 1)].1
}

/// Index in `LEVEL_BANDS` of a level score, or of a JLPT name.
pub fn band_index(level: f32) -> usize {
    LEVEL_BANDS.iter().position(|b| level < b.0).unwrap_or(LEVEL_BANDS.len() - 1)
}

pub fn band_index_of_cefr(cefr: &str) -> Option<usize> {
    LEVEL_BANDS.iter().position(|b| b.2 == cefr)
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
    /// vocab | phrase | expression | reading | self-expression | business | youtube | homework
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
    /// Marked 학습완료 as too easy: counts as learned but never comes up for review.
    pub known: bool,
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
            known: false,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LessonWord {
    pub word: String,
    /// Reading in hiragana (shown as furigana; empty = worked out by the analyser).
    #[serde(default)]
    pub reading: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
}

/// One unit of 직독직해 (direct reading): a Japanese 문절 in sentence order with its Korean
/// meaning (Japanese and Korean share word order, so the meanings read as a Korean sentence).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Chunk {
    pub ja: String,
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
    /// Questions the AI asks in turn (자유 회화 scenarios); empty = free conversation.
    #[serde(default)]
    pub guide: Vec<String>,
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
    /// The day's native expression or slang (one a day, classic and recent in turn).
    #[serde(default)]
    pub expressions: Vec<LessonWord>,
    /// High-frequency spoken chunks ("I was wondering if", "It depends on") — learned whole.
    #[serde(default)]
    pub phrases: Vec<LessonWord>,
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

/// Practice record of one 새 단어 entry (word, chunk or expression), kept in word_stats.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct WordStat {
    /// Passing repetitions so far (all days together).
    pub passes: u32,
    pub attempts: u32,
    pub last: Option<NaiveDate>,
}

/// Repetitions after which a word counts as memorized (기억 완료).
pub const WORD_DONE_PASSES: u32 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_speed_follows_the_level() {
        assert!(level_speech_rate(1.0) < level_speech_rate(5.0) && level_speech_rate(5.0) < level_speech_rate(8.0));
        assert_eq!(level_speech_rate(10.0), 1.0);
        let base = level_speech_rate(3.0);
        assert!((dictation_rate(3.0, 0) - base).abs() < 1e-6, "느리게 = the level's pace");
        assert!((dictation_rate(3.0, 1) - base * 1.03).abs() < 1e-6, "보통 = +3%");
        assert!((dictation_rate(3.0, 2) - base * 1.15).abs() < 1e-6, "빠르게 = +15%");
        assert_eq!(Settings::default().dictation_speed, 1, "보통 by default");
    }
}
