//! Built-in 1,300 idiom sentences (`assets/idioms1300.json`, from "English Script for 1~1300")
//! for the 문장 읽기 menu: 30 new ones a day, read aloud before the day's longer sentences.

use serde::Deserialize;
use std::sync::OnceLock;

/// New idiom sentences per day (Order.md: 하루 30개).
pub const PER_DAY: usize = 30;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Idiom {
    pub no: u32,
    pub en: String,
    pub ko: String,
    /// Other ways to say it ("That's very catchy").
    #[serde(default)]
    pub also: Vec<String>,
}

/// All idioms in the book's order (parsed once, ~200 KB).
pub fn all() -> &'static [Idiom] {
    static ALL: OnceLock<Vec<Idiom>> = OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(include_str!("../assets/idioms1300.json")).unwrap_or_default())
}

pub fn by_no(no: u32) -> Option<&'static Idiom> {
    all().iter().find(|i| i.no == no)
}

/// The next `n` idioms the learner hasn't saved yet, in the book's order. After the whole book
/// (about 43 days) it starts over from the first ones.
pub fn pick(n: usize, knows: impl Fn(&str) -> bool) -> Vec<u32> {
    let mut v: Vec<u32> = all().iter().filter(|i| !knows(&i.en)).take(n).map(|i| i.no).collect();
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
    fn has_1300_idioms_in_order() {
        let a = all();
        assert_eq!(a.len(), 1300);
        assert!(a.iter().enumerate().all(|(i, x)| x.no == i as u32 + 1));
        assert!(a.iter().all(|x| !x.en.is_empty() && !x.ko.is_empty()));
        assert_eq!(by_no(1).map(|i| i.en.as_str()), Some("It's my first time"));
        assert_eq!(by_no(14).map(|i| i.ko.as_str()), Some("중독성 있네 / 귀에 감기네"));
    }

    #[test]
    fn thirty_new_a_day_then_starts_over() {
        assert_eq!(pick(PER_DAY, |_| false), (1..=30).collect::<Vec<u32>>());
        let first = by_no(1).unwrap().en.clone();
        assert_eq!(pick(3, |t| t == first), vec![2, 3, 4]);
        assert_eq!(pick(3, |_| true), vec![1, 2, 3], "after the whole book, start over");
    }
}
