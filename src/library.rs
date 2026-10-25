//! Reading library: graded stories from picture-book tales to classic novels.
//!
//! Easy tales retold for learners are built in (`assets/ja/stories.json`). The other books are
//! public-domain originals from 青空文庫 (Aozora Bunko, `library_books.tsv`: 新字新仮名 texts
//! graded N4 → N1), downloaded once on request (a ZIP of Shift_JIS text), converted to UTF-8
//! and cached in the data folder.
//! Books are split into pages of paragraphs so a session is a comfortable read.

use anyhow::{Result, anyhow};
use crate::models::band_index_of_cefr;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Builtin(&'static str),
    /// Path of the text ZIP under https://www.aozora.gr.jp/.
    Aozora(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Book {
    pub id: &'static str,
    pub title: &'static str,
    pub title_ko: &'static str,
    pub author: &'static str,
    /// The learner level it's shelved under (index in `LEVEL_BANDS`).
    pub level: usize,
    pub kind: &'static str,
    pub blurb_ko: &'static str,
    pub source: Source,
}

#[derive(Deserialize)]
struct Story {
    id: String,
    title: String,
    title_ko: String,
    author: String,
    level: usize,
    kind: String,
    blurb_ko: String,
    text: String,
}

/// Easy tales retold for learners (levels N5–N4).
fn builtin() -> &'static [Book] {
    static V: OnceLock<Vec<Book>> = OnceLock::new();
    V.get_or_init(|| {
        static STORIES: OnceLock<Vec<Story>> = OnceLock::new();
        let stories = STORIES.get_or_init(|| serde_json::from_str(include_str!("../assets/ja/stories.json")).unwrap_or_default());
        stories
            .iter()
            .map(|s| Book {
                id: &s.id,
                title: &s.title,
                title_ko: &s.title_ko,
                author: &s.author,
                level: s.level,
                kind: &s.kind,
                blurb_ko: &s.blurb_ko,
                source: Source::Builtin(&s.text),
            })
            .collect()
    })
}

/// 青空文庫 books, one per line:
/// level (N4 … N1), id, ZIP path, kind, title, Korean title, author, Korean blurb.
const CATALOG: &str = include_str!("library_books.tsv");

fn parse_catalog() -> Vec<Book> {
    CATALOG
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let f: Vec<&'static str> = l.split('\t').collect();
            let [level, id, path, kind, title, title_ko, author, blurb_ko] = f[..] else { return None };
            Some(Book { id, title, title_ko, author, level: band_index_of_cefr(level)?, kind, blurb_ko, source: Source::Aozora(path) })
        })
        .collect()
}

/// Every book on the shelf, easiest level first.
pub fn books() -> &'static [Book] {
    static ALL: OnceLock<Vec<Book>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<Book> = builtin().iter().copied().chain(parse_catalog()).collect();
        all.sort_by_key(|b| b.level);
        all
    })
}

pub fn book(id: &str) -> Option<&'static Book> {
    books().iter().find(|b| b.id == id)
}

/// The books shelved under a level (`LEVEL_BANDS` index).
pub fn shelf(level: usize) -> Vec<&'static Book> {
    books().iter().filter(|b| b.level == level).collect()
}

/// A readable unit: a title and its paragraphs.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub heading: String,
    pub paragraphs: Vec<String>,
}

/// About this many characters per page (Japanese paragraphs vary a lot in length).
const PAGE_CHARS: usize = 1200;

pub fn cache_path(dir: &Path, b: &Book) -> PathBuf {
    dir.join(format!("{}.txt", b.id))
}

pub fn is_downloaded(dir: &Path, b: &Book) -> bool {
    match b.source {
        Source::Builtin(_) => true,
        Source::Aozora(_) => cache_path(dir, b).exists(),
    }
}

/// Downloads an Aozora book once: the ZIP's text file, Shift_JIS → UTF-8.
pub fn download(dir: &Path, b: &Book) -> Result<PathBuf> {
    let Source::Aozora(path) = b.source else { return Ok(cache_path(dir, b)) };
    std::fs::create_dir_all(dir)?;
    let dest = cache_path(dir, b);
    if dest.exists() {
        return Ok(dest);
    }
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(120)).user_agent("SpeakUp6M/2.0 (personal reading)").build()?;
    let url = format!("https://www.aozora.gr.jp/{path}");
    let bytes = client.get(&url).send()?.error_for_status()?.bytes()?;
    let text = unzip_text(&bytes)?;
    if text.chars().count() < 300 {
        return Err(anyhow!("책을 받지 못했습니다 ({url})"));
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

/// The first .txt file in an Aozora ZIP, decoded from Shift_JIS.
pub fn unzip_text(zip_bytes: &[u8]) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.name().to_lowercase().ends_with(".txt") {
            let mut raw = Vec::new();
            f.read_to_end(&mut raw)?;
            let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&raw);
            return Ok(text.into_owned());
        }
    }
    Err(anyhow!("압축 파일에 본문이 없습니다"))
}

