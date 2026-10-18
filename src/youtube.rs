//! YouTube listening material via yt-dlp: downloads the audio track and English subtitles,
//! then splits the subtitles into sentence-sized segments for section repeat and shadowing.

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub url: String,
    pub audio: PathBuf,
    pub segments: Vec<Segment>,
}

pub fn ytdlp_asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    }
}

/// Finds yt-dlp: the configured path, the copy in the app's bin dir, then PATH.
pub fn find_ytdlp(configured: &str, bin_dir: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let mut candidates = Vec::new();
    if !configured.trim().is_empty() {
        candidates.push(PathBuf::from(configured.trim()));
    }
    candidates.push(bin_dir.join(exe));
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|p| p.join(exe)));
    }
    // GUI apps on macOS don't inherit the shell PATH; check Homebrew locations too.
    candidates.push(PathBuf::from("/opt/homebrew/bin/yt-dlp"));
    candidates.push(PathBuf::from("/usr/local/bin/yt-dlp"));
    candidates.into_iter().find(|p| p.is_file())
}

/// Downloads the official yt-dlp release binary for this OS into `bin_dir`.
pub fn install_ytdlp(bin_dir: &Path) -> Result<PathBuf> {
    let exe = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let dest = bin_dir.join(exe);
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", ytdlp_asset_name());
    let (done, total) = (Default::default(), Default::default());
    crate::stt::download_file(&url, &dest, &done, &total)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(dest)
}

pub fn video_id(url: &str) -> Option<String> {
    let url = url.trim();
    let candidate = if let Some(i) = url.find("v=") {
        &url[i + 2..]
    } else if let Some(i) = url.find("youtu.be/") {
        &url[i + 9..]
    } else if let Some(i) = url.find("/shorts/") {
        &url[i + 8..]
    } else if let Some(i) = url.find("/embed/") {
        &url[i + 7..]
    } else {
        url
    };
    let id: String = candidate.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    (id.len() == 11).then_some(id)
}

/// Downloads (or reuses) audio + English subtitles for a video and parses the segments.
pub fn fetch(ytdlp: &Path, url: &str, cache_dir: &Path) -> Result<Video> {
    let id = video_id(url).ok_or_else(|| anyhow!("올바른 유튜브 주소가 아닙니다."))?;
    let dir = cache_dir.join(&id);
    let meta = dir.join("video.json");
    if let Ok(s) = std::fs::read_to_string(&meta) {
        if let Ok(v) = serde_json::from_str::<Video>(&s) {
            if v.audio.exists() {
                return Ok(v);
            }
        }
    }
    std::fs::create_dir_all(&dir)?;
    let watch_url = format!("https://www.youtube.com/watch?v={id}");
    let out = Command::new(ytdlp)
        .args([
            "--no-playlist",
            "-f",
            "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio",
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs",
            "en,en-US,en-GB,en-orig",
            "--sub-format",
            "vtt",
            "--write-info-json",
            "-o",
        ])
        .arg(dir.join("audio.%(ext)s"))
        .arg(&watch_url)
        .output()
        .map_err(|e| anyhow!("yt-dlp 실행 실패: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("다운로드 실패: {}", err.lines().last().unwrap_or("알 수 없는 오류"));
    }
    let mut audio = None;
    let mut vtts = Vec::new();
    let mut title = id.clone();
    for e in std::fs::read_dir(&dir)?.flatten() {
        let p = e.path();
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.ends_with(".vtt") {
            vtts.push(p);
        } else if name.ends_with(".info.json") {
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    title = v["title"].as_str().unwrap_or(&id).to_string();
                }
            }
        } else if name.starts_with("audio.") && !name.ends_with(".part") {
            audio = Some(p);
        }
    }
    let audio = audio.ok_or_else(|| anyhow!("오디오 파일을 받지 못했습니다."))?;
    if audio.extension().is_some_and(|e| e == "webm" || e == "opus") {
        bail!("이 영상은 webm/opus 오디오만 제공되어 재생할 수 없습니다. 다른 영상을 선택해 주세요.");
    }
    // Prefer human-made subtitles ("audio.en.vtt") over auto-generated ("audio.en-orig.vtt").
    vtts.sort_by_key(|p| {
        let n = p.to_string_lossy().to_string();
        (n.contains("orig"), n.len())
    });
    let vtt = vtts.first().ok_or_else(|| anyhow!("이 영상에는 영어 자막이 없습니다. 자막이 있는 영상을 선택해 주세요."))?;
    let segments = merge_into_sentences(&parse_vtt(&std::fs::read_to_string(vtt)?));
    if segments.is_empty() {
        bail!("자막을 해석하지 못했습니다.");
    }
    let video = Video { id, title, url: watch_url, audio, segments };
    std::fs::write(&meta, serde_json::to_string_pretty(&video)?)?;
    Ok(video)
}

