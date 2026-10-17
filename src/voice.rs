//! Text-to-speech through the operating system's voices (macOS AVFoundation, Windows WinRT,
//! Linux speech-dispatcher), plus the shared voice I/O bundle used by every screen.

use crate::audio::{Player, Recorder};
use crate::language::LanguagePack;
use crate::stt::Stt;
use tts::Tts;

pub struct Speaker {
    tts: Option<Tts>,
    pub voices: Vec<(String, String)>, // (id, label)
    pub error: Option<String>,
}

impl Speaker {
    pub fn new(lang: &LanguagePack, preferred_id: &str) -> Self {
        let mut tts = match Tts::default() {
            Ok(t) => t,
            Err(e) => {
                return Self { tts: None, voices: Vec::new(), error: Some(format!("음성 출력(TTS)을 사용할 수 없습니다: {e}")) };
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
        Self { tts: Some(tts), voices, error: None }
    }

    /// No speech output (automated tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        Self { tts: None, voices: Vec::new(), error: None }
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
    }
}

/// Everything audio-related, owned by the app and lent to screens.
pub struct Voice {
    pub speaker: Speaker,
    pub recorder: Recorder,
    pub player: Player,
    pub stt: Stt,
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
        let r = self.base_rate * 0.7;
        self.speaker.say(text, r);
    }
    pub fn say_fast(&mut self, text: &str) {
        self.player.stop();
        let r = self.base_rate * 1.35;
        self.speaker.say(text, r);
    }
    pub fn play_last_recording(&mut self) {
        self.speaker.stop();
        let rec = self.last_recording.clone();
        self.player.play_samples(rec, crate::audio::STT_RATE, false);
    }
}
