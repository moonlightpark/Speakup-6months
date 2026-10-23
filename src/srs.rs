//! Spaced repetition (SM-2 variant, as used by Anki/SuperMemo) plus "weak item" tracking:
//! items the learner keeps failing get extra repetitions and are sent to homework.

use crate::models::*;
use chrono::Duration;

/// Maps a 0..100 speaking score to an SM-2 quality grade (0..5).
pub fn quality_from_score(score: f32, pass: f32) -> u8 {
    match score {
        s if s >= 95.0 => 5,
        s if s >= pass + 5.0 => 4,
        s if s >= pass => 3,
        s if s >= pass - 25.0 => 2,
        s if s > 0.0 => 1,
        _ => 0,
    }
}

/// Applies one graded review. `score` is the learner's first (unprompted) attempt of the day.
pub fn review(item: &mut StudyItem, score: f32, pass: f32) {
    let q = quality_from_score(score, pass);
    item.attempts += 1;
    item.last_score = score;
    item.best_score = item.best_score.max(score);
    if q >= 3 {
        item.reps += 1;
        item.pass_streak += 1;
        item.interval_days = match item.reps {
            1 => 1,
            2 => 3,
            _ => ((item.interval_days.max(1) as f32) * item.ease).round() as u32,
        };
    } else {
        item.reps = 0;
        item.lapses += 1;
        item.pass_streak = 0;
        item.interval_days = 1;
        item.weak = true;
    }
    let qf = q as f32;
    item.ease = (item.ease + 0.1 - (5.0 - qf) * (0.08 + (5.0 - qf) * 0.02)).max(1.3);
    // A weak item is cleared only after three passes in a row.
    if item.weak && item.pass_streak >= 3 {
        item.weak = false;
    }
    // Weak items come back sooner, whatever the interval says.
    let interval = if item.weak { item.interval_days.min(2) } else { item.interval_days };
    item.due = today() + Duration::days(interval.max(1) as i64);
}

/// Most single words a review session holds when chunks/sentences are waiting: review is
/// centred on whole expressions, which carry over to speaking much faster than single words.
pub const MAX_WORD_SHARE: f32 = 0.4;

/// Due items first (most overdue first), then weak items that are not yet due. Chunks and
/// sentences go first; single words fill at most `MAX_WORD_SHARE` of the session unless there
/// is nothing else to review.
pub fn review_queue(items: &[StudyItem], limit: usize) -> Vec<String> {
    let t = today();
    let mut due: Vec<&StudyItem> = items.iter().filter(|i| i.due <= t).collect();
    due.sort_by_key(|i| (i.due, std::cmp::Reverse(i.lapses)));
    let mut ordered: Vec<&StudyItem> = due;
    for i in items.iter().filter(|i| i.weak && i.due > t) {
        if !ordered.iter().any(|o| o.id == i.id) {
            ordered.push(i);
        }
    }
    let (words, others): (Vec<&StudyItem>, Vec<&StudyItem>) = ordered.into_iter().partition(|i| i.kind == ItemKind::Word);
    let word_cap = ((limit as f32 * MAX_WORD_SHARE).floor() as usize).max(limit.saturating_sub(others.len()));
    let mut ids: Vec<String> = others.iter().map(|i| i.id.clone()).collect();
    ids.truncate(limit.saturating_sub(words.len().min(word_cap)));
    ids.extend(words.iter().take(word_cap.min(limit - ids.len())).map(|i| i.id.clone()));
    ids
}

pub fn due_count(items: &[StudyItem]) -> usize {
    let t = today();
    items.iter().filter(|i| i.due <= t).count()
}

pub fn make_id(kind: ItemKind, text: &str) -> String {
    let k = match kind {
        ItemKind::Word => "w",
        ItemKind::Sentence => "s",
    };
    let norm: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    format!("{k}:{norm}")
}

/// Adds a newly learned item (or returns the existing one's index). New items are due tomorrow.
pub fn add_item(items: &mut Vec<StudyItem>, mut item: StudyItem) -> usize {
    item.id = make_id(item.kind, &item.text);
    if let Some(pos) = items.iter().position(|i| i.id == item.id) {
        return pos;
    }
    item.added = today();
    item.due = today() + Duration::days(1);
    items.push(item);
    items.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> StudyItem {
        StudyItem { id: "w:test".into(), kind: ItemKind::Word, text: "test".into(), ..Default::default() }
    }

    #[test]
    fn passing_grows_interval() {
        let mut i = item();
        review(&mut i, 90.0, 80.0);
        assert_eq!(i.interval_days, 1);
        review(&mut i, 90.0, 80.0);
        assert_eq!(i.interval_days, 3);
        review(&mut i, 90.0, 80.0);
        assert!(i.interval_days >= 6);
        assert!(!i.weak);
    }

    #[test]
    fn failing_marks_weak_until_three_passes() {
        let mut i = item();
        review(&mut i, 30.0, 80.0);
        assert!(i.weak);
        assert_eq!(i.lapses, 1);
        review(&mut i, 85.0, 80.0);
        review(&mut i, 85.0, 80.0);
        assert!(i.weak);
        review(&mut i, 85.0, 80.0);
        assert!(!i.weak);
    }

    #[test]
    fn review_centres_on_chunks_and_sentences() {
        let mk = |kind, n: usize| StudyItem { id: format!("{kind:?}{n}"), kind, text: format!("t{n}"), due: today() - Duration::days(n as i64), ..Default::default() };
        let words: Vec<StudyItem> = (0..10).map(|n| mk(ItemKind::Word, n)).collect();
        let chunks: Vec<StudyItem> = (0..8).map(|n| mk(ItemKind::Sentence, n)).collect();
        let all: Vec<StudyItem> = words.iter().chain(&chunks).cloned().collect();
        let q = review_queue(&all, 10);
        let n_words = q.iter().filter(|id| id.starts_with("Word")).count();
        assert_eq!((q.len(), n_words), (10, 4), "words fill at most 40%");
        assert!(q[0].starts_with("Sentence"), "chunks and sentences come first");
        // With few chunks waiting, words fill the rest; with only words, all words.
        assert_eq!(review_queue(&[words.clone(), chunks[..2].to_vec()].concat(), 10).len(), 10);
        assert_eq!(review_queue(&words[..3], 10).len(), 3);
    }

    #[test]
    fn ids_are_stable() {
        assert_eq!(make_id(ItemKind::Sentence, "Nice to meet you!"), "s:nice_to_meet_you");
    }
}
