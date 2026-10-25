//! Text-to-speech — the natural neural voice (VOICEVOX Nemo, voicevox.rs) once it is installed,
//! otherwise the operating system's Japanese voices (macOS AVFoundation: Kyoko, Otoya …;
//! Windows WinRT: Haruka, Ayumi …) — plus the shared voice I/O bundle used by every screen.
//! (The English app's neural voice, Kokoro, can't read kanji.)

use crate::audio::{Player, Recorder};
use crate::language::LanguagePack;
use crate::stt::Stt;
use tts::Tts;

pub struct Speaker {
    tts: Option<Tts>,
    pub voices: Vec<(String, String)>, // (id, label)
    pub error: Option<String>,
    /// A high-quality (enhanced / premium, neural) OS voice is installed.
    pub natural: bool,
    /// Natural neural voice (VOICEVOX); used instead of the OS voice once it is installed.
    pub neural: Option<crate::voicevox::VoiceVox>,
    pub use_neural: bool,
    pub neural_voice: u32,
}

impl Speaker {
    pub fn new(lang: &LanguagePack, preferred_id: &str) -> Self {
        let mut tts = match Tts::default() {
            Ok(t) => t,
            Err(e) => {
                return Self {
                    tts: None,
                    voices: Vec::new(),
                    error: Some(format!("음성 출력(TTS)을 사용할 수 없습니다: {e}")),
                    natural: false,
                    neural: None,
                    use_neural: true,
                    neural_voice: 0,
                };
            }
        };
        let mut voices = Vec::new();
        let mut natural = false;
        if let Ok(list) = tts.voices() {
            let mut matching: Vec<_> = list
                .into_iter()
                .filter(|v| v.language().as_str().to_lowercase().starts_with(lang.tts_lang_prefix))
                .collect();
            // Best voices first: the natural ones, then the standard reading voices; the
            // novelty voices (macOS Eloquence: Eddy, Grandma …) last.
            matching.sort_by_key(|v| (voice_rank(&v.id(), &v.name()), !v.language().as_str().eq_ignore_ascii_case(lang.tts_preferred), v.name()));
            let chosen = matching.iter().find(|v| v.id() == preferred_id).or(matching.first());
            if let Some(v) = chosen {
                crate::applog!("tts: {} ({})", v.name(), v.id());
                let _ = tts.set_voice(v);
            }
            natural = matching.first().is_some_and(|v| voice_rank(&v.id(), &v.name()) <= 1);
            voices = matching.iter().map(|v| (v.id(), voice_label(&v.id(), &v.name(), v.language().as_str()))).collect();
        }
        let error = voices.is_empty().then(|| format!("{} 음성이 설치되어 있지 않습니다. {}", lang.name_ko, INSTALL_VOICE_HELP));
        Self { tts: Some(tts), voices, error, natural, neural: None, use_neural: true, neural_voice: 0 }
    }

    /// No speech output (automated tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        Self { tts: None, voices: Vec::new(), error: None, natural: false, neural: None, use_neural: false, neural_voice: 0 }
    }

    /// True when speech goes to the natural neural voice (installed and switched on).
    pub fn neural_active(&self) -> bool {
        self.use_neural && self.neural.as_ref().is_some_and(|n| n.is_ready())
    }

    pub fn set_voice(&mut self, id: &str) {
        let Some(tts) = &mut self.tts else { return };
        if let Ok(list) = tts.voices() {
            if let Some(v) = list.iter().find(|v| v.id() == id) {
                let _ = tts.set_voice(v);
            }
        }
    }

    /// Speaks `text`, interrupting anything already playing. `speed` is relative (1.0 = normal).
    pub fn say(&mut self, text: &str, speed: f32) {
        let text = &speakable(text);
        if self.neural_active() {
            if let Some(t) = &mut self.tts {
                let _ = t.stop();
            }
            if let Some(n) = &self.neural {
                n.say(text, self.neural_voice, speed);
            }
            return;
        }
        let Some(tts) = &mut self.tts else { return };
        let (min, normal, max) = (tts.min_rate(), tts.normal_rate(), tts.max_rate());
        let rate = if speed >= 1.0 { normal + (max - normal) * (speed - 1.0) * 0.5 } else { normal - (normal - min) * (1.0 - speed) };
        let _ = tts.set_rate(rate.clamp(min, max));
        if let Err(e) = tts.speak(text, true) {
            self.error = Some(format!("TTS 오류: {e}"));
        }
    }

    pub fn stop(&mut self) {
        if let Some(t) = &mut self.tts {
            let _ = t.stop();
        }
        if let Some(n) = &self.neural {
            n.stop();
        }
    }

    /// Synthesizes ahead of time so pressing 🔊 later plays instantly (neural voice only).
    pub fn prefetch(&self, text: &str, speed: f32) {
        if self.neural_active() {
            if let Some(n) = &self.neural {
                n.prefetch(&speakable(text), self.neural_voice, speed);
            }
        }
    }
}

