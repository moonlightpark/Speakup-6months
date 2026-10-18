//! Built-in speaking content for when the Claude API can't be used (no key, no credit, no
//! network): self-expression questions with follow-up questions, and scripted conversations
//! where the app plays the other person turn by turn. Written for four level bands
//! (`assets/talk_offline.json`, 30 questions + 15 conversations each).

use crate::claude::RoleplayTurn;
use crate::models::{BusinessScenario, Phrase, SelfExpression};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FollowUp {
    pub question: String,
    pub question_ko: String,
    pub sample_answer: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TalkQuestion {
    pub band: u32,
    pub question: String,
    pub question_ko: String,
    pub useful_phrases: Vec<Phrase>,
    pub model_answer: String,
    pub model_answer_ko: String,
    pub follow_ups: Vec<FollowUp>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ScriptTurn {
    pub ai: String,
    pub ai_ko: String,
    /// Things the learner could answer (shown as hints, and the model for a reply far off).
    pub replies: Vec<Phrase>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TalkScenario {
    pub band: u32,
    pub title: String,
    pub scenario_ko: String,
    pub ai_role: String,
    pub user_role: String,
    pub key_phrases: Vec<Phrase>,
    pub script: Vec<ScriptTurn>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Data {
    questions: Vec<TalkQuestion>,
    scenarios: Vec<TalkScenario>,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/talk_offline.json")).unwrap_or_default())
}

/// Level band of the content: A1 → 0, A2 → 1, B1–B1+ → 2, B2 and up → 3.
pub fn band_for(level: f32) -> u32 {
    match level {
        l if l < 2.0 => 0,
        l if l < 3.5 => 1,
        l if l < 6.5 => 2,
        _ => 3,
    }
}

pub fn questions(level: f32) -> Vec<&'static TalkQuestion> {
    let b = band_for(level);
    data().questions.iter().filter(|q| q.band == b).collect()
}

pub fn scenarios(level: f32) -> Vec<&'static TalkScenario> {
    let b = band_for(level);
    data().scenarios.iter().filter(|s| s.band == b).collect()
}

pub fn question_by_text(q: &str) -> Option<&'static TalkQuestion> {
    data().questions.iter().find(|x| x.question == q)
}

pub fn scenario_by_title(t: &str) -> Option<&'static TalkScenario> {
    data().scenarios.iter().find(|x| x.title == t)
}

/// A question for the level not used yet (else the day's one in turn).
pub fn pick_question(level: f32, used: impl Fn(&str) -> bool, day: usize) -> Option<&'static TalkQuestion> {
    let qs = questions(level);
    qs.iter().find(|q| !used(&q.question)).or(qs.get(day % qs.len().max(1))).copied()
}

pub fn pick_scenario(level: f32, used: impl Fn(&str) -> bool, day: usize) -> Option<&'static TalkScenario> {
    let ss = scenarios(level);
    ss.iter().find(|s| !used(&s.title)).or(ss.get(day % ss.len().max(1))).copied()
}

impl TalkQuestion {
    pub fn to_self_expression(&self) -> SelfExpression {
        SelfExpression {
            question: self.question.clone(),
            question_ko: self.question_ko.clone(),
            useful_phrases: self.useful_phrases.clone(),
            model_answer: self.model_answer.clone(),
        }
    }
}

impl TalkScenario {
    pub fn to_business(&self) -> BusinessScenario {
        BusinessScenario {
            title: self.title.clone(),
            scenario_ko: self.scenario_ko.clone(),
            ai_role: self.ai_role.clone(),
            user_role: self.user_role.clone(),
            opening_line: self.script.first().map(|t| t.ai.clone()).unwrap_or_default(),
            key_phrases: self.key_phrases.clone(),
            guide: Vec::new(),
        }
    }

    /// The app's next line after the learner's `user_turns`-th reply (`said`). A reply far
    /// from every example gets the closest example as a hint.
    pub fn turn(&self, user_turns: usize, said: &str) -> RoleplayTurn {
        let last = self.script.len().saturating_sub(1);
        let k = user_turns.min(last);
        let next = &self.script[k];
        let (correction, correction_ko) = self
            .script
            .get(user_turns.saturating_sub(1))
            .and_then(|asked| {
                asked
                    .replies
                    .iter()
                    .map(|r| (crate::scoring::score(&r.text, said).score, r))
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .filter(|(s, _)| *s < 40.0)
                    .map(|(_, r)| (r.text.clone(), format!("💡 이렇게 말할 수도 있어요 — {}", r.ko)))
            })
            .unwrap_or_default();
        let finished = k >= last;
        RoleplayTurn {
            reply: next.ai.clone(),
            reply_ko: next.ai_ko.clone(),
            correction,
            correction_ko,
            suggestions: next.replies.clone(),
            finished,
            summary_ko: if finished { "대화를 끝까지 마쳤어요! 💡 예시 문장들을 따라 말해 보면 다음엔 더 자연스러워져요.".into() } else { String::new() },
        }
    }
}

/// Spoken length a band's answer should reach (words), for the offline feedback.
pub fn target_words(level: f32) -> usize {
    [12, 25, 45, 70][band_for(level) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_for_every_band() {
        for (level, band) in [(1.0, 0), (3.0, 1), (5.0, 2), (8.0, 3)] {
            assert_eq!(band_for(level), band);
            let qs = questions(level);
            let ss = scenarios(level);
            assert!(qs.len() >= 30, "band {band}: {} questions", qs.len());
            assert!(ss.len() >= 15, "band {band}: {} scenarios", ss.len());
            for q in qs {
                assert!(!q.question.is_empty() && !q.question_ko.is_empty() && !q.model_answer.is_empty(), "{q:?}");
                assert_eq!(q.useful_phrases.len(), 3, "{}", q.question);
                assert_eq!(q.follow_ups.len(), 3, "{}", q.question);
            }
            for s in ss {
                assert!(s.script.len() >= 4 && !s.title.is_empty() && s.key_phrases.len() == 3, "{}", s.title);
                for t in &s.script[..s.script.len() - 1] {
                    assert!(!t.ai.is_empty() && !t.ai_ko.is_empty() && t.replies.len() >= 2, "{}: {}", s.title, t.ai);
                }
            }
        }
        let titles: std::collections::HashSet<&str> = data().scenarios.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles.len(), data().scenarios.len(), "scenario titles are unique");
    }

    #[test]
    fn a_scripted_conversation_runs_to_its_end() {
        let s = scenarios(3.0)[0];
        let n = s.script.len();
        let t1 = s.turn(1, &s.script[0].replies[0].text);
        assert_eq!(t1.reply, s.script[1].ai);
        assert!(t1.correction.is_empty(), "an example reply needs no hint");
        let off = s.turn(1, "banana");
        assert_eq!(off.correction, s.script[0].replies.iter().map(|r| r.text.clone()).find(|t| *t == off.correction).unwrap(), "hint = an example");
        let end = s.turn(n - 1, "Thanks");
        assert!(end.finished && end.reply == s.script[n - 1].ai);
    }
}
