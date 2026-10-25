//! 시제 3문장: each new word said in one frame across three tenses —
//! "～しようと思っています" (~할까 생각 중이에요) · "～しようと思っていました" (~할까 생각했어요) ·
//! "～するつもりです" (~할 생각이에요). Claude writes natural sentences for the day's words in one
//! call (kept in tenses.json); without it a frame fitted to the part of speech is used, with
//! the verb's volitional form worked out from its conjugation type.

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
pub const TENSES: [(&str, &str); 3] = [("현재", "~할까 생각 중이에요"), ("과거", "~할까 생각했어요"), ("미래", "~할 생각이에요")];

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

/// Volitional form (行く → 行こう, 食べる → 食べよう, 勉強する → 勉強しよう, 来る → 来よう) of a
/// verb in its dictionary form; None if it isn't one.
pub fn volitional(verb: &str) -> Option<String> {
    let tokens = crate::ja::tokenize(verb);
    let last = tokens.last()?;
    let stem = &verb[..verb.len() - last.surface.len()];
    let v = &last.surface;
    if last.pos != "動詞" || last.conj_form != "基本形" {
        return None;
    }
    let ct = last.conj_type.as_str();
    let out = if ct.starts_with("サ変") {
        format!("{}しよう", v.strip_suffix("する").or_else(|| v.strip_suffix("ずる")).unwrap_or(""))
    } else if ct.starts_with("カ変") {
        format!("{}よう", v.strip_suffix('る').unwrap_or(v))
    } else if ct.starts_with("一段") {
        format!("{}よう", v.strip_suffix('る')?)
    } else if ct.starts_with("五段") {
        let mut chars: Vec<char> = v.chars().collect();
        let end = chars.pop()?;
        const U: &str = "うくぐすつぬぶむる";
        const O: &str = "おこごそとのぼもろ";
        let i = U.chars().position(|c| c == end)?;
        chars.push(O.chars().nth(i)?);
        format!("{}う", chars.into_iter().collect::<String>())
    } else {
        return None;
    };
    Some(format!("{stem}{out}"))
}

/// Sentences from a frame when Claude can't write them: the action fits the part of speech.
pub fn template(word: &str, pos_ko: &str, meaning_ko: &str) -> TenseSet {
    let pos = pos_ko.split(['·', ',', '/']).map(str::trim).collect::<Vec<_>>();
    // (volitional, dictionary form) of the action.
    let (vol, dict) = if pos.contains(&"동사") && let Some(v) = volitional(word) {
        (v, word.to_string())
    } else if pos.contains(&"동사") && pos.contains(&"명사") {
        (format!("{word}しよう"), format!("{word}する"))
    } else if pos.first() == Some(&"명사") {
        (format!("{word}について話そう"), format!("{word}について話す"))
    } else {
        (format!("「{word}」を使ってみよう"), format!("「{word}」を使ってみる"))
    };
    let ko = |frame: &str| format!("({meaning_ko}) {frame}");
    TenseSet {
        word: word.to_string(),
        present: format!("{vol}と思っています。"),
        present_ko: ko(TENSES[0].1),
        past: format!("{vol}と思っていました。"),
        past_ko: ko(TENSES[1].1),
        future: format!("{dict}つもりです。"),
        future_ko: ko(TENSES[2].1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volitional_forms() {
        for (v, o) in [("行く", "行こう"), ("食べる", "食べよう"), ("話す", "話そう"), ("待つ", "待とう"), ("買う", "買おう"), ("帰る", "帰ろう"), ("勉強する", "勉強しよう"), ("来る", "来よう"), ("泳ぐ", "泳ごう")] {
            assert_eq!(volitional(v).as_deref(), Some(o), "{v}");
        }
        assert_eq!(volitional("会議"), None);
    }

    #[test]
    fn frames_by_part_of_speech() {
        let v = template("説明する", "동사", "설명하다");
        assert_eq!((v.present.as_str(), v.past.as_str(), v.future.as_str()), ("説明しようと思っています。", "説明しようと思っていました。", "説明するつもりです。"));
        assert!(v.present_ko.contains("~할까 생각 중이에요") && v.past_ko.contains("생각했어요") && v.future_ko.contains("생각이에요"));
        assert_eq!(template("運動", "명사·동사", "운동").present, "運動しようと思っています。");
        assert_eq!(template("締め切り", "명사", "마감").past, "締め切りについて話そうと思っていました。");
        assert_eq!(template("きれい", "な형용사", "예쁜").future, "「きれい」を使ってみるつもりです。");
    }
}
