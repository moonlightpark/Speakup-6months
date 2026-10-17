//! Compares what the learner said (speech-recognition transcript) with the target text.
//! Word-level alignment (Levenshtein over words) gives a per-word hit/miss map and a 0..100 score.

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

const CONTRACTIONS: &[(&str, &str)] = &[
    ("i'm", "i am"), ("you're", "you are"), ("we're", "we are"), ("they're", "they are"),
    ("he's", "he is"), ("she's", "she is"), ("it's", "it is"), ("that's", "that is"),
    ("there's", "there is"), ("what's", "what is"), ("let's", "let us"), ("i've", "i have"),
    ("you've", "you have"), ("we've", "we have"), ("they've", "they have"), ("i'll", "i will"),
    ("you'll", "you will"), ("we'll", "we will"), ("they'll", "they will"), ("it'll", "it will"),
    ("i'd", "i would"), ("you'd", "you would"), ("we'd", "we would"), ("they'd", "they would"),
    ("don't", "do not"), ("doesn't", "does not"), ("didn't", "did not"), ("can't", "can not"),
    ("cannot", "can not"), ("won't", "will not"), ("isn't", "is not"), ("aren't", "are not"),
    ("wasn't", "was not"), ("weren't", "were not"), ("haven't", "have not"), ("hasn't", "has not"),
    ("couldn't", "could not"), ("shouldn't", "should not"), ("wouldn't", "would not"),
    ("gonna", "going to"), ("wanna", "want to"),
];

const NUMBERS: &[&str] = &[
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
    "nineteen", "twenty",
];

fn normalize_token(raw: &str) -> String {
    raw.chars()
        .map(|c| if c == '’' { '\'' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect::<String>()
        .trim_matches('\'')
        .to_lowercase()
}

/// Lower-cases, strips punctuation, expands contractions and small numbers so that
/// "I'm 5." and "I am five" compare equal.
pub fn normalize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| c.is_whitespace() || c == '-' || c == '—' || c == '/') {
        let t = normalize_token(raw);
        if t.is_empty() {
            continue;
        }
        if let Some((_, full)) = CONTRACTIONS.iter().find(|(c, _)| *c == t) {
            out.extend(full.split(' ').map(String::from));
        } else if let Ok(n) = t.parse::<usize>() {
            match NUMBERS.get(n) {
                Some(w) => out.push(w.to_string()),
                None => out.push(t),
            }
        } else {
            out.push(t);
        }
    }
    out
}

fn char_similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return if n == m { 1.0 } else { 0.0 };
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    1.0 - prev[m] as f32 / n.max(m) as f32
}

fn words_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Tolerate spelling variants from the recognizer (colour/color, okay/ok) on longer words.
    a.chars().count() >= 4 && char_similarity(a, b) >= 0.75
}

/// Scores how well `spoken` reproduces `target`.
pub fn score(target: &str, spoken: &str) -> ScoreResult {
    let display: Vec<&str> = target.split_whitespace().collect();
    let t = normalize(target);
    let s = normalize(spoken);
    if t.is_empty() {
        return ScoreResult { score: 0.0, marks: Vec::new() };
    }
    let (n, m) = (t.len(), s.len());
    // dp[i][j] = edit distance between t[..i] and s[..j]
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = if words_match(&t[i - 1], &s[j - 1]) { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1).min(dp[i][j - 1] + 1).min(dp[i - 1][j - 1] + sub);
        }
    }
    // Backtrack to find which target words were matched.
    let mut hit = vec![false; n];
    let (mut i, mut j, mut extra) = (n, m, 0);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && words_match(&t[i - 1], &s[j - 1]) && dp[i][j] == dp[i - 1][j - 1] {
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
    let matched = hit.iter().filter(|h| **h).count();
    let penalty = (extra as f32 * 0.5).min(n as f32 * 0.3);
    let score = ((matched as f32 - penalty).max(0.0) / n as f32 * 100.0).clamp(0.0, 100.0);

    // Map normalized hits back onto the words as displayed. Contractions expand to several
    // normalized tokens; a display word counts as OK only if all of its tokens were hit.
    let mut marks = Vec::new();
    let mut k = 0;
    for w in display {
        let cnt = normalize(w).len();
        if cnt == 0 {
            marks.push(WordMark { word: w.to_string(), ok: true });
            continue;
        }
        let ok = (k..(k + cnt).min(n)).all(|x| hit[x]);
        k += cnt;
        marks.push(WordMark { word: w.to_string(), ok });
    }
    ScoreResult { score, marks }
}

/// Recall check: did the learner produce the target word/phrase somewhere in what they said?
pub fn contains_phrase(spoken: &str, phrase: &str) -> bool {
    let s = normalize(spoken);
    let p = normalize(phrase);
    if p.is_empty() || s.len() < p.len() {
        return false;
    }
    s.windows(p.len()).any(|w| w.iter().zip(&p).all(|(a, b)| words_match(a, b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_match_ignores_case_and_punctuation() {
        let r = score("Nice to meet you!", "nice to meet you");
        assert_eq!(r.score, 100.0);
        assert!(r.marks.iter().all(|m| m.ok));
    }

    #[test]
    fn contractions_and_numbers_are_equivalent() {
        assert_eq!(score("I'm five years old.", "I am 5 years old").score, 100.0);
        assert_eq!(score("I do not know", "I don't know").score, 100.0);
    }

    #[test]
    fn missing_word_is_marked() {
        let r = score("I would like a cup of coffee", "I would like cup of coffee");
        assert!(r.score > 80.0 && r.score < 100.0);
        let missing: Vec<_> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.as_str()).collect();
        assert_eq!(missing, vec!["a"]);
    }

    #[test]
    fn unrelated_speech_scores_low() {
        assert!(score("Could you send me the report by Friday?", "the weather is nice").score < 30.0);
    }

    #[test]
    fn spelling_variants_tolerated() {
        assert_eq!(score("My favourite colour", "my favorite color").score, 100.0);
    }

    #[test]
    fn recall_contains() {
        assert!(contains_phrase("I think it's an opportunity.", "opportunity"));
        assert!(contains_phrase("we should follow up tomorrow", "follow up"));
        assert!(!contains_phrase("no idea", "deadline"));
    }
}
