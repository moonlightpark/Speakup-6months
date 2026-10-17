//! Target-language packs. The whole app is driven by one of these, so another language can be
//! added later by adding a pack (plus built-in fallback content). Only English is enabled now.

pub struct LanguagePack {
    /// ISO 639-1 code, also used as the whisper language.
    pub code: &'static str,
    /// Name used inside Claude prompts.
    pub name: &'static str,
    pub name_ko: &'static str,
    /// Prefix matched against OS voice language tags (e.g. "en" matches en-US, en-GB).
    pub tts_lang_prefix: &'static str,
    /// Preferred OS voice language tag when several match.
    pub tts_preferred: &'static str,
    /// Whisper models offered for this language: (id, size label). English-only models are
    /// smaller and more accurate for English; other languages need the multilingual ones.
    pub whisper_models: &'static [(&'static str, &'static str)],
}

pub const ENGLISH: LanguagePack = LanguagePack {
    code: "en",
    name: "English",
    name_ko: "영어",
    tts_lang_prefix: "en",
    tts_preferred: "en-US",
    whisper_models: &[("tiny.en", "75MB · 빠름"), ("base.en", "142MB · 권장"), ("small.en", "466MB · 정확")],
};

pub const LANGUAGES: &[&LanguagePack] = &[&ENGLISH];

pub fn by_code(code: &str) -> &'static LanguagePack {
    LANGUAGES.iter().copied().find(|l| l.code == code).unwrap_or(&ENGLISH)
}
