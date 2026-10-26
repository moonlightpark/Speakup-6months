//! Built-in grammar lessons for the 📐 문법 menu (`grammar_book.txt`): for each level band
//! (`models::LEVEL_BANDS`) a few grammar points, each with an explanation, the pattern,
//! examples to read aloud and a short quiz. No Claude API involved.

use std::sync::OnceLock;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Example {
    pub ja: String,
    pub ko: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quiz {
    /// The sentence with `___` where the answer goes.
    pub sentence: String,
    /// Choices; the first one is right (shown shuffled).
    pub choices: Vec<String>,
    pub ko: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub title: String,
    pub form: String,
    pub explain: Vec<String>,
    pub examples: Vec<Example>,
    pub quiz: Vec<Quiz>,
}

impl Point {
    /// Key stored in `Profile::grammar_done`.
    pub fn key(&self, band: &str) -> String {
        format!("{band}:{}", self.title)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Level {
    pub band: String,
    pub points: Vec<Point>,
}

fn split2(s: &str) -> (String, String) {
    let mut it = s.splitn(2, '|');
    (it.next().unwrap_or("").trim().to_string(), it.next().unwrap_or("").trim().to_string())
}

pub fn parse(text: &str) -> Vec<Level> {
    let mut levels: Vec<Level> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || (line.starts_with('#') && !line.starts_with("## ")) {
            continue;
        }
        if let Some(band) = line.strip_prefix("@ ") {
            levels.push(Level { band: band.trim().to_string(), points: Vec::new() });
            continue;
        }
        let Some(level) = levels.last_mut() else { continue };
        if let Some(title) = line.strip_prefix("## ") {
            level.points.push(Point { title: title.trim().to_string(), ..Default::default() });
            continue;
        }
        let Some(p) = level.points.last_mut() else { continue };
        if let Some(form) = line.strip_prefix("= ") {
            p.form = form.trim().to_string();
        } else if let Some(ex) = line.strip_prefix("> ") {
            let (ja, ko) = split2(ex);
            p.examples.push(Example { ja, ko });
        } else if let Some(q) = line.strip_prefix("? ") {
            let parts: Vec<&str> = q.split('|').map(str::trim).collect();
            if parts.len() == 3 {
                let choices = parts[1].split(" / ").map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                p.quiz.push(Quiz { sentence: parts[0].to_string(), choices, ko: parts[2].to_string() });
            }
        } else {
            p.explain.push(line.to_string());
        }
    }
    levels
}

/// All levels, easiest first (parsed once).
pub fn all() -> &'static [Level] {
    static ALL: OnceLock<Vec<Level>> = OnceLock::new();
    ALL.get_or_init(|| parse(include_str!("grammar_book.txt")))
}

/// The lessons for a level band name ("N5" … "N1").
pub fn level(band: &str) -> Option<&'static Level> {
    all().iter().find(|l| l.band == band)
}

/// Display order of a question's choices: a fixed shuffle so the right answer isn't always first.
pub fn choice_order(q: &Quiz) -> Vec<usize> {
    let n = q.choices.len();
    let shift = q.sentence.bytes().map(|b| b as usize).sum::<usize>() % n.max(1);
    (0..n).map(|i| (i + shift) % n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LEVEL_BANDS;

    #[test]
    fn every_level_band_has_lessons() {
        for b in LEVEL_BANDS {
            let l = level(b.2).unwrap_or_else(|| panic!("no grammar for {}", b.2));
            assert!(l.points.len() >= 6, "{}: {} points", b.2, l.points.len());
        }
        assert_eq!(all().len(), LEVEL_BANDS.len());
    }

    #[test]
    fn points_are_complete() {
        for l in all() {
            for p in &l.points {
                assert!(!p.form.is_empty() && !p.explain.is_empty(), "{}", p.title);
                assert!(p.examples.len() >= 4, "{}", p.title);
                assert!(p.examples.iter().all(|e| !e.ja.is_empty() && !e.ko.is_empty()), "{}", p.title);
                assert!(p.quiz.len() >= 3, "{}", p.title);
                for q in &p.quiz {
                    assert!(q.sentence.contains("___"), "{}", q.sentence);
                    assert_eq!(q.choices.len(), 3, "{}", q.sentence);
                    assert!(!q.ko.is_empty(), "{}", q.sentence);
                    let mut c = q.choices.clone();
                    c.sort();
                    c.dedup();
                    assert_eq!(c.len(), 3, "{}", q.sentence);
                }
            }
        }
    }

    #[test]
    fn choice_order_is_a_permutation() {
        let q = Quiz { sentence: "私___学生です。".into(), choices: vec!["は".into(), "を".into(), "に".into()], ko: String::new() };
        let mut o = choice_order(&q);
        o.sort();
        assert_eq!(o, vec![0, 1, 2]);
    }
}
