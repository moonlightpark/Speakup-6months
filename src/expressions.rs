//! Everyday native expressions and slang, old and new, and high-frequency spoken chunks for the
//! offline lessons (Claude adds fresh ones to generated lessons), from `assets/ja/misc.json`.
//! (expression, 뜻, example, 예문 해석)

use std::sync::OnceLock;

pub type Expr = (&'static str, &'static str, &'static str, &'static str);

fn list(cell: &'static OnceLock<Vec<Expr>>, src: &'static [crate::content::MiscExpr]) -> &'static [Expr] {
    cell.get_or_init(|| src.iter().map(|e| (e.expr.as_str(), e.meaning_ko.as_str(), e.example.as_str(), e.example_ko.as_str())).collect())
}

/// Expressions that have been around for decades and are still heard every day (お疲れさまです …).
pub fn classic() -> &'static [Expr] {
    static V: OnceLock<Vec<Expr>> = OnceLock::new();
    list(&V, &crate::content::misc().classic)
}

/// Recent slang, mostly from social media (2010s–2020s: エモい, それな …).
pub fn recent() -> &'static [Expr] {
    static V: OnceLock<Vec<Expr>> = OnceLock::new();
    list(&V, &crate::content::misc().recent)
}

/// High-frequency spoken chunks (sentence frames: と思います, てもいいですか …) — learned whole,
/// they make speaking fluent much faster than single words. No "…": the example sentence shows
/// how each one is used.
pub fn chunks() -> &'static [Expr] {
    static V: OnceLock<Vec<Expr>> = OnceLock::new();
    list(&V, &crate::content::misc().chunks)
}

/// Today's expression: one a day keeps idioms and slang at about 5% of new material.
/// Classic expressions and recent slang take turns (odd days: recent slang).
pub fn pick_for_today(day: i64, knows: impl Fn(&str) -> bool) -> Vec<Expr> {
    let banks = if day % 2 == 1 { [recent(), classic()] } else { [classic(), recent()] };
    banks.iter().flat_map(|bank| bank.iter()).find(|e| !knows(e.0)).copied().into_iter().collect()
}

/// The next `n` high-frequency chunks the learner hasn't saved yet.
pub fn pick_chunks(n: usize, knows: impl Fn(&str) -> bool) -> Vec<Expr> {
    chunks().iter().filter(|e| !knows(e.0)).take(n).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_expression_a_day_alternating() {
        let none = |_: &str| false;
        assert!(classic().len() >= 20 && recent().len() >= 20 && chunks().len() >= 30);
        assert_eq!(pick_for_today(2, none), vec![classic()[0]]);
        assert_eq!(pick_for_today(3, none), vec![recent()[0]]);
        // Falls back to the other bank once one is used up.
        let knows_recent = |t: &str| recent().iter().any(|e| e.0 == t);
        assert_eq!(pick_for_today(3, knows_recent), vec![classic()[0]]);
        assert_eq!(pick_chunks(4, |t| t == chunks()[0].0)[0], chunks()[1]);
        assert!(chunks().iter().all(|c| !c.0.contains('…') && c.2.contains(c.0)), "examples use their chunk");
    }
}
