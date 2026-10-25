//! Built-in book of about 1,300 short everyday Japanese lines (`assets/ja/lines.json`, written
//! once during development: greetings, reactions, daily life, work, travel, slang …) for the
//! 문장 읽기 menu: 30 new ones a day, read aloud before the day's longer sentences.

use serde::Deserialize;
use std::sync::OnceLock;

/// New idiom sentences per day (Order.md: 하루 30개).
pub const PER_DAY: usize = 30;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Idiom {
    pub no: u32,
    pub ja: String,
    pub ko: String,
    /// Other ways to say it.
    #[serde(default)]
    pub also: Vec<String>,
}

/// All lines in the book's order (parsed once, ~200 KB).
pub fn all() -> &'static [Idiom] {
    static ALL: OnceLock<Vec<Idiom>> = OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(include_str!("../assets/ja/lines.json")).unwrap_or_default())
}

pub fn by_no(no: u32) -> Option<&'static Idiom> {
    all().iter().find(|i| i.no == no)
}

/// The next `n` lines the learner hasn't saved yet, in the book's order. After the whole book
/// (about 43 days) it starts over from the first ones.
pub fn pick(n: usize, knows: impl Fn(&str) -> bool) -> Vec<u32> {
    let mut v: Vec<u32> = all().iter().filter(|i| !knows(&i.ja)).take(n).map(|i| i.no).collect();
    if v.len() < n {
        let more: Vec<u32> = all().iter().map(|i| i.no).filter(|no| !v.contains(no)).take(n - v.len()).collect();
        v.extend(more);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_about_1300_lines_in_order() {
        let a = all();
        assert!(a.len() >= 1250, "{}", a.len());
        assert!(a.iter().enumerate().all(|(i, x)| x.no == i as u32 + 1));
        assert!(a.iter().all(|x| crate::ja::is_japanese(&x.ja) && !x.ko.is_empty()));
        let unique: std::collections::HashSet<&str> = a.iter().map(|x| x.ja.as_str()).collect();
        assert_eq!(unique.len(), a.len(), "no line twice");
    }

    #[test]
    fn thirty_new_a_day_then_starts_over() {
        assert_eq!(pick(PER_DAY, |_| false), (1..=30).collect::<Vec<u32>>());
        let first = by_no(1).unwrap().ja.clone();
        assert_eq!(pick(3, |t| t == first), vec![2, 3, 4]);
        assert_eq!(pick(3, |_| true), vec![1, 2, 3], "after the whole book, start over");
    }
}
