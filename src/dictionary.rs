//! English–Korean dictionary lookups from the built-in 6,000-word database
//! (`assets/words6000.json`), without the Claude API. Words in a text are inflected
//! ("went", "cities", "running"), so a lookup also tries their base forms.

use crate::wordbank::{self, BankWord};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Lower-case headword → index in `wordbank::all()`.
fn index() -> &'static HashMap<String, usize> {
    static INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();
    INDEX.get_or_init(|| wordbank::all().iter().enumerate().map(|(i, w)| (w.word.to_lowercase(), i)).collect())
}

/// Irregular past forms, participles and plurals → base form.
const IRREGULAR: &[(&str, &str)] = &[
    ("am", "be"), ("is", "be"), ("are", "be"), ("was", "be"), ("were", "be"), ("been", "be"), ("being", "be"),
    ("has", "have"), ("had", "have"), ("does", "do"), ("did", "do"), ("done", "do"),
    ("went", "go"), ("gone", "go"), ("goes", "go"), ("got", "get"), ("gotten", "get"), ("made", "make"),
    ("said", "say"), ("says", "say"), ("saw", "see"), ("seen", "see"), ("came", "come"), ("took", "take"),
    ("taken", "take"), ("knew", "know"), ("known", "know"), ("thought", "think"), ("told", "tell"),
    ("found", "find"), ("gave", "give"), ("given", "give"), ("felt", "feel"), ("left", "leave"),
    ("kept", "keep"), ("began", "begin"), ("begun", "begin"), ("brought", "bring"), ("bought", "buy"),
    ("ran", "run"), ("sat", "sit"), ("stood", "stand"), ("heard", "hear"), ("held", "hold"), ("met", "meet"),
    ("paid", "pay"), ("sent", "send"), ("spent", "spend"), ("built", "build"), ("lost", "lose"),
    ("meant", "mean"), ("wrote", "write"), ("written", "write"), ("spoke", "speak"), ("spoken", "speak"),
    ("broke", "break"), ("broken", "break"), ("chose", "choose"), ("chosen", "choose"), ("drove", "drive"),
    ("driven", "drive"), ("ate", "eat"), ("eaten", "eat"), ("fell", "fall"), ("fallen", "fall"),
    ("flew", "fly"), ("flown", "fly"), ("forgot", "forget"), ("forgotten", "forget"), ("grew", "grow"),
    ("grown", "grow"), ("hid", "hide"), ("hidden", "hide"), ("led", "lead"), ("lay", "lie"), ("lain", "lie"),
    ("rode", "ride"), ("ridden", "ride"), ("rose", "rise"), ("risen", "rise"), ("sang", "sing"), ("sung", "sing"),
    ("sank", "sink"), ("slept", "sleep"), ("sold", "sell"), ("shook", "shake"), ("shot", "shoot"),
    ("showed", "show"), ("shown", "show"), ("stole", "steal"), ("stolen", "steal"), ("struck", "strike"),
    ("swam", "swim"), ("taught", "teach"), ("threw", "throw"), ("thrown", "throw"), ("understood", "understand"),
    ("woke", "wake"), ("woken", "wake"), ("wore", "wear"), ("worn", "wear"), ("won", "win"), ("caught", "catch"),
    ("fought", "fight"), ("fed", "feed"), ("drew", "draw"), ("drawn", "draw"), ("drank", "drink"),
    ("drunk", "drink"), ("forgave", "forgive"), ("froze", "freeze"), ("frozen", "freeze"), ("hung", "hang"),
    ("lent", "lend"), ("lit", "light"), ("rang", "ring"), ("rung", "ring"), ("sought", "seek"), ("shone", "shine"),
    ("stuck", "stick"), ("stung", "sting"), ("swore", "swear"), ("tore", "tear"), ("torn", "tear"),
    ("wept", "weep"), ("bent", "bend"), ("bit", "bite"), ("bitten", "bite"), ("blew", "blow"), ("blown", "blow"),
    ("bred", "breed"), ("clung", "cling"), ("crept", "creep"), ("dealt", "deal"), ("dug", "dig"),
    ("dreamt", "dream"), ("fled", "flee"), ("knelt", "kneel"), ("leapt", "leap"), ("slid", "slide"),
    ("spun", "spin"), ("sprang", "spring"), ("swept", "sweep"), ("swung", "swing"), ("wound", "wind"),
    ("children", "child"), ("men", "man"), ("women", "woman"), ("people", "person"), ("feet", "foot"),
    ("teeth", "tooth"), ("mice", "mouse"), ("geese", "goose"), ("lives", "life"), ("wives", "wife"),
    ("knives", "knife"), ("leaves", "leaf"), ("wolves", "wolf"), ("halves", "half"), ("shelves", "shelf"),
    ("better", "good"), ("best", "good"), ("worse", "bad"), ("worst", "bad"), ("me", "I"), ("my", "I"),
    ("him", "he"), ("his", "he"), ("her", "she"), ("us", "we"), ("our", "we"), ("them", "they"), ("their", "they"),
];

