//! Japanese text analysis with lindera (IPADIC, built in): words, readings, furigana,
//! pronunciation kana for scoring, base forms for the dictionary and 문절 (bunsetsu) for
//! 직독직해. Everything runs locally; the dictionary is static data in the binary.

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use std::borrow::Cow;
use std::sync::OnceLock;

/// One word of a text as IPADIC analyses it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Token {
    pub surface: String,
    /// 品詞 and its first subdivision ("名詞", "固有名詞").
    pub pos: String,
    pub pos1: String,
    /// 活用型 ("五段・カ行促音便", "一段") and 活用形 ("連用形").
    pub conj_type: String,
    pub conj_form: String,
    /// Dictionary form ("行く" for 行き).
    pub base: String,
    /// Reading in hiragana ("きょう"); empty for unknown words.
    pub reading: String,
    /// Pronunciation in hiragana ("きょお", particle は → "わ"); empty for unknown words.
    pub pron: String,
}

impl Token {
    pub fn is_punct(&self) -> bool {
        self.pos == "記号" || self.surface.chars().all(|c| !c.is_alphanumeric())
    }

    /// A content word (自立語) that starts a 문절: nouns, verbs, adjectives, adverbs, …
    /// Particles, auxiliaries, suffixes and non-independent verbs/nouns attach to the one before.
    pub fn is_independent(&self) -> bool {
        match self.pos.as_str() {
            "名詞" => !matches!(self.pos1.as_str(), "接尾" | "非自立"),
            "動詞" | "形容詞" => self.pos1 == "自立",
            "副詞" | "連体詞" | "接続詞" | "感動詞" | "接頭詞" | "フィラー" => true,
            "助詞" | "助動詞" | "記号" => false,
            _ => true,
        }
    }

    /// Worth looking up / counting as vocabulary (not a particle, auxiliary or symbol).
    pub fn is_content(&self) -> bool {
        !matches!(self.pos.as_str(), "助詞" | "助動詞" | "記号") && !self.is_punct()
    }
}

fn segmenter() -> &'static Segmenter {
    static SEG: OnceLock<Segmenter> = OnceLock::new();
    SEG.get_or_init(|| {
        let dict = load_dictionary("embedded://ipadic").expect("IPADIC is built in");
        Segmenter::new(Mode::Normal, dict, None)
    })
}

/// The words of `text`, in order; their surfaces put together give back the text without
/// whitespace.
pub fn tokenize(text: &str) -> Vec<Token> {
    let Ok(mut tokens) = segmenter().segment(Cow::Borrowed(text)) else { return Vec::new() };
    tokens
        .iter_mut()
        .filter(|t| !t.surface.trim().is_empty())
        .map(|t| {
            let surface = t.surface.to_string();
            let d: Vec<String> = t.details().iter().map(|s| s.to_string()).collect();
            let field = |i: usize| d.get(i).filter(|s| *s != "*" && *s != "UNK").cloned().unwrap_or_default();
            let base = field(6);
            Token {
                pos: field(0),
                pos1: field(1),
                conj_type: field(4),
                conj_form: field(5),
                base: if base.is_empty() { surface.clone() } else { base },
                reading: to_hiragana(&field(7)),
                pron: to_hiragana(&field(8)),
                surface,
            }
        })
        .collect()
}

// ---------------------------------------------------------------- characters

pub fn is_kanji(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' | 'ヶ')
}

pub fn is_hiragana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{309F}')
}

pub fn is_katakana(c: char) -> bool {
    matches!(c, '\u{30A1}'..='\u{30FA}' | 'ー' | '\u{31F0}'..='\u{31FF}')
}

pub fn is_kana(c: char) -> bool {
    is_hiragana(c) || is_katakana(c)
}

/// Kana or kanji: a Japanese letter.
pub fn is_japanese_char(c: char) -> bool {
    is_kana(c) || is_kanji(c)
}

pub fn has_kanji(s: &str) -> bool {
    s.chars().any(is_kanji)
}

/// Text with any Japanese in it.
pub fn is_japanese(s: &str) -> bool {
    s.chars().any(is_japanese_char)
}

/// Katakana → hiragana (other characters unchanged; ー stays).
pub fn to_hiragana(s: &str) -> String {
    s.chars().map(|c| if ('\u{30A1}'..='\u{30F6}').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap_or(c) } else { c }).collect()
}

