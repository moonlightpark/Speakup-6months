//! Built-in JLPT kanji (`assets/ja/kanji.json`, 2,211 kanji N5 → N1) for the 한자 카드 in 새 단어.
//! Grades and readings: kanji-data (MIT, David Gouveia); Korean 음 checked against Unihan kHangul;
//! 훈 written once; example words picked from the built-in word database.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Kanji {
    pub k: String,
    /// JLPT grade: 5 (N5, easiest) … 1 (N1).
    pub n: u8,
    /// Korean 훈 and 음: 日 → "날", "일".
    pub hun: String,
    pub eum: String,
    /// 음독 (katakana) and 훈독 (hiragana, okurigana in brackets).
    pub on: Vec<String>,
    pub kun: Vec<String>,
    /// Example word: [word, reading, Korean meaning]; empty when none.
    pub ex: Vec<String>,
    /// Meaning in Japanese (太陽・ひる・一日).
    pub ja: String,
}

impl Kanji {
    /// "날 일".
    pub fn hun_eum(&self) -> String {
        format!("{} {}", self.hun, self.eum)
    }
    /// 読み方: "음 ニチ・ジツ  훈 ひ・か".
    pub fn readings(&self) -> String {
        let mut s = Vec::new();
        if !self.on.is_empty() {
            s.push(format!("음 {}", self.on.join("・")));
        }
        if !self.kun.is_empty() {
            s.push(format!("훈 {}", self.kun.join("・")));
        }
        s.join("  ")
    }
    /// A choice on the card: 훈음, 読み方 and the Japanese meaning, one per line.
    pub fn card_label(&self) -> String {
        format!("{}\n{}\n{}", self.hun_eum(), self.readings(), self.ja)
    }
    /// What the speaker reads: the example word, else the first reading.
    pub fn speak(&self) -> String {
        if let Some(r) = self.ex.get(1) {
            return r.clone();
        }
        self.kun.first().or(self.on.first()).map(|r| r.replace(['(', ')'], "")).unwrap_or_else(|| self.k.clone())
    }
}

/// All kanji, easiest grade first, most frequent first within a grade.
pub fn all() -> &'static [Kanji] {
    static ALL: OnceLock<Vec<Kanji>> = OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(include_str!("../assets/ja/kanji.json")).unwrap_or_default())
}

/// JLPT grade of a level label ("N3+" → 3).
pub fn grade_of(jlpt: &str) -> u8 {
    jlpt.trim_start_matches('N').chars().next().and_then(|c| c.to_digit(10)).unwrap_or(5) as u8
}

/// Indexes of the kanji a learner of this grade must know (that grade and the easier ones).
pub fn required(grade: u8) -> Vec<usize> {
    (0..all().len()).filter(|&i| all()[i].n >= grade).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jlpt_kanji_by_grade() {
        let count = |n| all().iter().filter(|k| k.n == n).count();
        assert_eq!((count(5), count(4), count(3), count(2), count(1)), (79, 166, 367, 367, 1232));
        assert!(all().windows(2).all(|p| p[0].n >= p[1].n), "N5 first");
        assert!(all().iter().all(|k| !k.hun.is_empty() && !k.eum.is_empty() && !k.ja.is_empty() && (!k.on.is_empty() || !k.kun.is_empty())));
        let day = all().iter().find(|k| k.k == "日").unwrap();
        assert_eq!((day.hun_eum().as_str(), day.n), ("날 일", 5));
        assert_eq!(day.readings(), "음 ニチ・ジツ  훈 ひ・び・か");
        assert_eq!(required(5).len(), 79);
        assert_eq!(required(4).len(), 245);
        assert_eq!(required(1).len(), 2211);
        assert_eq!((grade_of("N3+"), grade_of("N1")), (3, 1));
    }
}
