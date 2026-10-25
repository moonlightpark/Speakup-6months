//! Built-in Japanese word database (`assets/ja/words.json`, about 6,000 words, JLPT N5 → N1) for
//! the 새 단어 menu. The list, readings, Korean meanings and example sentences were written once
//! with Claude during development, ordered by JLPT grade and, within a grade, by how useful the
//! word is in speech. Picking today's words is done locally, so the menu opens instantly and
//! works offline.

use crate::models::{LessonWord, level_words};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug)]
pub struct BankWord {
    /// Frequency rank, 1 = most important.
    pub rank: u32,
    pub word: String,
    /// Reading in hiragana (katakana words as written).
    pub reading: String,
    /// Part(s) of speech in Korean ("명사·동사", "い형용사").
    pub pos_ko: String,
    pub meaning_ko: String,
    pub example: String,
    pub example_ko: String,
    /// JLPT grade: 5 (N5, easiest) … 1 (N1).
    pub jlpt: u8,
}

impl BankWord {
    /// Particles, auxiliaries, affixes… only: learned through sentences and chunks, not drilled
    /// alone ("は" × 3 teaches nothing).
    pub fn is_function_word(&self) -> bool {
        const FUNCTION: [&str; 5] = ["조사", "조동사", "접두사", "접미사", "관사"];
        self.pos_ko.split(['·', ',', '/']).map(str::trim).all(|p| FUNCTION.contains(&p))
    }

    pub fn lesson_word(&self) -> LessonWord {
        LessonWord { word: self.word.clone(), reading: self.reading.clone(), meaning_ko: self.meaning_ko.clone(), example: self.example.clone(), example_ko: self.example_ko.clone() }
    }
}

/// All words, in rank order (parsed once, ~2 MB).
pub fn all() -> &'static [BankWord] {
    static BANK: OnceLock<Vec<BankWord>> = OnceLock::new();
    BANK.get_or_init(|| {
        let mut v: Vec<BankWord> = serde_json::from_str(include_str!("../assets/ja/words.json")).unwrap_or_default();
        v.sort_by_key(|w| w.rank);
        v
    })
}

/// New words a day: 35 × 182 days ≈ the whole word list in six months.
pub const NEW_PER_DAY: usize = 35;
/// Learned words reviewed a day in the 새 단어 menu.
pub const REVIEW_PER_DAY: usize = 50;

/// First rank worth teaching at this level: the words well below the learner's expected
/// vocabulary are assumed known (beginners start at rank 1).
pub fn start_rank(level: f32) -> u32 {
    level_words(level).saturating_sub(1500).min(5000) + 1
}

/// The next `n` words for this level that the learner hasn't learned yet: from `start_rank`
/// upwards, then (when those run out) the easier ones that were skipped.
pub fn pick(level: f32, n: usize, knows: impl Fn(&str) -> bool) -> Vec<&'static BankWord> {
    let start = start_rank(level);
    let (easier, rest): (Vec<&BankWord>, Vec<&BankWord>) = all().iter().partition(|w| w.rank < start);
    rest.into_iter().chain(easier).filter(|w| !w.is_function_word() && !knows(&w.word)).take(n).collect()
}

/// A word by its written form or its reading.
pub fn find(word: &str) -> Option<&'static BankWord> {
    all().iter().find(|w| w.word == word).or_else(|| all().iter().find(|w| w.reading == word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_six_thousand_ranked_words_with_meanings() {
        let b = all();
        assert!(b.len() >= 5500, "{} words", b.len());
        assert_eq!(b[0].rank, 1);
        assert!(b.iter().all(|w| !w.word.is_empty() && !w.reading.is_empty() && !w.meaning_ko.is_empty() && !w.example.is_empty() && !w.example_ko.is_empty()));
        assert!(b.windows(2).all(|p| p[0].rank < p[1].rank), "unique ranks");
        assert!(b.windows(2).all(|p| p[0].jlpt >= p[1].jlpt), "N5 first, N1 last");
        assert!(b.iter().all(|w| w.reading.chars().all(|c| crate::ja::is_kana(c) || "・〜～".contains(c))), "readings are kana");
        assert!(!b.iter().any(|w| w.word == "ソウル" || w.word == "東京"), "no place names");
    }

    #[test]
    fn picks_by_level_and_skips_known_words() {
        let beginner = pick(1.0, 5, |_| false);
        assert!(beginner.iter().all(|w| w.jlpt == 5), "{beginner:?}");
        let advanced = pick(8.5, 5, |_| false);
        assert!(advanced[0].rank > 3000, "advanced learners skip the basics: {}", advanced[0].rank);
        let first = beginner[0].word.clone();
        let next = pick(1.0, 5, |w| w == first);
        assert_ne!(next[0].word, first);
        assert_eq!(next.len(), 5);
        // Everything above the start used up → the skipped easier words come next.
        let after = pick(8.5, 1, |w| all().iter().any(|b| b.word == w && b.rank >= start_rank(8.5)));
        assert!(after[0].rank < start_rank(8.5));
    }
}