/// The vowel (あいうえお) a hiragana mora ends with, for turning ー into a vowel.
fn vowel_of(c: char) -> Option<char> {
    const ROWS: [(&str, char); 5] = [
        ("あかさたなはまやらわがざだばぱぁゃゎ", 'あ'),
        ("いきしちにひみりぎじぢびぴぃ", 'い'),
        ("うくすつぬふむゆるぐずづぶぷぅゅゔ", 'う'),
        ("えけせてねへめれげぜでべぺぇ", 'え'),
        ("おこそとのほもよろをごぞどぼぽぉょ", 'お'),
    ];
    ROWS.iter().find(|(row, _)| row.contains(c)).map(|r| r.1)
}

/// Characters to compare when scoring: hiragana with ー spelled out as its vowel, ぢ/づ/を as
/// じ/ず/お, and the long vowels おう/えい read as おお/ええ (how they're spoken); latin letters
/// and digits lower-cased.
fn normalize_kana(s: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for c in to_hiragana(s).chars() {
        let c = match c {
            'ぢ' => 'じ',
            'づ' => 'ず',
            'を' => 'お',
            'ゎ' => 'わ',
            'ゐ' => 'い',
            'ゑ' => 'え',
            _ => c,
        };
        if c == 'ー' || c == '〜' {
            if let Some(v) = out.last().and_then(|p| vowel_of(*p)) {
                out.push(v);
            }
            continue;
        }
        if (c == 'う' && out.last().and_then(|p| vowel_of(*p)) == Some('お')) || (c == 'い' && out.last().and_then(|p| vowel_of(*p)) == Some('え')) {
            let v = vowel_of(*out.last().unwrap()).unwrap();
            out.push(v);
            continue;
        }
        if is_hiragana(c) || c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

// ---------------------------------------------------------------- numbers

/// Arabic numbers written out in kanji (7 → 七, 2025 → 二千二十五) so "7時" and "七時" read the
/// same; full-width digits too. Decimal points and very long numbers are left alone.
pub fn digits_to_kanji(text: &str) -> String {
    let mut out = String::new();
    let mut num = String::new();
    let flush = |num: &mut String, out: &mut String| {
        if !num.is_empty() {
            match num.parse::<u64>() {
                Ok(n) if num.len() <= 12 => out.push_str(&kanji_number(n)),
                _ => out.push_str(num),
            }
            num.clear();
        }
    };
    for c in text.chars() {
        let d = match c {
            '0'..='9' => Some(c),
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32),
            _ => None,
        };
        match d {
            Some(d) => num.push(d),
            None => {
                flush(&mut num, &mut out);
                out.push(c);
            }
        }
    }
    flush(&mut num, &mut out);
    out
}

fn kanji_number(n: u64) -> String {
    const D: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return D[0].into();
    }
    // Up to 9999 within each 万 group.
    let small = |mut n: u64| -> String {
        let mut s = String::new();
        for (unit, name) in [(1000, "千"), (100, "百"), (10, "十")] {
            let d = n / unit;
            if d > 0 {
                if d > 1 {
                    s.push_str(D[d as usize]);
                }
                s.push_str(name);
            }
            n %= unit;
        }
        if n > 0 {
            s.push_str(D[n as usize]);
        }
        s
    };
    let mut s = String::new();
    let mut n = n;
    for (unit, name) in [(100_000_000, "億"), (10_000, "万")] {
        if n >= unit {
            s.push_str(&small(n / unit));
            s.push_str(name);
            n %= unit;
        }
    }
    if n > 0 {
        s.push_str(&small(n));
    }
    s
}

// ---------------------------------------------------------------- readings

/// Hiragana reading of a whole text (kana and unknown words as written).
pub fn reading(text: &str) -> String {
    tokenize(&digits_to_kanji(text)).iter().map(|t| if t.reading.is_empty() { to_hiragana(&t.surface) } else { t.reading.clone() }).collect()
}

/// One piece of text with the reading to show above it (`None` for kana, symbols, latin).
#[derive(Clone, Debug, PartialEq)]
pub struct Ruby {
    pub text: String,
    pub ruby: Option<String>,
}

/// Text split for furigana, one piece per word (so lines can wrap between words): kanji get
/// their reading above, okurigana and kana stay plain (食べました → 食[た] べ ました).
pub fn furigana(text: &str) -> Vec<Ruby> {
    let mut out: Vec<Ruby> = Vec::new();
    // Spaces in the text (between latin words) are kept as plain pieces.
    let mut rest = text;
    while !rest.is_empty() {
        let ws = rest.find(|c: char| !c.is_whitespace()).unwrap_or(rest.len());
        if ws > 0 {
            out.push(Ruby { text: " ".into(), ruby: None });
            rest = &rest[ws..];
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        for t in tokenize(&rest[..end]) {
            if !has_kanji(&t.surface) || t.reading.is_empty() {
                out.push(Ruby { text: t.surface, ruby: None });
                continue;
            }
            match align(&t.surface, &t.reading) {
                Some(parts) => out.extend(parts.into_iter().map(|(text, ruby)| Ruby { text, ruby })),
                None => out.push(Ruby { ruby: Some(t.reading.clone()), text: t.surface }),
            }
        }
        rest = &rest[end..];
    }
    out
}

/// `furigana`, remembered for texts shown again every frame (bounded; cleared when full).
pub fn furigana_cached(text: &str) -> std::sync::Arc<Vec<Ruby>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<Ruby>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap().get(text) {
        return v.clone();
    }
    let v = Arc::new(furigana(text));
    let mut c = cache.lock().unwrap();
    if c.len() > 3000 {
        c.clear();
    }
    c.insert(text.to_string(), v.clone());
    v
}

