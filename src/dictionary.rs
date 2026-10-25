//! Japanese–Korean dictionary lookups from the built-in word database (`assets/ja/words.json`),
//! without the Claude API. Words in a text are conjugated (行きました, 食べたい, 高かった), so a
//! lookup also tries the dictionary form the analyser (ja.rs) gives, and the reading.

use crate::wordbank::{self, BankWord};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Headword → index in `wordbank::all()` (the written form first, readings after).
fn index() -> &'static HashMap<String, usize> {
    static INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, w) in wordbank::all().iter().enumerate() {
            m.entry(w.word.clone()).or_insert(i);
        }
        for (i, w) in wordbank::all().iter().enumerate() {
            m.entry(w.reading.clone()).or_insert(i);
        }
        m
    })
}

fn get(key: &str) -> Option<&'static BankWord> {
    index().get(key).map(|i| &wordbank::all()[*i])
}

/// Dictionary entry for a word as it appears in a text, and whether it was found under its
/// dictionary form (an inflected form was given).
pub fn lookup(word: &str) -> Option<(&'static BankWord, bool)> {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric());
    if w.is_empty() {
        return None;
    }
    if let Some(b) = get(w) {
        return Some((b, false));
    }
    // The dictionary form of the (first content) word: 行きました → 行く.
    let tokens = crate::ja::tokenize(w);
    let head = tokens.iter().find(|t| t.is_content())?;
    let candidates = [head.base.clone(), crate::ja::to_hiragana(&head.base), head.reading.clone()];
    candidates.iter().filter(|c| !c.is_empty()).find_map(|c| get(c)).map(|b| (b, true))
}

/// Dictionary forms of a sentence's words, and their readings.
fn forms(sentence: &str) -> Vec<String> {
    crate::ja::tokenize(&crate::ja::digits_to_kanji(sentence))
        .into_iter()
        .filter(|t| !t.is_punct())
        .flat_map(|t| {
            let reading_base = crate::ja::reading(&t.base);
            [t.base, reading_base]
        })
        .collect()
}

/// Whether `token` is `target` or a form of it (行った → 行く, 高くて → 高い).
pub fn same_word(token: &str, target: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    if t.is_empty() {
        return false;
    }
    t == target || forms(t).iter().any(|f| f == target || *f == crate::ja::reading(target))
}

/// Whether a spoken sentence uses `target` (a word in any form, or a whole phrase). The
/// recognizer may write a word in kana or other kanji, so the sounds are compared too.
pub fn sentence_uses(sentence: &str, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() || sentence.trim().is_empty() {
        return false;
    }
    let compact: String = sentence.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains(target) {
        return true;
    }
    let target_tokens: Vec<crate::ja::Token> = crate::ja::tokenize(target).into_iter().filter(|t| !t.is_punct()).collect();
    if target_tokens.len() == 1 {
        let t = &target_tokens[0];
        let said = forms(sentence);
        if said.iter().any(|f| *f == t.base || *f == crate::ja::reading(&t.base)) {
            return true;
        }
    }
    crate::scoring::contains_phrase(sentence, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(w: &str) -> Option<String> {
        lookup(w).map(|(b, _)| b.word.clone())
    }

    #[test]
    fn finds_words_and_their_dictionary_forms() {
        assert_eq!(head("食べる"), Some("食べる".into()));
        assert_eq!(lookup("食べる").map(|x| x.1), Some(false));
        assert_eq!(head("「食べる」"), Some("食べる".into()), "punctuation");
        for (form, base) in [("食べました", "食べる"), ("行きたい", "行く"), ("高かった", "高い"), ("見て", "見る")] {
            assert_eq!(head(form), Some(base.to_string()), "{form} → {base}");
            assert_eq!(lookup(form).map(|x| x.1), Some(true), "{form} is an inflected form");
        }
        assert_eq!(head("たべる"), Some("食べる".into()), "by reading");
        assert!(lookup("食べる").is_some_and(|(b, _)| !b.meaning_ko.is_empty() && !b.example.is_empty()));
    }

    #[test]
    fn finds_the_word_in_a_sentence() {
        assert!(sentence_uses("明日、映画を見に行きます", "行く"));
        assert!(sentence_uses("きのう友達とあそびました", "遊ぶ"));
        assert!(sentence_uses("締め切りは金曜日です", "締め切り"));
        assert!(sentence_uses("よろしくお願いします", "お願いします"));
        assert!(!sentence_uses("わかりません", "締め切り"));
        assert!(!sentence_uses("", "行く"));
        assert!(same_word("行った", "行く") && !same_word("来た", "行く"));
    }
}
