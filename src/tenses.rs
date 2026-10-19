//! 시제 3문장: each new word said in one frame across three tenses —
//! "I'm thinking of …" (~할까 생각 중이에요) · "I was thinking of …" (~할까 생각했어요) ·
//! "I'll think about …" (~할까 생각할 거예요). Claude writes natural sentences for the day's
//! words in one call (kept in tenses.json); without it a frame fitted to the part of speech is
//! used.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TenseSet {
    pub word: String,
    pub present: String,
    pub present_ko: String,
    pub past: String,
    pub past_ko: String,
    pub future: String,
    pub future_ko: String,
}

/// (label, Korean frame) of the three tenses, in drill order.
pub const TENSES: [(&str, &str); 3] = [("현재", "~할까 생각 중이에요"), ("과거", "~할까 생각했어요"), ("미래", "~할까 생각할 거예요")];

/// (label, Korean ending) for sentences said in three tenses in 문장 읽기.
pub const SENTENCE_TENSES: [(&str, &str); 3] = [("현재", "~해요"), ("과거", "~했어요"), ("미래", "~할 거예요")];

impl TenseSet {
    /// (sentence, Korean, tense label, Korean frame) × 3.
    pub fn rows(&self) -> [(&str, &str, &str, &str); 3] {
        [
            (&self.present, &self.present_ko, TENSES[0].0, TENSES[0].1),
            (&self.past, &self.past_ko, TENSES[1].0, TENSES[1].1),
            (&self.future, &self.future_ko, TENSES[2].0, TENSES[2].1),
        ]
    }

    pub fn is_complete(&self) -> bool {
        [&self.present, &self.past, &self.future].iter().all(|s| !s.trim().is_empty())
    }
}

/// "-ing" form: make → making, run → running, die → dying, see → seeing.
pub fn ing(verb: &str) -> String {
    let v = verb.to_lowercase();
    if let Some(stem) = v.strip_suffix("ie") {
        return format!("{stem}ying");
    }
    if v.ends_with('e') && !v.ends_with("ee") && v.len() > 2 {
        return format!("{}ing", &v[..v.len() - 1]);
    }
    let b = v.as_bytes();
    let vowel = |c: u8| b"aeiou".contains(&c);
    // Short consonant–vowel–consonant words double the last letter (run, stop, sit).
    if b.len() == 3 && !vowel(b[0]) && vowel(b[1]) && !vowel(b[2]) && !b"wxy".contains(&b[2]) {
        return format!("{v}{}ing", b[2] as char);
    }
    format!("{v}ing")
}

/// Sentences from a frame when Claude can't write them: the action fits the part of speech.
pub fn template(word: &str, pos_ko: &str, meaning_ko: &str) -> TenseSet {
    let pos = pos_ko.split(['·', ',', '/']).next().unwrap_or("").trim();
    let action = match pos {
        "동사" => ing(word),
        "형용사" => format!("being more {word}"),
        "명사" => format!("talking about the {word}"),
        _ => format!("using \"{word}\" more often"),
    };
    let ko = |frame: &str| format!("({meaning_ko}) {frame}");
    TenseSet {
        word: word.to_string(),
        present: format!("I'm thinking of {action}."),
        present_ko: ko(TENSES[0].1),
        past: format!("I was thinking of {action}."),
        past_ko: ko(TENSES[1].1),
        future: format!("I'll think about {action}."),
        future_ko: ko(TENSES[2].1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ing_forms() {
        for (v, i) in [("make", "making"), ("run", "running"), ("die", "dying"), ("see", "seeing"), ("explain", "explaining"), ("go", "going"), ("fix", "fixing")] {
            assert_eq!(ing(v), i);
        }
    }

    #[test]
    fn frames_by_part_of_speech() {
        let v = template("explain", "동사", "설명하다");
        assert_eq!((v.present.as_str(), v.past.as_str(), v.future.as_str()), ("I'm thinking of explaining.", "I was thinking of explaining.", "I'll think about explaining."));
        assert!(v.present_ko.contains("~할까 생각 중이에요") && v.past_ko.contains("생각했어요") && v.future_ko.contains("생각할 거예요"));
        assert_eq!(template("careful", "형용사", "조심하는").present, "I'm thinking of being more careful.");
        assert_eq!(template("deadline", "명사·동사", "마감일").past, "I was thinking of talking about the deadline.");
    }
}