/// 0 = premium, 1 = enhanced (both neural, natural), 2 = the standard reading voices (Kyoko,
/// Otoya, Windows Haruka …), 3 = others, 4 = novelty voices (macOS Eloquence).
fn voice_rank(id: &str, name: &str) -> u8 {
    let id = id.to_lowercase();
    if id.contains("premium") {
        0
    } else if id.contains("enhanced") {
        1
    } else if id.contains("eloquence") || id.contains("speech.synthesis") {
        4
    } else if ["Kyoko", "Otoya", "Haruka", "Ayumi", "Ichiro", "Sayaka", "Nanami", "Keita"].iter().any(|n| name.contains(n)) {
        2
    } else {
        3
    }
}

fn voice_label(id: &str, name: &str, lang: &str) -> String {
    let quality = match voice_rank(id, name) {
        0 => " · 최고 품질",
        1 => " · 고품질",
        2 => " · 기본 품질",
        4 => " · 효과음 음성",
        _ => "",
    };
    format!("{name} ({lang}){quality}")
}

/// How to add a natural Japanese voice to the OS (the enhanced/premium voices sound far better).
pub const INSTALL_VOICE_HELP: &str = if cfg!(target_os = "macos") {
    "시스템 설정 › 손쉬운 사용 › 읽기 및 말하기 › 시스템 음성 › 음성 관리… › 일본어에서 'Kyoko (향상됨/프리미엄)' 또는 'Otoya (향상됨/프리미엄)'을 받으면(무료) 훨씬 자연스러워요. 받은 뒤 앱을 다시 켜세요."
} else {
    "Windows 설정 › 시간 및 언어 › 음성 › 음성 추가에서 '일본어(일본)'를 설치하세요 (Haruka · Ayumi · Ichiro). 설치 후 앱을 다시 켜세요."
};

