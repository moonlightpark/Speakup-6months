//! 자유 회화 scenario library: 8 categories × 50 scenarios (`assets/freetalk.json`). In each
//! the app plays someone and asks 5 questions in turn; the learner answers in Japanese. With
//! Claude the questions are a plan the AI follows while reacting naturally; without it they are
//! asked one after another as a script, with a sample answer as the hint.

use crate::models::{BusinessScenario, Phrase};
use crate::offline_talk::{ScriptTurn, TalkScenario};
use serde::Deserialize;
use std::sync::OnceLock;

/// (id, Korean name, icon).
pub const CATEGORIES: [(&str, &str, &str); 8] = [
    ("daily", "일상생활", "🏠"),
    ("travel", "여행", "✈"),
    ("food", "음식·식당", "🍽"),
    ("shopping", "쇼핑·서비스", "💳"),
    ("work", "직장·업무", "💼"),
    ("hobby", "취미·여가", "🎨"),
    ("health", "건강·생활 문제", "🏥"),
    ("social", "친구·소셜·의견", "👥"),
];

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Question {
    pub q: String,
    pub q_ko: String,
    pub sample: String,
    pub sample_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FreeScenario {
    pub id: String,
    pub category: String,
    /// 1 = easy (N5–N4), 2 = N4–N3, 3 = N3–N2.
    pub level: u32,
    pub title: String,
    pub title_ko: String,
    pub situation_ko: String,
    pub ai_role: String,
    pub user_role: String,
    pub key_phrases: Vec<Phrase>,
    pub questions: Vec<Question>,
    pub closing: String,
    pub closing_ko: String,
}

pub fn all() -> &'static [FreeScenario] {
    static DATA: OnceLock<Vec<FreeScenario>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/ja/freetalk.json")).unwrap_or_default())
}

pub fn by_category(id: &str) -> Vec<&'static FreeScenario> {
    all().iter().filter(|s| s.category == id).collect()
}

pub fn by_title(title: &str) -> Option<&'static FreeScenario> {
    all().iter().find(|s| s.title == title)
}

/// The level badge for a scenario.
pub fn level_label(level: u32) -> &'static str {
    match level {
        1 => "쉬움",
        2 => "보통",
        _ => "도전",
    }
}

impl FreeScenario {
    pub fn to_business(&self) -> BusinessScenario {
        BusinessScenario {
            title: self.title.clone(),
            scenario_ko: self.situation_ko.clone(),
            ai_role: self.ai_role.clone(),
            user_role: self.user_role.clone(),
            opening_line: self.questions.first().map(|q| q.q.clone()).unwrap_or_default(),
            key_phrases: self.key_phrases.clone(),
            guide: self.questions.iter().map(|q| q.q.clone()).collect(),
        }
    }

    /// The questions as a turn-by-turn script (sample answer as the hint), then the closing line.
    pub fn to_script(&self) -> TalkScenario {
        let mut script: Vec<ScriptTurn> = self
            .questions
            .iter()
            .map(|q| ScriptTurn { ai: q.q.clone(), ai_ko: q.q_ko.clone(), replies: vec![Phrase { text: q.sample.clone(), ko: q.sample_ko.clone() }] })
            .collect();
        script.push(ScriptTurn { ai: self.closing.clone(), ai_ko: self.closing_ko.clone(), replies: Vec::new() });
        TalkScenario {
            band: self.level,
            title: self.title.clone(),
            scenario_ko: self.situation_ko.clone(),
            ai_role: self.ai_role.clone(),
            user_role: self.user_role.clone(),
            key_phrases: self.key_phrases.clone(),
            script,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_scenarios_in_each_category() {
        for (id, name, _) in CATEGORIES {
            let v = by_category(id);
            assert_eq!(v.len(), 50, "{name}");
            for s in v {
                assert!(!s.title.is_empty() && !s.title_ko.is_empty() && !s.situation_ko.is_empty() && !s.closing.is_empty(), "{}", s.id);
                assert_eq!(s.questions.len(), 5, "{}", s.id);
                assert_eq!(s.key_phrases.len(), 3, "{}", s.id);
                assert!((1..=3).contains(&s.level), "{}", s.id);
                assert!(s.questions.iter().all(|q| !q.q.is_empty() && !q.q_ko.is_empty() && !q.sample.is_empty()), "{}", s.id);
            }
        }
        let titles: std::collections::HashSet<&str> = all().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles.len(), all().len(), "titles are unique (they key the practice counts)");
    }

    #[test]
    fn a_scenario_becomes_a_plan_and_a_script() {
        let s = &by_category("daily")[0];
        let b = s.to_business();
        assert_eq!((b.opening_line.as_str(), b.guide.len()), (s.questions[0].q.as_str(), 5));
        let sc = s.to_script();
        assert_eq!(sc.script.len(), 6);
        let t = sc.turn(5, &s.questions[4].sample);
        assert!(t.finished && t.reply == s.closing);
    }
}
