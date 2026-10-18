//! Built-in 6,000-word database (`assets/words6000.json`) for the 새 단어 menu.
//! The words and their ranks come from "필수 영어단어 Top 5,000 (1 ~ 6000)"; the Korean meanings
//! and example sentences were written once with Claude during development. Picking today's
//! words is done locally, so the menu opens instantly and works offline.

use crate::models::{LessonWord, level_words};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug)]
pub struct BankWord {
    /// Frequency rank, 1 = most important.
    pub rank: u32,
    pub word: String,
    /// Part(s) of speech in Korean ("명사·동사").
    pub pos_ko: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
    /// On the Ministry of Education's middle-school list (중등 필수 800).
    #[serde(default)]
    pub school: bool,
}

impl BankWord {
    /// Articles, prepositions, conjunctions, pronouns, auxiliaries… only: learned through
    /// sentences and chunks, not drilled alone ("the" × 3 teaches nothing).
    pub fn is_function_word(&self) -> bool {
        const FUNCTION: [&str; 7] = ["관사", "전치사", "접속사", "대명사", "조동사", "한정사", "접두사"];
        self.pos_ko.split(['·', ',', '/']).map(str::trim).all(|p| FUNCTION.contains(&p))
    }

    pub fn lesson_word(&self) -> LessonWord {
        LessonWord { word: self.word.clone(), meaning_ko: self.meaning_ko.clone(), example: self.example.clone(), example_ko: self.example_ko.clone() }
    }
}

/// All words, in rank order (parsed once, ~2 MB).
pub fn all() -> &'static [BankWord] {
    static BANK: OnceLock<Vec<BankWord>> = OnceLock::new();
    BANK.get_or_init(|| {
        let mut v: Vec<BankWord> = serde_json::from_str(include_str!("../assets/words6000.json")).unwrap_or_default();
        v.sort_by_key(|w| w.rank);
        v
    })
}

/// First rank worth teaching at this level: the words well below the learner's expected
/// vocabulary are assumed known (beginners start at rank 1).
pub fn start_rank(level: f32) -> u32 {
    level_words(level).saturating_sub(1500).min(5000) + 1
}

/// The next `n` words for this level that the learner hasn't learned yet: from `start_rank`
/// upwards, then (when those run out) the easier ones that were skipped. From A2 up, the
/// middle-school words (learned at school in Korea) count as easier too.
pub fn pick(level: f32, n: usize, knows: impl Fn(&str) -> bool) -> Vec<&'static BankWord> {
    let start = start_rank(level);
    let (easier, rest): (Vec<&BankWord>, Vec<&BankWord>) = all().iter().partition(|w| w.rank < start || (level >= 2.0 && w.school));
    rest.into_iter().chain(easier).filter(|w| !w.is_function_word() && !knows(&w.word)).take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_six_thousand_ranked_words_with_meanings() {
        let b = all();
        assert!(b.len() >= 5900, "{} words", b.len());
        assert_eq!(b[0].rank, 1);
        assert!(b.iter().all(|w| !w.word.is_empty() && !w.meaning_ko.is_empty() && !w.example.is_empty() && !w.example_ko.is_empty()));
        assert!(b.windows(2).all(|p| p[0].rank < p[1].rank), "unique ranks");
        assert!(!b.iter().any(|w| w.word == "seoul" || w.word == "Seoul"), "no place names");
    }

    #[test]
    fn picks_by_level_and_skips_known_words() {
        let beginner = pick(1.0, 5, |_| false);
        // Most frequent content words first; "be" is a verb, "the"/"of" are skipped.
        assert_eq!(beginner[0].word, "be");
        assert!(beginner.iter().all(|w| w.rank < 100 && !["the", "and", "of", "to", "a"].contains(&w.word.as_str())), "{beginner:?}");
        let advanced = pick(8.5, 5, |_| false);
        assert!(advanced[0].rank > 3000, "advanced learners skip the basics: {}", advanced[0].rank);
        // From A2, school words ("people", "go") wait until the rest is learned.
        let a2 = pick(2.5, 8, |_| false);
        assert!(a2.iter().all(|w| !w.school), "{a2:?}");
        assert!(all().iter().filter(|w| w.school).count() > 500, "중등 필수 flags present");
        let first = beginner[0].word.clone();
        let next = pick(1.0, 5, |w| w == first);
        assert_ne!(next[0].word, first);
        assert_eq!(next.len(), 5);
        // Everything above the start used up → the skipped easier words come next.
        let after = pick(8.5, 1, |w| all().iter().any(|b| b.word == w && b.rank >= start_rank(8.5)));
        assert!(after[0].rank < start_rank(8.5));
    }
}
