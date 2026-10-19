//! A picture of the situation for a sentence (받아쓰기): Claude turns the sentence into a short
//! image search ("coffee cup on a cafe table"), and the first matching photo on Wikimedia
//! Commons (free licences, no key) is downloaded once into `scenes/` in the data folder.
//! Without Claude, the sentence's content words are searched.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One sentence's picture. `file` empty = nothing found (not searched again).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SceneImage {
    pub query: String,
    /// File name in the scenes folder.
    pub file: String,
    pub title: String,
    pub license: String,
    pub artist: String,
}

/// Words that say nothing about the picture.
const STOP: &[&str] = &[
    "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "its", "our", "their", "a", "an", "the", "is", "am", "are", "was",
    "were", "be", "been", "do", "does", "did", "have", "has", "had", "will", "would", "can", "could", "should", "to", "of", "in", "on", "at", "for", "with", "and", "or",
    "but", "so", "not", "this", "that", "what", "where", "when", "how", "why", "who", "there", "here", "very", "just", "really", "some", "any", "about", "up", "out",
    "get", "got", "go", "going", "let", "like", "if", "too", "all",
];

/// The sentence's content words, for searching without Claude.
pub fn fallback_query(sentence: &str) -> String {
    sentence
        .split(|c: char| !c.is_ascii_alphabetic() && c != '\'')
        .map(|w| w.trim_matches('\'').to_lowercase())
        .filter(|w| w.len() > 2 && !w.contains('\'') && !STOP.contains(&w.as_str()))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

/// File name for a query's picture.
fn file_name(query: &str) -> String {
    // FNV-1a: stable across runs (std's hasher isn't).
    let mut h: u64 = 0xcbf29ce484222325;
    for b in query.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}.jpg")
}

/// Strips tags from Commons' HTML credit fields.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds and downloads the first photo for `query` into `dir`.
pub fn fetch(query: &str, dir: &Path) -> Result<SceneImage> {
    let mut img = SceneImage { query: query.to_string(), ..Default::default() };
    if query.trim().is_empty() || cfg!(test) {
        return Ok(img);
    }
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).user_agent("SpeakUp6M/1.0 (English learning app)").build()?;
    let search = format!("{query} filetype:bitmap");
    let v: serde_json::Value = client
        .get("https://commons.wikimedia.org/w/api.php")
        .query(&[
            ("action", "query"),
            ("format", "json"),
            ("generator", "search"),
            ("gsrnamespace", "6"),
            ("gsrsearch", search.as_str()),
            ("gsrlimit", "5"),
            ("prop", "imageinfo"),
            ("iiprop", "url|extmetadata"),
            ("iiurlwidth", "800"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    let Some(pages) = v["query"]["pages"].as_object() else { return Ok(img) };
    // The search's best match first.
    let mut pages: Vec<&serde_json::Value> = pages.values().collect();
    pages.sort_by_key(|p| p["index"].as_i64().unwrap_or(99));
    for p in pages {
        let ii = &p["imageinfo"][0];
        let Some(url) = ii["thumburl"].as_str() else { continue };
        let bytes = client.get(url).send()?.error_for_status()?.bytes()?;
        if image::load_from_memory(&bytes).is_err() {
            continue;
        }
        std::fs::create_dir_all(dir)?;
        let name = file_name(query);
        std::fs::write(dir.join(&name), &bytes).with_context(|| format!("saving {name}"))?;
        let m = &ii["extmetadata"];
        img.file = name;
        img.title = p["title"].as_str().unwrap_or("").trim_start_matches("File:").rsplit_once('.').map(|x| x.0.to_string()).unwrap_or_default();
        img.license = plain(m["LicenseShortName"]["value"].as_str().unwrap_or(""));
        img.artist = plain(m["Artist"]["value"].as_str().unwrap_or("")).chars().take(60).collect();
        break;
    }
    Ok(img)
}

/// Decodes a saved picture for display.
pub fn load(path: &PathBuf) -> Option<egui::ColorImage> {
    let img = image::open(path).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn content_words_without_claude() {
        assert_eq!(super::fallback_query("I like coffee."), "coffee");
        assert_eq!(super::fallback_query("We had a math test today."), "math test today");
        assert_eq!(super::fallback_query("Where were you born?"), "born");
        assert_eq!(super::file_name("coffee"), super::file_name("coffee"));
    }
}