/// Splits a word into kanji runs (with their part of the reading) and kana runs, matching the
/// kana of the word against the reading: 食べる + たべる → [食|た] [べる].
fn align(surface: &str, reading: &str) -> Option<Vec<(String, Option<String>)>> {
    // Runs of kanji / non-kanji.
    let mut runs: Vec<(String, bool)> = Vec::new();
    for c in surface.chars() {
        let k = is_kanji(c);
        match runs.last_mut() {
            Some((s, kk)) if *kk == k => s.push(c),
            _ => runs.push((c.to_string(), k)),
        }
    }
    let reading: Vec<char> = to_hiragana(reading).chars().collect();
    fn go(runs: &[(String, bool)], r: &[char], out: &mut Vec<(String, Option<String>)>) -> bool {
        let Some(((text, kanji), rest)) = runs.split_first() else { return r.is_empty() };
        if !kanji {
            let kana: Vec<char> = to_hiragana(text).chars().collect();
            if r.len() >= kana.len() && r[..kana.len()] == kana[..] {
                out.push((text.clone(), None));
                if go(rest, &r[kana.len()..], out) {
                    return true;
                }
                out.pop();
            }
            return false;
        }
        // A kanji run reads as at least one kana; try the shortest first when kana follows.
        for n in 1..=r.len() {
            if rest.is_empty() && n != r.len() {
                continue;
            }
            out.push((text.clone(), Some(r[..n].iter().collect())));
            if go(rest, &r[n..], out) {
                return true;
            }
            out.pop();
        }
        false
    }
    let mut out = Vec::new();
    go(&runs, &reading, &mut out).then_some(out)
}

// ---------------------------------------------------------------- scoring support

/// Pronunciation characters of a text, each with the index of the word (`tokenize`) it belongs to.
pub fn pron_chars(tokens: &[Token]) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.is_punct() {
            continue;
        }
        let src = if !t.pron.is_empty() {
            t.pron.as_str()
        } else if !t.reading.is_empty() {
            t.reading.as_str()
        } else {
            t.surface.as_str()
        };
        out.extend(normalize_kana(src).into_iter().map(|c| (c, i)));
    }
    out
}

/// The text as normalized pronunciation kana (what is compared when scoring).
pub fn pron(text: &str) -> String {
    pron_chars(&tokenize(&digits_to_kanji(text))).into_iter().map(|(c, _)| c).collect()
}

/// Letters that count toward reading/speaking speed: kana, kanji, latin letters and digits.
pub fn char_count(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

// ---------------------------------------------------------------- 문절 (bunsetsu)

/// A 문절: a content word with the particles and endings that follow it ("友達と", "行きました。").
#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub text: String,
    pub tokens: Vec<Token>,
}

impl Phrase {
    /// Grammatical role (`grammar::ROLES`) from the particles and the head word.
    pub fn role(&self) -> &'static str {
        let particles: Vec<&str> = self.tokens.iter().filter(|t| t.pos == "助詞").map(|t| t.surface.as_str()).collect();
        let head = self.tokens.iter().find(|t| t.is_independent() && t.pos != "接頭詞");
        let last_content = self.tokens.iter().rev().find(|t| !t.is_punct());
        let Some(head) = head else { return "other" };
        if particles.iter().any(|p| matches!(*p, "は" | "が" | "も")) && head.pos == "名詞" {
            return if head.pos1 == "代名詞" && particles.contains(&"は") { "subject" } else if particles.iter().any(|p| matches!(*p, "は" | "が")) { "subject" } else { "noun" };
        }
        if particles.contains(&"を") {
            return "object";
        }
        match head.pos.as_str() {
            "動詞" => return "verb",
            "形容詞" if last_content.is_some_and(|t| t.pos != "助詞" || t.pos1 == "終助詞") => return "verb",
            _ => {}
        }
        // A noun + だ / です / でした at the end of a sentence is the predicate.
        if self.tokens.iter().any(|t| t.pos == "助動詞" && matches!(t.base.as_str(), "だ" | "です")) && particles.is_empty() {
            return "complement";
        }
        match head.pos.as_str() {
            "名詞" if head.pos1 == "代名詞" => "pronoun",
            "名詞" if particles.iter().any(|p| matches!(*p, "の")) => "modifier",
            "名詞" if particles.iter().any(|p| matches!(*p, "に" | "で" | "へ" | "と" | "から" | "まで" | "より")) => "noun",
            "名詞" => "noun",
            "連体詞" => "determiner",
            "形容詞" | "副詞" => "modifier",
            "接続詞" => "connector",
            "感動詞" | "フィラー" => "other",
            _ => "other",
        }
    }
}

