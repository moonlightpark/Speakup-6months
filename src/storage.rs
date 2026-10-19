//! JSON-file storage in the per-user data directory.
//!
//! Layout:
//!   settings.json      app settings (API key, model, thresholds)
//!   profile.json       user level, level history, streak, study time
//!   level_log.csv      human-readable level history (one line per change)
//!   items.json         spaced-repetition items (learned words / sentences)
//!   homework.json      assigned homework and its check results
//!   attempts.jsonl     every spoken attempt (target, transcript, score)
//!   lessons/DATE.json  the generated lesson for each day
//!   library.json       reading-library progress per book; library/ holds downloaded books
//!   writing.json       writing exercises (dictation, translation, free writing)
//!   models/            whisper speech-recognition and Kokoro voice models
//!   youtube/ID/        downloaded YouTube audio + English subtitles

use crate::models::*;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Store {
    pub dir: PathBuf,
    pub settings: Settings,
    pub profile: Profile,
    pub items: Vec<StudyItem>,
    pub homework: Vec<Homework>,
    pub mistakes: Vec<Mistake>,
    pub corrections: Vec<CorrectionRecord>,
    /// 스피드 리딩 scripts by program day (speed_scripts.json): the same 회차 always reads the same.
    pub speed_scripts: std::collections::BTreeMap<u32, crate::claude::SpeedPassage>,
    /// 직독직해 glosses by sentence text.
    pub gloss: std::collections::HashMap<String, Vec<Chunk>>,
    /// 시제 3문장 for new words, by lower-case word (tenses.json, written by Claude).
    pub tenses: std::collections::HashMap<String, crate::tenses::TenseSet>,
    pub history: Vec<DayRecord>,
    /// Reading library progress by book id.
    pub library: std::collections::HashMap<String, BookProgress>,
    pub writings: Vec<WritingRecord>,
    /// 새 단어 practice counts by lower-case text (word_stats.json).
    pub word_stats: std::collections::HashMap<String, WordStat>,
    /// Each day's 새 단어 list (word_days.json), so words left unfinished can be found later.
    pub word_days: std::collections::BTreeMap<chrono::NaiveDate, WordSet>,
    /// Each day's idiom sentences for 문장 읽기 (idiom_days.json, numbers in the built-in list).
    pub idiom_days: std::collections::BTreeMap<chrono::NaiveDate, Vec<u32>>,
    /// Earlier days' lessons, read once per day (screens ask for them every frame).
    lessons_cache: std::cell::RefCell<Option<(chrono::NaiveDate, Vec<Lesson>)>>,
}

fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            crate::applog!("storage: {} 읽기 실패 · {e}", path.display());
            // Keep the unreadable file so the user's data is never silently lost.
            let _ = fs::copy(path, path.with_extension("json.broken"));
            T::default()
        }),
        Err(_) => T::default(),
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(value)?)?;
    fs::rename(&tmp, path).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("SPEAKUP_DATA_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("com", "SpeakUp", "SpeakUp6M")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("speakup-data"))
}

impl Store {
    pub fn open() -> Result<Self> {
        Self::open_at(data_dir())
    }

    pub fn open_at(dir: PathBuf) -> Result<Self> {
        for sub in ["", "lessons", "models", "youtube", "bin", "library", "recordings"] {
            fs::create_dir_all(dir.join(sub))?;
        }
        Ok(Self {
            settings: load_json(&dir.join("settings.json")),
            profile: load_json(&dir.join("profile.json")),
            items: load_json(&dir.join("items.json")),
            homework: load_json(&dir.join("homework.json")),
            mistakes: load_json(&dir.join("mistakes.json")),
            corrections: load_json(&dir.join("corrections.json")),
            gloss: load_json(&dir.join("gloss.json")),
            tenses: load_json(&dir.join("tenses.json")),
            speed_scripts: load_json(&dir.join("speed_scripts.json")),
            history: load_json(&dir.join("history.json")),
            library: load_json(&dir.join("library.json")),
            writings: load_json(&dir.join("writing.json")),
            word_stats: load_json(&dir.join("word_stats.json")),
            word_days: load_json(&dir.join("word_days.json")),
            idiom_days: load_json(&dir.join("idiom_days.json")),
            lessons_cache: Default::default(),
            dir,
        })
    }

    /// Counts one 새 단어 attempt (and a pass when `passed`), then saves word_stats.json.
    pub fn record_word_practice(&mut self, text: &str, passed: bool) {
        let st = self.word_stats.entry(text.trim().to_lowercase()).or_default();
        st.attempts += 1;
        st.passes += passed as u32;
        st.last = Some(today());
        log_err(save_json(&self.dir.join("word_stats.json"), &self.word_stats));
    }

    /// Keeps a day's 새 단어 list in word_days.json.
    pub fn record_word_day(&mut self, date: chrono::NaiveDate, set: &WordSet) {
        if self.word_days.get(&date) != Some(set) {
            self.word_days.insert(date, set.clone());
            log_err(save_json(&self.dir.join("word_days.json"), &self.word_days));
        }
    }

