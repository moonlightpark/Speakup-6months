//! Compares what the learner said (speech-recognition transcript) with the target text.
//! Japanese has no spaces and the recognizer may write a word in kanji or kana (今日 / きょう),
//! so both are turned into pronunciation kana (ja.rs) and aligned character by character
//! (Levenshtein); each word of the target is OK when most of its sounds were heard.

#[derive(Clone, Debug)]
pub struct WordMark {
    pub word: String,
    pub ok: bool,
}

#[derive(Clone, Debug)]
pub struct ScoreResult {
    pub score: f32,
    pub marks: Vec<WordMark>,
}

/// Edit-distance alignment of two character sequences: which characters of `t` were matched.
fn align(t: &[char], s: &[char]) -> (Vec<bool>, usize) {
    let (n, m) = (t.len(), s.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = usize::from(t[i - 1] != s[j - 1]);
            dp[i][j] = (dp[i - 1][j] + 1).min(dp[i][j - 1] + 1).min(dp[i - 1][j - 1] + sub);
        }
    }
    let mut hit = vec![false; n];
    let (mut i, mut j, mut extra) = (n, m, 0);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && t[i - 1] == s[j - 1] && dp[i][j] == dp[i - 1][j - 1] {
            hit[i - 1] = true;
            i -= 1;
            j -= 1;
        } else if i > 0 && j > 0 && dp[i][j] == dp[i - 1][j - 1] + 1 {
            i -= 1;
            j -= 1;
        } else if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            i -= 1;
        } else {
            extra += 1;
            j -= 1;
        }
    }
    (hit, extra)
}

/// Scores how well `spoken` reproduces `target` (0–100, one mark per word of the target; the
/// marks' words put together give the target without whitespace).
pub fn score(target: &str, spoken: &str) -> ScoreResult {
    let tokens = crate::ja::tokenize(&crate::ja::digits_to_kanji(target));
    // Marks show the target as written (digits stay digits), so map the tokens back onto it.
    let display = display_words(target, &tokens);
    let t = crate::ja::pron_chars(&tokens);
    let s: Vec<char> = crate::ja::pron(spoken).chars().collect();
    if t.is_empty() {
        return ScoreResult { score: 0.0, marks: Vec::new() };
    }
    let tc: Vec<char> = t.iter().map(|x| x.0).collect();
    let (hit, extra) = align(&tc, &s);
    let n = tc.len();
    let matched = hit.iter().filter(|h| **h).count();
    let penalty = (extra as f32 * 0.5).min(n as f32 * 0.3);
    let score = ((matched as f32 - penalty).max(0.0) / n as f32 * 100.0).clamp(0.0, 100.0);
    let marks = display
        .into_iter()
        .enumerate()
        .map(|(i, word)| {
            let sounds: Vec<bool> = t.iter().zip(&hit).filter(|((_, k), _)| *k == i).map(|(_, h)| *h).collect();
            let got = sounds.iter().filter(|h| **h).count();
            // Short words must be heard whole; longer ones may miss a sound (a recognizer slip).
            let ok = sounds.is_empty() || got == sounds.len() || (sounds.len() >= 4 && got as f32 / sounds.len() as f32 >= 0.75);
            WordMark { word, ok }
        })
        .collect();
    ScoreResult { score, marks }
}

/// The target's words as written: token surfaces, except that numbers written in digits (which
/// were spelled out in kanji for analysis) are shown as the learner saw them.
fn display_words(target: &str, tokens: &[crate::ja::Token]) -> Vec<String> {
    let compact: String = target.chars().filter(|c| !c.is_whitespace()).collect();
    let joined: String = tokens.iter().map(|t| t.surface.as_str()).collect();
    if joined == compact {
        return tokens.iter().map(|t| t.surface.clone()).collect();
    }
    // Digits were converted: give each digit run to the first token that holds its kanji and
    // the rest of the text to the tokens in order (falls back to one mark per token).
    let mut out = Vec::new();
    let mut rest = compact.as_str();
    for t in tokens {
        let surf = &t.surface;
        if let Some(r) = rest.strip_prefix(surf.as_str()) {
            out.push(surf.clone());
            rest = r;
        } else {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c)).collect();
            if !digits.is_empty() && surf.chars().all(|c| "〇一二三四五六七八九十百千万億".contains(c)) {
                out.push(digits.clone());
                rest = &rest[digits.len()..];
            } else if surf.chars().all(|c| "〇一二三四五六七八九十百千万億".contains(c)) {
                out.push(String::new());
            } else {
                return tokens.iter().map(|t| t.surface.clone()).collect();
            }
        }
    }
    if let Some(last) = out.last_mut() {
        last.push_str(rest);
    }
    out
}

impl ScoreResult {
    /// Characters of the words that were heard (for speaking speed in 자/분).
    pub fn heard_chars(&self) -> usize {
        self.marks.iter().filter(|m| m.ok).map(|m| crate::ja::char_count(&m.word)).sum()
    }
}

/// Recall check: did the learner say the target word/phrase somewhere in what they said?
/// (By sound, so kanji/kana differences and a slip of one sound in a long word don't matter.)
pub fn contains_phrase(spoken: &str, phrase: &str) -> bool {
    let s: Vec<char> = crate::ja::pron(spoken).chars().collect();
    let p: Vec<char> = crate::ja::pron(phrase).chars().collect();
    if p.is_empty() || s.len() < p.len() {
        return false;
    }
    let allowed = if p.len() >= 5 { 1 } else { 0 };
    s.windows(p.len()).any(|w| w.iter().zip(&p).filter(|(a, b)| a != b).count() <= allowed)
        || (p.len() >= 3 && s.windows(p.len() - 1).any(|w| {
            // A dropped long-vowel sound (きょお → きょ).
            let (hit, _) = align(&p, w);
            hit.iter().filter(|h| !**h).count() <= 1 && p.len() >= 5
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_match_ignores_punctuation_and_script() {
        let r = score("今日はいい天気ですね。", "きょうはいいてんきですね");
        assert_eq!(r.score, 100.0);
        assert!(r.marks.iter().all(|m| m.ok));
        assert_eq!(score("コーヒーをください。", "珈琲を下さい").score, 100.0);
    }

    #[test]
    fn numbers_in_digits_or_kanji_are_equivalent() {
        assert_eq!(score("七時に起きます。", "7時に起きます").score, 100.0);
        let r = score("3時に会いましょう。", "三時に会いましょう");
        assert_eq!(r.score, 100.0);
        assert_eq!(r.marks.iter().map(|m| m.word.as_str()).collect::<String>(), "3時に会いましょう。");
    }

    #[test]
    fn missing_word_is_marked() {
        let r = score("昨日、友達と映画を見ました。", "昨日、映画を見ました");
        assert!(r.score > 60.0 && r.score < 100.0, "{}", r.score);
        let missing: Vec<_> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.as_str()).collect();
        assert_eq!(missing, vec!["友達", "と"]);
    }

    #[test]
    fn marks_spell_the_target() {
        let target = "すみません、駅はどこですか？";
        let r = score(target, "すみません駅はどこですか");
        assert_eq!(r.marks.iter().map(|m| m.word.as_str()).collect::<String>(), target);
    }

    #[test]
    fn unrelated_speech_scores_low() {
        assert!(score("金曜日までに資料を送っていただけますか。", "今日は暑いですね").score < 30.0);
    }

    #[test]
    fn recall_contains() {
        assert!(contains_phrase("明日は会議があります。", "会議"));
        assert!(contains_phrase("きのうともだちとあった", "友達"));
        assert!(!contains_phrase("わかりません", "締め切り"));
    }
}