/// Splits a sentence into 문절, punctuation kept with the phrase before it.
pub fn phrases(text: &str) -> Vec<Phrase> {
    let mut out: Vec<Phrase> = Vec::new();
    let mut prefix_pending = false;
    for t in tokenize(text) {
        let starts = t.is_independent() && !prefix_pending && !(t.pos == "名詞" && out.last().is_some_and(|p| p.tokens.last().is_some_and(|l| l.pos == "名詞" && !l.is_punct() && l.pos1 != "代名詞" && t.pos1 != "代名詞")));
        // Compound nouns (会議室, 新しい + …) stay as one phrase; a prefix (お, ご) joins the next word.
        prefix_pending = t.pos == "接頭詞";
        match out.last_mut() {
            Some(p) if !starts => {
                p.text.push_str(&t.surface);
                p.tokens.push(t);
            }
            _ => out.push(Phrase { text: t.surface.clone(), tokens: vec![t] }),
        }
    }
    out
}

/// 직독직해 without Claude: the sentence's 문절 with their roles, each glossed with the built-in
/// dictionary's first meaning of its head word (empty when the word isn't in it).
pub fn local_chunks(text: &str) -> Vec<crate::models::Chunk> {
    phrases(text)
        .into_iter()
        .map(|p| {
            let head = p.tokens.iter().find(|t| t.is_content() && t.pos != "接頭詞");
            let ko = head
                .and_then(|h| crate::dictionary::lookup(&h.base).or_else(|| crate::dictionary::lookup(&h.surface)))
                .map(|(b, _)| b.meaning_ko.split([',', '/']).next().unwrap_or("").trim().to_string())
                .unwrap_or_default();
            crate::models::Chunk { role: p.role().to_string(), ja: p.text, ko }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_and_pronunciation() {
        assert_eq!(reading("今日はいい天気ですね。"), "きょうはいいてんきですね。");
        // は as a particle is said わ; long vowels are spelled out.
        assert_eq!(pron("今日は"), "きょおわ");
        assert_eq!(pron("コーヒー"), pron("こおひい"));
        assert_eq!(pron("7時に行きます"), pron("七時に行きます"));
        assert_eq!(digits_to_kanji("2025年3月15日"), "二千二十五年三月十五日");
    }

    #[test]
    fn furigana_keeps_okurigana_plain() {
        let f = furigana("朝ごはんを食べました。");
        let ruby: Vec<(&str, Option<&str>)> = f.iter().map(|r| (r.text.as_str(), r.ruby.as_deref())).collect();
        assert_eq!(ruby, vec![("朝", Some("あさ")), ("ごはん", None), ("を", None), ("食", Some("た")), ("べ", None), ("まし", None), ("た", None), ("。", None)]);
        assert!(furigana("コーヒーをください").iter().all(|r| r.ruby.is_none()));
    }

    #[test]
    fn phrases_attach_particles_and_endings() {
        let p: Vec<String> = phrases("昨日、友達と映画を見に行きました。").into_iter().map(|p| p.text).collect();
        assert_eq!(p, vec!["昨日、", "友達と", "映画を", "見に", "行きました。"]);
        let roles: Vec<&str> = phrases("私は学生です。").iter().map(|p| p.role()).collect();
        assert_eq!(roles, vec!["subject", "complement"]);
        assert_eq!(phrases("映画を見ました").iter().map(|p| p.role()).collect::<Vec<_>>(), vec!["object", "verb"]);
    }

    #[test]
    fn counts_letters_only() {
        assert_eq!(char_count("今日は、いい天気！"), 7);
    }

    #[test]
    fn local_gloss_spells_the_sentence() {
        let text = "友達と映画を見に行きました。";
        let c = local_chunks(text);
        assert!(crate::storage::chunks_match(text, &c));
        assert_eq!(c[0].ja, "友達と");
        assert!(c.iter().any(|x| x.role == "object"));
    }
}
