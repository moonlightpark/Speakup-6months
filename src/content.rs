//! Built-in Japanese content used for the placement test and as an offline fallback when
//! Claude is unavailable (no API key / network error): `assets/ja/misc.json`, written once
//! during development. Claude-generated lessons are preferred.

use crate::models::*;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Misc {
    placement: Vec<(String, String)>,
    placement_question: (String, String),
    sentences: Vec<Vec<MiscSentence>>,
    questions: Vec<Vec<MiscQuestion>>,
    scenarios: Vec<Vec<MiscScenario>>,
    roleplays: Vec<MiscRoleplay>,
    speed_passages: Vec<MiscPassage>,
    pub classic: Vec<MiscExpr>,
    pub recent: Vec<MiscExpr>,
    pub chunks: Vec<MiscExpr>,
    pub starters: Vec<[String; 3]>,
}

#[derive(Deserialize, Default)]
struct MiscSentence {
    text: String,
    ko: String,
    tip_ko: String,
}

#[derive(Deserialize, Default)]
struct MiscQuestion {
    question: String,
    question_ko: String,
    model_answer: String,
}

#[derive(Deserialize, Default)]
struct MiscScenario {
    title: String,
    scenario_ko: String,
    ai_role: String,
    user_role: String,
    opening_line: String,
    key_phrases: Vec<Phrase>,
}

#[derive(Deserialize, Default)]
struct MiscRoleplay {
    title: String,
    scenario_ko: String,
    ai_role: String,
    user_role: String,
}

#[derive(Deserialize, Default)]
struct MiscPassage {
    title: String,
    text: String,
}

#[derive(Deserialize, Default)]
pub struct MiscExpr {
    pub expr: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
}

/// The whole file, parsed once.
pub fn misc() -> &'static Misc {
    static DATA: OnceLock<Misc> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/ja/misc.json")).unwrap_or_default())
}

type Pair = (&'static str, &'static str);

/// Placement / checkpoint reading sentences, easiest first (roughly N5 → N1).
pub fn placement() -> &'static [Pair] {
    static V: OnceLock<Vec<Pair>> = OnceLock::new();
    V.get_or_init(|| misc().placement.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect())
}

pub fn placement_question() -> Pair {
    let (q, ko) = &misc().placement_question;
    (q.as_str(), ko.as_str())
}

/// Role-play collection (like Learna's scene collections): (title, situation, AI role, learner
/// role). The AI opens each scene.
pub fn roleplays() -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    static V: OnceLock<Vec<(&'static str, &'static str, &'static str, &'static str)>> = OnceLock::new();
    V.get_or_init(|| misc().roleplays.iter().map(|r| (r.title.as_str(), r.scenario_ko.as_str(), r.ai_role.as_str(), r.user_role.as_str())).collect())
}

/// Offline passages for speed reading (used when Claude isn't available): (title, text).
pub fn speed_passages() -> &'static [Pair] {
    static V: OnceLock<Vec<Pair>> = OnceLock::new();
    V.get_or_init(|| misc().speed_passages.iter().map(|p| (p.title.as_str(), p.text.as_str())).collect())
}

fn phase_index(week: u32) -> usize {
    match week {
        1..=6 => 0,
        7..=12 => 1,
        13..=19 => 2,
        _ => 3,
    }
}

/// High-frequency spoken chunks per lesson.
pub const CHUNKS_PER_DAY: usize = 4;

/// Builds an offline lesson for the given week, skipping words/sentences the learner already has.
pub fn builtin_lesson(week: u32, day: i64, known: &[StudyItem], past: &[Lesson], n_words: usize, n_sent: usize) -> Lesson {
    let m = misc();
    let p = phase_index(week);
    let knows = |t: &str| known.iter().any(|i| i.text == t);
    // The day's words come from the word database (levelled by JLPT grade).
    let level = (p as f32) * 2.0 + 1.0;
    let words: Vec<LessonWord> = crate::wordbank::pick(level, n_words, knows).into_iter().map(|w| w.lesson_word()).collect();
    // Fall back to earlier phases once a phase's pool is used up.
    let sentences: Vec<LessonSentence> = (0..=p)
        .rev()
        .flat_map(|i| m.sentences.get(i).into_iter().flatten())
        .filter(|s| !knows(&s.text))
        .take(n_sent)
        .map(|s| LessonSentence { text: s.text.clone(), ko: s.ko.clone(), tip_ko: s.tip_ko.clone(), chunks: Vec::new() })
        .collect();
    // Prefer a question / scenario that hasn't been used yet (any phase), then rotate.
    let used_q = |q: &str| past.iter().any(|l| l.self_expression.question == q);
    let used_s = |s: &str| past.iter().any(|l| l.business.title == s);
    let rotate = |k: usize| (p + k) % 4;
    let q = (0..4).flat_map(|k| m.questions.get(rotate(k)).into_iter().flatten()).find(|q| !used_q(&q.question)).or_else(|| {
        let pool = m.questions.get(p)?;
        pool.get(day as usize % pool.len().max(1))
    });
    let sc = (0..4).flat_map(|k| m.scenarios.get(rotate(k)).into_iter().flatten()).find(|s| !used_s(&s.title)).or_else(|| {
        let pool = m.scenarios.get(p)?;
        pool.get(day as usize % pool.len().max(1))
    });
    let wk = crate::curriculum::week(week);
    Lesson {
        date: Some(today()),
        source: "built-in".into(),
        words,
        sentences,
        self_expression: q
            .map(|q| SelfExpression { question: q.question.clone(), question_ko: q.question_ko.clone(), useful_phrases: Vec::new(), model_answer: q.model_answer.clone() })
            .unwrap_or_default(),
        business: sc
            .map(|sc| BusinessScenario {
                title: sc.title.clone(),
                scenario_ko: sc.scenario_ko.clone(),
                ai_role: sc.ai_role.clone(),
                user_role: sc.user_role.clone(),
                opening_line: sc.opening_line.clone(),
                key_phrases: sc.key_phrases.clone(),
                guide: Vec::new(),
            })
            .unwrap_or_default(),
        listening: ListeningPlan {
            search_queries: vec![wk.listening.into()],
            focus_ko: "자막 한 줄씩 듣고 바로 따라 말하세요(쉐도잉).".into(),
        },
        expressions: crate::expressions::pick_for_today(day, knows)
            .into_iter()
            .map(|e| LessonWord { word: e.0.into(), reading: String::new(), meaning_ko: e.1.into(), example: e.2.into(), example_ko: e.3.into() })
            .collect(),
        phrases: crate::expressions::pick_chunks(CHUNKS_PER_DAY, knows)
            .into_iter()
            .map(|e| LessonWord { word: e.0.into(), reading: String::new(), meaning_ko: e.1.into(), example: e.2.into(), example_ko: e.3.into() })
            .collect(),
    }
}