/// Loads a book as pages (built-in text, or a downloaded Aozora file).
pub fn load_pages(dir: &Path, b: &Book) -> Result<Vec<Page>> {
    match b.source {
        Source::Builtin(text) => Ok(vec![Page { heading: b.title.to_string(), paragraphs: paragraphs(text) }]),
        Source::Aozora(_) => {
            let raw = std::fs::read_to_string(cache_path(dir, b)).map_err(|_| anyhow!("책을 먼저 받아 주세요."))?;
            let mut pages = paginate(&aozora_chapters(&raw));
            for p in pages.iter_mut().filter(|p| p.heading.is_empty() || p.heading.starts_with(" (")) {
                p.heading = format!("{}{}", b.title, p.heading);
            }
            Ok(pages)
        }
    }
}

/// Blank-line separated paragraphs with line breaks joined.
pub fn paragraphs(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n").split("\n\n").map(|p| p.lines().map(str::trim).collect::<String>()).filter(|p| !p.is_empty()).collect()
}

/// Removes Aozora's markup from a line: ruby 《よみ》 and its ｜ start mark, annotations
/// ［＃…］ and the ※ that marks a character given only in an annotation.
fn strip_markup(line: &str) -> String {
    let mut out = String::new();
    let mut depth_ruby = 0;
    let mut depth_note = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '《' => depth_ruby += 1,
            '》' => depth_ruby = (depth_ruby - 1).max(0),
            '［' if chars.get(i + 1) == Some(&'＃') => depth_note += 1,
            '］' if depth_note > 0 => depth_note -= 1,
            '｜' => {}
            '※' if chars.get(i + 1) == Some(&'［') => {}
            _ if depth_ruby == 0 && depth_note == 0 => out.push(c),
            _ => {}
        }
        i += 1;
    }
    out.trim_matches(|c: char| c.is_whitespace() || c == '\u{3000}').to_string()
}

/// "一", "二", "（三）", "第四章", "上", "1" alone on a line: a chapter number.
fn is_number_heading(line: &str) -> bool {
    let t = line.trim_matches(|c: char| matches!(c, '（' | '）' | '(' | ')' | '　' | ' ' | '・'));
    let n = t.chars().count();
    n > 0 && n <= 8 && !t.contains('。') && t.chars().all(|c| "〇一二三四五六七八九十百上中下第章話回部編0123456789０１２３４５６７８９".contains(c))
}

/// An Aozora text as chapters (heading, paragraphs): the title block and the markup notes at
/// the top and the 底本 (source edition) notes at the end are dropped.
pub fn aozora_chapters(raw: &str) -> Vec<(String, Vec<String>)> {
    let raw = raw.replace("\r\n", "\n");
    let lines: Vec<&str> = raw.lines().collect();
    // Body: after the second line of dashes (the markup explanation), else after the title block.
    let dashes: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.starts_with("-----")).map(|(i, _)| i).collect();
    let start = match dashes.as_slice() {
        [_, second, ..] => second + 1,
        _ => lines.iter().position(|l| l.trim().is_empty()).map(|i| i + 1).unwrap_or(0),
    };
    let end = lines.iter().position(|l| l.starts_with("底本：") || l.starts_with("底本:")).unwrap_or(lines.len()).max(start);
    let mut chapters: Vec<(String, Vec<String>)> = Vec::new();
    for line in &lines[start..end] {
        let heading_note = line.contains("見出し］");
        let text = strip_markup(line);
        if text.is_empty() {
            continue;
        }
        if heading_note || is_number_heading(&text) {
            match chapters.last_mut() {
                Some((h, body)) if body.is_empty() && !h.is_empty() => {
                    h.push_str(" ");
                    h.push_str(&text);
                }
                _ => chapters.push((text, Vec::new())),
            }
            continue;
        }
        if chapters.is_empty() {
            chapters.push((String::new(), Vec::new()));
        }
        chapters.last_mut().unwrap().1.push(text);
    }
    chapters.retain(|c| !c.1.is_empty());
    chapters
}

