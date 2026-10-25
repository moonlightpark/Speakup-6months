//! A picture of the situation for a sentence (받아쓰기): Claude turns the sentence into a short
//! image search ("coffee cup on a cafe table"), and the first matching photo on Wikimedia
//! Commons (free licences, no key) is downloaded once into `scenes/` in the data folder.
//! Without Claude, the sentence's nouns are searched.

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

/// Nouns that say nothing about the picture.
const STOP: &[&str] = &["こと", "もの", "ところ", "とき", "時", "人", "方", "今日", "明日", "昨日", "今", "私", "わたし", "あなた", "これ", "それ", "あれ", "ここ", "そこ", "何", "なに"];

/// The sentence's nouns, for searching without Claude (Commons describes many photos in Japanese too).
pub fn fallback_query(sentence: &str) -> String {
    crate::ja::tokenize(sentence)
        .into_iter()
        .filter(|t| t.pos == "名詞" && !matches!(t.pos1.as_str(), "代名詞" | "非自立" | "数" | "接尾" | "副詞可能" | "形容動詞語幹") && !STOP.contains(&t.surface.as_str()))
        .map(|t| t.surface)
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
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).user_agent("SpeakUp6M/2.0 (Japanese learning app)").build()?;
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
        assert_eq!(super::fallback_query("コーヒーが好きです。"), "コーヒー");
        assert_eq!(super::fallback_query("今日は数学のテストがありました。"), "数学 テスト");
        assert_eq!(super::fallback_query("駅の近くに大きい公園があります。"), "駅 公園");
        assert_eq!(super::file_name("coffee"), super::file_name("coffee"));
    }
}
