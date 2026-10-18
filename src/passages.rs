//! Reading passages for 스피드 리딩, taken from the learner's own graded readers.
//! The text is extracted from `DATA/지문_N단계_*.pdf` on the learner's computer and kept in the
//! data folder (`passages/*.json`, one entry per page). The books themselves are never bundled
//! with the app or published: only this reader is.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Book {
    /// Reading stage from the file name (지문_0단계 … 지문_3단계): 0 = easiest.
    pub stage: u32,
    pub title: String,
    /// The text of each story page, in order.
    pub pages: Vec<String>,
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("passages")
}

/// All books in the passages folder, easiest first.
pub fn load(dir: &Path) -> Vec<Book> {
    let mut books: Vec<Book> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_str::<Book>(&std::fs::read_to_string(e.path()).ok()?).ok())
        .filter(|b| !b.pages.is_empty())
        .collect();
    books.sort_by(|a, b| (a.stage, &a.title).cmp(&(b.stage, &b.title)));
    books
}

/// Reading stage that fits a level: A1 → 0, A2 → 1, B1 → 2, B1+ and up → 3.
pub fn stage_for(level: f32) -> u32 {
    match level {
        l if l < 2.0 => 0,
        l if l < 3.5 => 1,
        l if l < 5.0 => 2,
        _ => 3,
    }
}

/// The book for this level: the hardest one at or below the level's stage (else the easiest).
pub fn pick(books: &[Book], level: f32) -> Option<usize> {
    let want = stage_for(level);
    (0..books.len()).filter(|i| books[*i].stage <= want).max_by_key(|i| books[*i].stage).or((!books.is_empty()).then_some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(stage: u32, title: &str) -> Book {
        Book { stage, title: title.into(), pages: vec!["One page.".into()] }
    }

    #[test]
    fn picks_the_stage_for_the_level() {
        let books = vec![book(0, "The Cat"), book(1, "Pocahontas"), book(3, "A Christmas Carol")];
        assert_eq!(pick(&books, 1.5), Some(0));
        assert_eq!(pick(&books, 3.0), Some(1));
        assert_eq!(pick(&books, 4.0), Some(1), "no stage-2 book: the easier one");
        assert_eq!(pick(&books, 7.0), Some(2));
        assert_eq!(pick(&[book(2, "One-Way Ticket")], 1.0), Some(0), "only a harder book: still something to read");
        assert_eq!(pick(&[], 3.0), None);
    }

    #[test]
    fn loads_json_books_easiest_first() {
        let d = std::env::temp_dir().join(format!("speakup-passages-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("b.json"), serde_json::to_string(&book(2, "B")).unwrap()).unwrap();
        std::fs::write(d.join("a.json"), serde_json::to_string(&book(0, "A")).unwrap()).unwrap();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        let books = load(&d);
        assert_eq!(books.iter().map(|b| b.title.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        let _ = std::fs::remove_dir_all(d);
    }
}