    /// Keeps a day's idiom sentences in idiom_days.json.
    pub fn record_idiom_day(&mut self, date: chrono::NaiveDate, nos: &[u32]) {
        if self.idiom_days.get(&date).map(|v| v.as_slice()) != Some(nos) {
            self.idiom_days.insert(date, nos.to_vec());
            log_err(save_json(&self.dir.join("idiom_days.json"), &self.idiom_days));
        }
    }

    /// Counts a 새 단어 entry as fully practised (학습완료 for a word that is too easy).
    pub fn mark_word_done(&mut self, text: &str) {
        let st = self.word_stats.entry(text.trim().to_lowercase()).or_default();
        st.passes = st.passes.max(WORD_DONE_PASSES);
        st.last = Some(today());
        log_err(save_json(&self.dir.join("word_stats.json"), &self.word_stats));
    }

    pub fn word_stat(&self, text: &str) -> WordStat {
        self.word_stats.get(&text.trim().to_lowercase()).cloned().unwrap_or_default()
    }

    pub fn save_settings(&self) {
        log_err(save_json(&self.dir.join("settings.json"), &self.settings));
    }
    pub fn save_profile(&self) {
        log_err(save_json(&self.dir.join("profile.json"), &self.profile));
    }
    pub fn save_items(&self) {
        log_err(save_json(&self.dir.join("items.json"), &self.items));
    }
    pub fn save_homework(&self) {
        log_err(save_json(&self.dir.join("homework.json"), &self.homework));
    }

    /// Stores a gloss if its chunks really spell out the sentence (otherwise word colouring
    /// would misalign).
    pub fn put_tenses(&mut self, sets: &[crate::tenses::TenseSet]) {
        for t in sets.iter().filter(|t| t.is_complete()) {
            self.tenses.insert(t.word.to_lowercase(), t.clone());
        }
        log_err(save_json(&self.dir.join("tenses.json"), &self.tenses));
    }

    pub fn put_speed_script(&mut self, day: u32, p: &crate::claude::SpeedPassage) {
        self.speed_scripts.insert(day, p.clone());
        log_err(save_json(&self.dir.join("speed_scripts.json"), &self.speed_scripts));
    }

    pub fn put_gloss(&mut self, text: &str, chunks: &[Chunk]) {
        if chunks_match(text, chunks) {
            self.gloss.insert(text.trim().to_string(), chunks.to_vec());
            log_err(save_json(&self.dir.join("gloss.json"), &self.gloss));
        }
    }

    /// A saved gloss; ones saved before grammatical roles existed count as missing so they are
    /// fetched again with colours.
    pub fn gloss_for(&self, text: &str) -> Vec<Chunk> {
        self.gloss.get(text.trim()).filter(|c| c.iter().any(|c| !c.role.is_empty())).cloned().unwrap_or_default()
    }

    /// Weekly speaking recordings (WAV).
    pub fn recordings_dir(&self) -> PathBuf {
        self.dir.join("recordings")
    }

    pub fn save_library(&self) {
        log_err(save_json(&self.dir.join("library.json"), &self.library));
    }
    pub fn save_writings(&self) {
        log_err(save_json(&self.dir.join("writing.json"), &self.writings));
    }
    pub fn library_dir(&self) -> PathBuf {
        self.dir.join("library")
    }

    pub fn save_history(&self) {
        log_err(save_json(&self.dir.join("history.json"), &self.history));
    }

    /// Inserts or replaces the record for `rec.date`.
    pub fn upsert_day(&mut self, rec: DayRecord) {
        match self.history.iter_mut().find(|r| r.date == rec.date) {
            Some(r) => *r = rec,
            None => {
                self.history.push(rec);
                self.history.sort_by_key(|r| r.date);
            }
        }
        self.save_history();
    }