/// Candidate base forms of an inflected word, most likely first.
fn base_forms(w: &str) -> Vec<String> {
    let mut v = Vec::new();
    if let Some((_, base)) = IRREGULAR.iter().find(|(f, _)| *f == w) {
        v.push(base.to_string());
    }
    let strip = |suf: &str| w.strip_suffix(suf).filter(|s| s.len() >= 2);
    // cities → city, carried → carry
    for suf in ["ies", "ied"] {
        if let Some(s) = strip(suf) {
            v.push(format!("{s}y"));
        }
    }
    // bigger/biggest/running/stopped → big/run/stop (doubled consonant)
    for suf in ["ing", "ed", "er", "est"] {
        if let Some(s) = strip(suf) {
            let b = s.as_bytes();
            if b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] {
                v.push(s[..s.len() - 1].to_string());
            }
            v.push(s.to_string()); // walked → walk
            v.push(format!("{s}e")); // making → make, liked → like
        }
    }
    for suf in ["es", "s", "d", "ly", "'s", "’s"] {
        if let Some(s) = strip(suf) {
            v.push(s.to_string());
        }
    }
    // happily → happy
    if let Some(s) = strip("ily") {
        v.push(format!("{s}y"));
    }
    v
}

/// Dictionary entry for a word as it appears in a text, and the headword it was found under.
pub fn lookup(word: &str) -> Option<(&'static BankWord, bool)> {
    let w: String = word.trim_matches(|c: char| !c.is_alphanumeric()).replace('’', "'").to_lowercase();
    if w.is_empty() {
        return None;
    }
    let all = wordbank::all();
    if let Some(i) = index().get(&w) {
        return Some((&all[*i], false));
    }
    base_forms(&w).iter().find_map(|b| index().get(&b.to_lowercase())).map(|i| (&all[*i], true))
}

/// Whether `token` is `target` or a form of it ("went" → go, "deadlines" → deadline).
pub fn same_word(token: &str, target: &str) -> bool {
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).replace('’', "'").to_lowercase();
    let (t, g) = (norm(token), norm(target));
    !t.is_empty() && (t == g || base_forms(&t).iter().any(|b| b.eq_ignore_ascii_case(&g)))
}

/// Whether a spoken sentence uses `target` (a word in any form, or a whole phrase).
pub fn sentence_uses(sentence: &str, target: &str) -> bool {
    let words: Vec<&str> = target.split_whitespace().collect();
    let said: Vec<&str> = sentence.split_whitespace().collect();
    match words.len() {
        0 => false,
        1 => said.iter().any(|w| same_word(w, words[0])),
        n => said.windows(n).any(|win| win.iter().zip(&words).enumerate().all(|(i, (a, b))| if i + 1 == n { same_word(a, b) } else { same_word(a, b) || a.eq_ignore_ascii_case(b) })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(w: &str) -> Option<String> {
        lookup(w).map(|(b, _)| b.word.clone())
    }

    #[test]
    fn finds_words_and_their_base_forms() {
        assert_eq!(head("company"), Some("company".into()));
        assert_eq!(head("Company,"), Some("company".into()), "case and punctuation");
        assert_eq!(lookup("company").map(|x| x.1), Some(false));
        for (form, base) in [("went", "go"), ("cities", "city"), ("stopped", "stop"), ("hottest", "hot"), ("carried", "carry"), ("walked", "walk"), ("hoped", "hope"), ("liked", "like"), ("children", "child"), ("bigger", "big"), ("books", "book"), ("Scrooge’s", "")] {
            if base.is_empty() {
                assert_eq!(head(form), None, "{form}: not in the dictionary");
            } else {
                assert_eq!(head(form), Some(base.to_string()), "{form} → {base}");
                assert_eq!(lookup(form).map(|x| x.1), Some(true), "{form} is an inflected form");
            }
        }
        assert!(lookup("go").is_some_and(|(b, _)| !b.meaning_ko.is_empty() && !b.example.is_empty()));
    }

    #[test]
    fn finds_the_word_in_a_sentence() {
        assert!(sentence_uses("The deadline for the report is tomorrow", "deadline"));
        assert!(sentence_uses("We missed two deadlines.", "deadline"));
        assert!(sentence_uses("I went home early", "go"));
        assert!(sentence_uses("I'm looking forward to the trip", "look forward to"));
        assert!(!sentence_uses("I have no idea", "deadline"));
        assert!(!sentence_uses("", "deadline"));
    }
}