/// Splits chapters into pages of about `PAGE_CHARS` characters.
pub fn paginate(chapters: &[(String, Vec<String>)]) -> Vec<Page> {
    let mut pages = Vec::new();
    for (heading, body) in chapters {
        let mut groups: Vec<Vec<String>> = vec![Vec::new()];
        let mut chars = 0;
        for p in body {
            let n = p.chars().count();
            if chars > 0 && chars + n > PAGE_CHARS {
                groups.push(Vec::new());
                chars = 0;
            }
            groups.last_mut().unwrap().push(p.clone());
            chars += n;
        }
        let n = groups.len();
        for (i, g) in groups.into_iter().enumerate() {
            let h = if n > 1 { format!("{heading} ({}/{n})", i + 1) } else { heading.clone() };
            pages.push(Page { heading: h, paragraphs: g });
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_books_have_paragraphs() {
        assert!(builtin().len() >= 10);
        for b in builtin() {
            let pages = load_pages(Path::new("."), b).unwrap();
            assert!(pages[0].paragraphs.len() >= 5, "{}", b.id);
            assert!(crate::ja::is_japanese(&pages[0].paragraphs[0]), "{}", b.id);
        }
    }

    #[test]
    fn aozora_text_is_cleaned_and_split_into_chapters() {
        let raw = "走れメロス\r\n太宰治\r\n\r\n-------------------------------------------------------\r\n【テキスト中に現れる記号について】\r\n《》：ルビ\r\n-------------------------------------------------------\r\n\r\n\
                   ［＃３字下げ］一［＃「一」は中見出し］\r\n\u{3000}メロスは激怒《げきど》した。必ず、かの｜邪智暴虐《じゃちぼうぎゃく》の王を除かなければならぬと決意した。\r\n\u{3000}メロスには政治がわからぬ。\r\n\r\n［＃改ページ］\r\n二\r\n\u{3000}その夜、※［＃「てへん＋劣」、第3水準1-84-77］メロスは眠った。\r\n\r\n底本：「太宰治全集」\r\n入力：誰か\r\n";
        let ch = aozora_chapters(raw);
        assert_eq!(ch.len(), 2, "{ch:#?}");
        assert_eq!(ch[0].0, "一");
        assert_eq!(ch[0].1[0], "メロスは激怒した。必ず、かの邪智暴虐の王を除かなければならぬと決意した。");
        assert_eq!(ch[1].0, "二");
        assert_eq!(ch[1].1, vec!["その夜、メロスは眠った。"]);
        assert!(!ch.iter().any(|c| c.1.iter().any(|p| p.contains("底本") || p.contains("ルビ"))));
        let pages = paginate(&ch);
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn long_chapters_become_pages() {
        let body: Vec<String> = (0..10).map(|i| format!("{i}{}", "あ".repeat(300))).collect();
        let pages = paginate(&[("一".to_string(), body)]);
        // 301 characters each: three to a page (1,200), so 3 + 3 + 3 + 1.
        assert_eq!(pages.iter().map(|p| p.paragraphs.len()).collect::<Vec<_>>(), vec![3, 3, 3, 1]);
        assert_eq!(pages[0].heading, "一 (1/4)");
    }

    /// Downloads real books and prints how they were paginated.
    /// SPEAKUP_TEST_LIBRARY=<dir> cargo test real_books -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_books_paginate() {
        let dir = PathBuf::from(std::env::var("SPEAKUP_TEST_LIBRARY").expect("SPEAKUP_TEST_LIBRARY"));
        let mut bad = Vec::new();
        for b in books().iter().filter(|b| matches!(b.source, Source::Aozora(_))) {
            if let Err(e) = download(&dir, b) {
                bad.push(format!("{}: download {e:#}", b.id));
                continue;
            }
            let pages = load_pages(&dir, b).unwrap();
            let paras: usize = pages.iter().map(|p| p.paragraphs.len()).sum();
            println!("{} {}: {} pages, {paras} paragraphs | first: {:?} | {:.60}", b.id, b.title, pages.len(), pages.first().map(|p| p.heading.as_str()), pages.first().and_then(|p| p.paragraphs.first()).map(|s| s.as_str()).unwrap_or(""));
            if paras < 3 || pages.iter().any(|p| p.paragraphs.iter().any(|x| x.contains("底本") || x.contains("テキスト中に現れる記号"))) {
                bad.push(format!("{} ({}): {} pages, {paras} paragraphs", b.id, b.title, pages.len()));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn every_level_has_books() {
        for (i, band) in crate::models::LEVEL_BANDS.iter().enumerate() {
            assert!(shelf(i).len() >= 5, "{}: {}", band.2, shelf(i).len());
        }
        let mut ids: Vec<&str> = books().iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), books().len(), "duplicate ids");
        assert!(books().len() >= 120);
    }

    #[test]
    fn recommendations_follow_level() {
        let recommended = |level: f32| shelf(crate::models::band_index(level));
        assert!(recommended(1.5).iter().all(|b| b.level == 0) && recommended(1.5).iter().any(|b| b.id == "momotaro"));
        assert!(recommended(9.5).iter().any(|b| b.title == "こころ"));
        assert!(!recommended(2.0).iter().any(|b| b.title == "こころ"));
    }
}