    /// Lessons from earlier days, newest first (used so new lessons don't repeat them).
    pub fn past_lessons(&self, limit: usize) -> Vec<Lesson> {
        let mut cache = self.lessons_cache.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.0 != today()) {
            *cache = Some((today(), self.read_past_lessons()));
        }
        cache.as_ref().map(|c| c.1.iter().take(limit).cloned().collect()).unwrap_or_default()
    }

    /// All lessons before today, newest first.
    fn read_past_lessons(&self) -> Vec<Lesson> {
        let mut files: Vec<_> = std::fs::read_dir(self.dir.join("lessons"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json") && !p.ends_with(format!("{}.json", today())))
            .collect();
        files.sort();
        files.reverse();
        files
            .into_iter()
            .filter_map(|p| serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok())
            .collect()
    }

    pub fn save_mistakes(&self) {
        log_err(save_json(&self.dir.join("mistakes.json"), &self.mistakes));
    }
    pub fn save_corrections(&self) {
        log_err(save_json(&self.dir.join("corrections.json"), &self.corrections));
    }

    /// Counts words the recognizer didn't hear correctly (likely pronunciation problems).
    pub fn add_mistakes(&mut self, words: &[String]) {
        if words.is_empty() {
            return;
        }
        for w in words {
            let w: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '’').collect::<String>().to_lowercase();
            if w.chars().count() < 3 {
                continue; // tiny function words are mostly recognizer noise
            }
            match self.mistakes.iter_mut().find(|m| m.word == w) {
                Some(m) => {
                    m.count += 1;
                    m.last = today();
                    m.fixed_streak = 0;
                }
                None => self.mistakes.push(Mistake { word: w, count: 1, last: today(), fixed_streak: 0 }),
            }
        }
        self.save_mistakes();
    }

    pub fn add_correction(&mut self, source: &str, original: &str, corrected: &str, explanation_ko: &str) {
        if corrected.trim().is_empty() || corrected.trim().eq_ignore_ascii_case(original.trim()) {
            return;
        }
        self.corrections.push(CorrectionRecord {
            date: today(),
            source: source.into(),
            original: original.into(),
            corrected: corrected.into(),
            explanation_ko: explanation_ko.into(),
            practiced: false,
        });
        self.save_corrections();
    }

    pub fn lesson_path(&self, date: NaiveDate) -> PathBuf {
        self.dir.join("lessons").join(format!("{date}.json"))
    }

    pub fn load_lesson(&self, date: NaiveDate) -> Option<Lesson> {
        let s = fs::read_to_string(self.lesson_path(date)).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save_lesson(&self, lesson: &Lesson) {
        if let Some(d) = lesson.date {
            log_err(save_json(&self.lesson_path(d), lesson));
            self.lessons_cache.replace(None);
        }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.dir.join("models")
    }

    pub fn youtube_dir(&self) -> PathBuf {
        self.dir.join("youtube")
    }

    /// Record a level change both in profile.json and in the plain-text level_log.csv.
    pub fn record_level(&mut self, level: f32, reason: &str) {
        let level = level.clamp(1.0, 10.0);
        self.profile.level = level;
        let rec = LevelRecord {
            date: today(),
            level,
            cefr: cefr_of(level).to_string(),
            reason: reason.to_string(),
        };
        let path = self.dir.join("level_log.csv");
        let new_file = !path.exists();
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            if new_file {
                let _ = writeln!(f, "date,level,cefr,reason");
            }
            let _ = writeln!(f, "{},{:.2},{},\"{}\"", rec.date, rec.level, rec.cefr, rec.reason.replace('"', "'"));
        }
        self.profile.level_history.push(rec);
        self.save_profile();
    }

    pub fn log_attempt(&self, category: &str, target: &str, transcript: &str, score: f32) {
        let line = serde_json::json!({
            "time": chrono::Local::now().to_rfc3339(),
            "category": category,
            "target": target,
            "transcript": transcript,
            "score": score,
        });
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.dir.join("attempts.jsonl")) {
            let _ = writeln!(f, "{line}");
        }
    }
}

pub fn chunks_match(text: &str, chunks: &[Chunk]) -> bool {
    let joined: Vec<String> = chunks.iter().map(|c| c.en.clone()).collect();
    !chunks.is_empty()
        && text.split_whitespace().count() == joined.join(" ").split_whitespace().count()
        && crate::scoring::normalize(text) == crate::scoring::normalize(&joined.join(" "))
}

fn log_err(r: Result<()>) {
    if let Err(e) = r {
        crate::applog!("storage: 저장 실패 · {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(en: &str, ko: &str) -> Chunk {
        Chunk { en: en.into(), ko: ko.into(), role: "subject".into() }
    }

    #[test]
    fn gloss_must_spell_the_sentence() {
        let good = vec![ch("I", "나"), ch("am", "이다"), ch("a", "한"), ch("boy.", "소년")];
        assert!(chunks_match("I am a boy.", &good));
        let phrase = vec![ch("I", "나는"), ch("wake up", "일어난다"), ch("early.", "일찍")];
        assert!(chunks_match("I wake up early.", &phrase));
        let wrong = vec![ch("I", "나"), ch("am", "이다"), ch("boy", "소년")];
        assert!(!chunks_match("I am a boy.", &wrong));
        assert!(!chunks_match("I am a boy.", &[]));
    }

    #[test]
    fn mistakes_and_gloss_persist() {
        let dir = std::env::temp_dir().join(format!("speakup-test-{}", std::process::id()));
        let mut s = Store::open_at(dir.clone()).unwrap();
        s.add_mistakes(&["Think".into(), "a".into(), "think".into()]);
        s.put_gloss("I am a boy.", &[ch("I", "나"), ch("am", "이다"), ch("a", "한"), ch("boy.", "소년")]);
        s.record_level(4.2, "test");
        let s2 = Store::open_at(dir.clone()).unwrap();
        assert_eq!(s2.mistakes.len(), 1);
        assert_eq!(s2.mistakes[0].count, 2);
        assert_eq!(s2.gloss_for("I am a boy.").len(), 4);
        assert_eq!(s2.profile.level, 4.2);
        assert!(std::fs::read_to_string(dir.join("level_log.csv")).unwrap().contains("4.20,B1"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