/// Previously downloaded videos, newest first.
/// Deletes a downloaded video (audio, subtitles, video.json). Only plain YouTube ids are
/// accepted, so nothing outside the cache folder can be removed.
pub fn delete(cache_dir: &Path, id: &str) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(anyhow!("잘못된 영상 id: {id}"));
    }
    let dir = cache_dir.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn library(cache_dir: &Path) -> Vec<Video> {
    let mut v: Vec<(std::time::SystemTime, Video)> = std::fs::read_dir(cache_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path().join("video.json");
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            let v = serde_json::from_str::<Video>(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some((t, v))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, v)| v).collect()
}

fn parse_time(t: &str) -> Option<f32> {
    let parts: Vec<&str> = t.trim().split(':').collect();
    let secs: f32 = parts.last()?.replace(',', ".").parse().ok()?;
    let mins: f32 = parts.iter().rev().nth(1).map(|m| m.parse().unwrap_or(0.0)).unwrap_or(0.0);
    let hours: f32 = parts.iter().rev().nth(2).map(|h| h.parse().unwrap_or(0.0)).unwrap_or(0.0);
    Some(hours * 3600.0 + mins * 60.0 + secs)
}

fn strip_tags(line: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses WebVTT into cues. YouTube auto-captions repeat each line in the following cue
/// ("rolling" captions); repeated lines are dropped so every spoken line appears once.
pub fn parse_vtt(src: &str) -> Vec<Segment> {
    let mut cues = Vec::new();
    let mut last_line = String::new();
    for block in src.replace("\r\n", "\n").split("\n\n") {
        let mut lines = block.lines();
        let Some(timing) = lines.by_ref().find(|l| l.contains("-->")) else { continue };
        let mut it = timing.split("-->");
        let (Some(a), Some(b)) = (it.next(), it.next()) else { continue };
        let (Some(start), Some(end)) = (parse_time(a), parse_time(b.split_whitespace().next().unwrap_or(""))) else {
            continue;
        };
        for raw in lines {
            let text = strip_tags(raw);
            if text.is_empty() || text == last_line {
                continue;
            }
            last_line = text.clone();
            cues.push(Segment { start, end, text });
        }
    }
    cues
}

/// Joins cues into sentence-sized segments (ending at . ? ! or after ~7 s / 20 words).
pub fn merge_into_sentences(cues: &[Segment]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut cur: Option<Segment> = None;
    for c in cues {
        let seg = cur.get_or_insert_with(|| Segment { start: c.start, end: c.end, text: String::new() });
        if !seg.text.is_empty() {
            seg.text.push(' ');
        }
        seg.text.push_str(&c.text);
        seg.end = seg.end.max(c.end);
        let ends_sentence = seg.text.ends_with(['.', '?', '!']);
        if ends_sentence || seg.end - seg.start > 7.0 || seg.text.split_whitespace().count() > 20 {
            out.push(cur.take().unwrap());
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out.retain(|s| s.text.chars().any(|c| c.is_alphabetic()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_only_inside_the_cache() {
        let cache = std::env::temp_dir().join(format!("speakup-yt-del-{}", std::process::id()));
        let dir = cache.join("abc_DEF-123");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.json"), "{}").unwrap();
        assert!(delete(&cache, "../etc").is_err() && delete(&cache, "").is_err() && delete(&cache, "a/b").is_err());
        delete(&cache, "abc_DEF-123").unwrap();
        assert!(!dir.exists() && cache.exists());
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn ids() {
        assert_eq!(video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ?si=x").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("not a url"), None);
    }

    #[test]
    fn parses_rolling_auto_captions() {
        let vtt = "WEBVTT\nKind: captions\n\n00:00:00.000 --> 00:00:02.000 align:start\nhello<00:00:00.500><c> everyone</c>\n\n\
                   00:00:02.000 --> 00:00:02.010\nhello everyone\n\n00:00:02.010 --> 00:00:04.000\nhello everyone\nwelcome<00:00:02.500><c> back.</c>\n";
        let cues = parse_vtt(vtt);
        let texts: Vec<_> = cues.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["hello everyone", "welcome back."]);
        let merged = merge_into_sentences(&cues);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "hello everyone welcome back.");
        assert_eq!(merged[0].start, 0.0);
    }

    #[test]
    fn time_formats() {
        assert_eq!(parse_time("01:02.500"), Some(62.5));
        assert_eq!(parse_time("1:00:00.000"), Some(3600.0));
    }
}
