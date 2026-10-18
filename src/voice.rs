//! Text-to-speech through the operating system's voices (macOS AVFoundation, Windows WinRT,
//! Linux speech-dispatcher), plus the shared voice I/O bundle used by every screen.

use crate::audio::{Player, Recorder};
use crate::language::LanguagePack;
use crate::stt::Stt;
use crate::neural_tts::{NeuralStatus, NeuralTts};
use tts::Tts;

pub struct Speaker {
    tts: Option<Tts>,
    pub voices: Vec<(String, String)>, // (id, label)
    pub error: Option<String>,
    /// Natural neural voice (Kokoro); used instead of the OS voice once its model is loaded.
    pub neural: Option<NeuralTts>,
    pub use_neural: bool,
    pub neural_voice: String,
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
                    neural: None,
                    use_neural: true,
                    neural_voice: String::new(),
                };
            }
        };
        let mut voices = Vec::new();
        if let Ok(list) = tts.voices() {
            let mut matching: Vec<_> = list
                .into_iter()
                .filter(|v| v.language().as_str().to_lowercase().starts_with(lang.tts_lang_prefix))
                .collect();
            matching.sort_by_key(|v| (!v.language().as_str().eq_ignore_ascii_case(lang.tts_preferred), v.name()));
            let chosen = matching
                .iter()
                .find(|v| v.id() == preferred_id)
                .or_else(|| matching.iter().find(|v| v.language().as_str().eq_ignore_ascii_case(lang.tts_preferred)))
                .or(matching.first());
            if let Some(v) = chosen {
                let _ = tts.set_voice(v);
            }
            voices = matching.iter().map(|v| (v.id(), format!("{} ({})", v.name(), v.language().as_str()))).collect();
        }
        Self { tts: Some(tts), voices, error: None, neural: None, use_neural: true, neural_voice: String::new() }
    }

    /// True when speech will come from the natural neural voice.
    pub fn neural_active(&self) -> bool {
        self.use_neural && self.neural.as_ref().is_some_and(|n| n.is_ready())
    }

    /// True when speech should go to the neural voice: ready, or still loading at startup
    /// (the request waits a moment instead of falling back to the robotic OS voice).
    pub fn neural_wanted(&self) -> bool {
        self.use_neural && self.neural.as_ref().is_some_and(|n| matches!(n.status(), NeuralStatus::Ready | NeuralStatus::Loading))
    }

    /// No speech output (automated tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        Self { tts: None, voices: Vec::new(), error: None, neural: None, use_neural: false, neural_voice: String::new() }
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
        if self.neural_wanted() {
            if let Some(t) = &mut self.tts {
                let _ = t.stop();
            }
            if let Some(n) = &self.neural {
                n.say(text, &self.neural_voice, speed);
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
        let text = &speakable(text);
        if self.neural_active() {
            if let Some(n) = &self.neural {
                n.prefetch(text, &self.neural_voice, speed);
            }
        }
    }
}

/// Text as it should be read aloud: icons such as 💬 🔗 🔊 ✓ are dropped, so the voice never
/// says "speech balloon". Letters (any language), digits and ordinary punctuation are kept.
pub fn speakable(text: &str) -> String {
    let keep = |c: char| c.is_alphanumeric() || c.is_whitespace() || c.is_ascii_punctuation() || "‘’“”–—…·".contains(c);
    text.chars().filter(|c| keep(*c)).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod speakable_tests {
    #[test]
    fn icons_are_not_read_aloud() {
        assert_eq!(super::speakable("💬 It's my first time"), "It's my first time");
        assert_eq!(super::speakable("🔗 It depends on"), "It depends on");
        assert_eq!(super::speakable("✓ Done! ❤️ \u{1F9D1}\u{200D}\u{1F3EB} Nice — “quote” 3:30"), "Done! Nice — “quote” 3:30");
        assert_eq!(super::speakable("이번이 처음이야 😆"), "이번이 처음이야");
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
    pub fn say_slow(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate * if self.speaker.neural_wanted() { 0.75 } else { 0.7 };
        self.speaker.say(text, r);
    }
    pub fn say_fast(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate * if self.speaker.neural_wanted() { 1.3 } else { 1.35 };
        self.speaker.say(text, r);
    }
    pub fn play_last_recording(&mut self) {
        self.speaker.stop();
        let rec = self.last_recording.clone();
        self.player.play_samples(rec, crate::audio::STT_RATE, false);
    }
}