/// Splits text into sentences (one per line in passages, one at a time for the voice). Very
/// long sentences are split again at a comma.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let long_enough = |s: &str| s.split_whitespace().count() >= 2 || crate::ja::char_count(s) >= 2;
    for (i, c) in chars.iter().enumerate() {
        cur.push(*c);
        let next = chars.get(i + 1);
        // Japanese sentence ends (。！？) need no space after them; a closing quote stays with its sentence.
        let ja_end = matches!(c, '。' | '！' | '？') && !next.is_some_and(|n| matches!(n, '」' | '』' | '）' | '！' | '？'));
        let quote_end = matches!(c, '」' | '』') && i > 0 && matches!(chars[i - 1], '。' | '！' | '？');
        let end = ja_end || quote_end || (matches!(c, '.' | '!' | '?' | ';' | '\n') && next.is_none_or(|n| n.is_whitespace() || *n == '"' || *n == '\''));
        if end && long_enough(cur.trim()) {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out.into_iter().flat_map(|s| split_long(&s)).collect()
}

const MAX_WORDS: usize = 20;
/// Japanese sentences longer than this many characters are split at a comma.
const MAX_CHARS: usize = 60;

/// Breaks a long sentence into pieces: English at most `MAX_WORDS` words (after a comma, then
/// before a conjunction), Japanese at most `MAX_CHARS` characters (after 、), so each piece
/// still sounds like a natural phrase.
fn split_long(sentence: &str) -> Vec<String> {
    if crate::ja::is_japanese(sentence) {
        let chars: Vec<char> = sentence.chars().collect();
        if chars.len() <= MAX_CHARS {
            return vec![sentence.to_string()];
        }
        let mut out = Vec::new();
        let mut start = 0;
        while chars.len() - start > MAX_CHARS {
            let cut = (start + 8..start + MAX_CHARS).rev().find(|&i| chars[i - 1] == '、').unwrap_or(start + MAX_CHARS);
            out.push(chars[start..cut].iter().collect());
            start = cut;
        }
        out.push(chars[start..].iter().collect());
        return out;
    }
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if words.len() <= MAX_WORDS {
        return vec![sentence.to_string()];
    }
    const JOIN: &[&str] = &["and", "but", "so", "because", "or", "which", "while", "when", "then", "that"];
    let mut out = Vec::new();
    let mut start = 0;
    while words.len() - start > MAX_WORDS {
        let window = start + 4..start + MAX_WORDS; // never leave a tiny piece
        let cut = window.clone().rev().find(|&i| words[i - 1].ends_with(',')).or_else(|| window.clone().rev().find(|&i| JOIN.contains(&words[i].to_lowercase().as_str()))).unwrap_or(start + MAX_WORDS);
        out.push(words[start..cut].join(" "));
        start = cut;
    }
    out.push(words[start..].join(" "));
    out
}



/// Text as it should be read aloud: icons such as 💬 🔗 🔊 ✓ are dropped, so the voice never
/// says "speech balloon". Letters (any language), digits and ordinary punctuation (Japanese
/// 。、「」 too, for natural pauses) are kept.
pub fn speakable(text: &str) -> String {
    let keep = |c: char| c.is_alphanumeric() || c.is_whitespace() || c.is_ascii_punctuation() || "‘’“”–—…·。、！？「」『』（）・ー〜".contains(c);
    text.chars().filter(|c| keep(*c)).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod voice_rank_tests {
    #[test]
    fn natural_voices_come_first_and_novelty_last() {
        use super::voice_rank as r;
        assert!(r("com.apple.voice.premium.ja-JP.Kyoko", "Kyoko") < r("com.apple.voice.enhanced.ja-JP.Otoya", "Otoya"));
        assert!(r("com.apple.voice.enhanced.ja-JP.Otoya", "Otoya") < r("com.apple.voice.compact.ja-JP.Kyoko", "Kyoko"));
        assert!(r("com.apple.voice.compact.ja-JP.Kyoko", "Kyoko") < r("com.apple.eloquence.ja-JP.Eddy", "Eddy"));
        assert_eq!(r("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech_OneCore\\Voices\\Tokens\\MSTTS_V110_jaJP_HarukaM", "Microsoft Haruka"), 2);
    }
}

#[cfg(test)]
mod speakable_tests {
    #[test]
    fn icons_are_not_read_aloud() {
        assert_eq!(super::speakable("💬 It's my first time"), "It's my first time");
        assert_eq!(super::speakable("🔗 It depends on"), "It depends on");
        assert_eq!(super::speakable("✓ Done! ❤️ \u{1F9D1}\u{200D}\u{1F3EB} Nice — “quote” 3:30"), "Done! Nice — “quote” 3:30");
        assert_eq!(super::speakable("이번이 처음이야 😆"), "이번이 처음이야");
        assert_eq!(super::speakable("🔊 「はい」、そうです！"), "「はい」、そうです！");
    }
}

/// Everything audio-related, owned by the app and lent to screens.
pub struct Voice {
    pub speaker: Speaker,
    pub recorder: Recorder,
    pub player: Player,
    pub stt: Stt,
    /// Korean speech recognition (multilingual whisper model) for the translator.
    pub stt_ko: Stt,
    pub base_rate: f32,
    /// Last learner recording (16 kHz) for "listen to myself".
    pub last_recording: Vec<f32>,
}

impl Voice {
    pub fn say(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate;
        self.speaker.say(text, r);
    }
    /// Speaks at `factor` × the voice speed set in settings (받아쓰기: the level's pace).
    pub fn say_at(&mut self, text: &str, factor: f32) {
        self.player.stop();
        let r = self.base_rate * factor;
        self.speaker.say(text, r);
    }
    pub fn say_slow(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate * if self.speaker.neural_active() { 0.75 } else { 0.7 };
        self.speaker.say(text, r);
    }
    pub fn say_fast(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate * if self.speaker.neural_active() { 1.3 } else { 1.35 };
        self.speaker.say(text, r);
    }
    pub fn play_last_recording(&mut self) {
        self.speaker.stop();
        let rec = self.last_recording.clone();
        self.player.play_samples(rec, crate::audio::STT_RATE);
    }
}
