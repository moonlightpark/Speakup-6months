//! Target-language packs. The whole app is driven by one of these. This build teaches
//! Japanese to Korean speakers.

pub struct LanguagePack {
    /// ISO 639-1 code, also used as the whisper language.
    pub code: &'static str,
    /// Name used inside Claude prompts.
    pub name: &'static str,
    pub name_ko: &'static str,
    /// Prefix matched against OS voice language tags (e.g. "ja" matches ja-JP).
    pub tts_lang_prefix: &'static str,
    /// Preferred OS voice language tag when several match.
    pub tts_preferred: &'static str,
    /// Whisper models offered for this language: (id, size label). Japanese needs the
    /// multilingual models (the ".en" ones are English only).
    pub whisper_models: &'static [(&'static str, &'static str)],
    /// YouGlish language path ("japanese").
    pub youglish: &'static str,
}

pub const JAPANESE: LanguagePack = LanguagePack {
    code: "ja",
    name: "Japanese",
    name_ko: "일본어",
    tts_lang_prefix: "ja",
    tts_preferred: "ja-JP",
    whisper_models: &[("base", "142MB · 빠름"), ("small", "466MB · 권장"), ("medium", "1.5GB · 정확")],
    youglish: "japanese",
};

pub const LANGUAGES: &[&LanguagePack] = &[&JAPANESE];

pub fn by_code(code: &str) -> &'static LanguagePack {
    LANGUAGES.iter().copied().find(|l| l.code == code).unwrap_or(&JAPANESE)
}
