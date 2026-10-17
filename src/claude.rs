//! Claude API client (Messages API over HTTPS; there is no official Rust SDK).
//! Every call uses structured outputs (`output_config.format` = JSON schema), so responses are
//! parsed straight into Rust structs. Claude provides all of the app's "intelligence":
//! lesson generation, speaking feedback, role-play partner, homework checking, level assessment.

use crate::language::LanguagePack;
use crate::models::*;
use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::time::Duration;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";

pub const MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5", "Claude Opus 5.5 (권장)"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5 (저렴)"),
    ("claude-haiku-5-5", "Claude Haiku 5.5 (가장 저렴·빠름)"),
];

#[derive(Clone)]
pub struct Claude {
    /// Messages endpoint (overridable so tests can point at a local mock server).
    pub url: String,
    pub api_key: String,
    pub model: String,
    pub lang: &'static LanguagePack,
}

/// A JSON-schema object whose properties are all required (structured outputs needs
/// `additionalProperties: false`).
fn obj(props: Value) -> Value {
    let required: Vec<String> = props.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}
fn s() -> Value {
    json!({"type": "string"})
}
fn arr(items: Value) -> Value {
    json!({"type": "array", "items": items})
}
fn phrase_chunk() -> Value {
    obj(json!({"en": s(), "ko": s()}))
}

const CHUNK_RULE: &str = "chunks = 직독직해 (direct reading) for Korean learners: split the sentence, in English word order, into the smallest \
     meaningful units — usually single words, but keep fixed expressions together (e.g. \"wake up\", \"a lot of\", \"in charge of\"). \
     Each chunk's en must be copied exactly from the sentence (keep punctuation attached to its word) so that joining all en values with spaces \
     reproduces the sentence exactly; ko is that chunk's Korean meaning as it would be read in English order (e.g. I / am / a / boy → 나 / 이다 / 한 / 소년).";

fn phrase_schema() -> Value {
    obj(json!({"text": s(), "ko": s()}))
}

impl Claude {
    fn system_prompt(&self) -> String {
        format!(
            "You are the AI engine of a voice-first {lang} speaking and listening app for Korean adults. \
             The learner's single goal is to understand and speak {lang} within a 6-month program. \
             Write learning content in natural, spoken, native {lang} — the way people actually talk, not textbook prose. \
             Write every explanation, translation and piece of feedback for the learner in Korean (field names ending in _ko), \
             short and encouraging, like a friendly tutor. \
             The learner's speech reaches you as an automatic speech-recognition transcript, so ignore missing punctuation and capitalization, \
             and remember that a wrong word in the transcript may be a pronunciation problem.",
            lang = self.lang.name
        )
    }

    fn call<T: DeserializeOwned>(&self, user: String, schema: Value, effort: &str) -> Result<T> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "fallbacks": "default",
            "output_config": {"effort": effort, "format": {"type": "json_schema", "schema": schema}},
            "system": self.system_prompt(),
            "messages": [{"role": "user", "content": user}],
        });
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(300)).build()?;
        let mut last_err = anyhow!("요청 실패");
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
            }
            let resp = match client
                .post(&self.url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "server-side-fallback-2026-07-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = anyhow!("네트워크 오류: {e}");
                    continue;
                }
            };
            let status = resp.status().as_u16();
            let text = resp.text().unwrap_or_default();
            match status {
                200 => return parse_response(&text),
                401 | 403 => bail!("Claude API 키가 올바르지 않습니다 (설정에서 확인하세요)."),
                400 | 404 => bail!("Claude API 요청 오류 ({status}): {}", api_error_message(&text)),
                429 | 500..=599 => {
                    last_err = anyhow!("Claude API가 일시적으로 바쁩니다 ({status}). 잠시 후 다시 시도하세요.");
                    continue;
                }
                _ => bail!("Claude API 오류 ({status}): {}", api_error_message(&text)),
            }
        }
        Err(last_err)
    }

    pub fn generate_lesson(&self, req: &LessonRequest) -> Result<Lesson> {
        let schema = obj(json!({
            "words": arr(obj(json!({"word": s(), "meaning_ko": s(), "example": s(), "example_ko": s()}))),
            "sentences": arr(obj(json!({"text": s(), "ko": s(), "tip_ko": s(), "chunks": arr(phrase_chunk())}))),
            "self_expression": obj(json!({
                "question": s(), "question_ko": s(), "useful_phrases": arr(phrase_schema()), "model_answer": s()
            })),
            "business": obj(json!({
                "title": s(), "scenario_ko": s(), "ai_role": s(), "user_role": s(), "opening_line": s(),
                "key_phrases": arr(phrase_schema())
            })),
            "listening": obj(json!({"search_queries": arr(s()), "focus_ko": s()})),
            "expressions": arr(obj(json!({"word": s(), "meaning_ko": s(), "example": s(), "example_ko": s()}))),
        }));
        let user = format!(
            "Create today's lesson.\n\
             Program day {day} of 182 (week {week}). Phase: {phase}.\n\
             Weekly theme: {theme}. Business focus: {business}. Background grammar (never teach it explicitly): {grammar}.\n\
             Learner level: {level:.1}/10 (CEFR {cefr}).\n\
             Words the learner already knows (do NOT repeat them): {known}\n\
             Items the learner keeps getting wrong (reuse some of them inside the new example sentences so they get extra practice): {weak}\n\
             Already studied on earlier days — every part of today's lesson must be NEW, so do not repeat or closely paraphrase any of these:\n\
             - sentences: {avoid_s}\n\
             - self-expression questions: {avoid_q}\n\
             - role-play scenarios: {avoid_r}\n\n\
             Style: everyday English exactly as native speakers really say it — contractions, phrasal verbs, fillers, \
             natural reactions and idioms. Do not write textbook or grammar-drill sentences; the goal is listening and speaking, not grammar.\n\
             Requirements:\n\
             - words: exactly {nw} new, high-frequency spoken words or short phrases for the theme, each with a short natural example sentence.\n\
             - sentences: exactly {ns} natural spoken sentences for reading aloud / shadowing, at or slightly above the learner's level; tip_ko is one pronunciation or linking tip. {chunk_rule}\n\
             - self_expression: one open question on the theme that makes the learner talk about their own life for 30-60 seconds; 4 useful_phrases; a model_answer of 3-5 sentences at the learner's level.\n\
             - business: a realistic role-play for the business focus; ai_role is who you will play; opening_line is your first line; 4 key_phrases the learner can use.\n\
             - expressions: exactly 2 everyday native expressions or slang the learner hasn't seen: one classic expression \
             (still common, may be from past decades, e.g. 'my bad', 'rain check') and one recent slang term (2010s-2020s social media, e.g. 'no cap', 'low-key'). \
             meaning_ko: natural Korean meaning plus when/where it's used (e.g. '(요즘 유행어) …', '(90년대) …'); example: a short real-life line using it.\n\
             - listening: 2-3 YouTube search queries that find real native-speaker videos (channels such as BBC Learning English, VOA Learning English, TED, Rachel's English) on this theme at a suitable level; focus_ko says what to listen for.",
            day = req.day,
            week = req.week,
            phase = req.phase,
            theme = req.theme,
            business = req.business,
            grammar = req.grammar,
            level = req.level,
            cefr = cefr_of(req.level),
            known = if req.known.is_empty() { "(none yet)".into() } else { req.known.join(", ") },
            weak = if req.weak.is_empty() { "(none)".into() } else { req.weak.join(" | ") },
            nw = req.n_words,
            ns = req.n_sentences,
            chunk_rule = CHUNK_RULE,
            avoid_s = list_or_none(&req.avoid_sentences),
            avoid_q = list_or_none(&req.avoid_questions),
            avoid_r = list_or_none(&req.avoid_scenarios),
        );
        let mut lesson: Lesson = self.call(user, schema, "medium")?;
        lesson.date = Some(today());
        lesson.source = "claude".into();
        Ok(lesson)
    }

    pub fn speech_feedback(&self, question: &str, transcript: &str, level: f32) -> Result<SpeechFeedback> {
        let schema = obj(json!({
            "fluency": json!({"type": "integer"}), "grammar": json!({"type": "integer"}),
            "vocabulary": json!({"type": "integer"}), "overall": json!({"type": "integer"}),
            "feedback_ko": s(),
            "corrections": arr(obj(json!({"original": s(), "corrected": s(), "explanation_ko": s()}))),
            "natural_version": s(),
            "new_expressions": arr(phrase_schema()),
        }));
        let user = format!(
            "The learner (level {level:.1}/10, CEFR {cefr}) answered this question out loud.\n\
             Question: {question}\nTranscript of their answer: \"{transcript}\"\n\n\
             Score fluency, grammar, vocabulary and overall from 0 to 100 for their level. \
             List the most important corrections (max 5). natural_version: rewrite their answer as a native speaker would say it, \
             keeping their ideas, at most one level above theirs — they will read it aloud several times. \
             new_expressions: 2-4 useful expressions from the natural version worth memorizing. \
             feedback_ko: 2-3 sentences of encouraging, concrete feedback. If the transcript is empty or unrelated, say so kindly and give 0 scores.",
            cefr = cefr_of(level)
        );
        self.call(user, schema, "low")
    }

    pub fn roleplay_turn(&self, sc: &BusinessScenario, history: &[(bool, String)], level: f32) -> Result<RoleplayTurn> {
        let schema = obj(json!({
            "reply": s(), "reply_ko": s(),
            "correction": s(), "correction_ko": s(),
            "suggestions": arr(phrase_schema()),
            "finished": json!({"type": "boolean"}),
            "summary_ko": s(),
        }));
        let convo: String = history
            .iter()
            .map(|(is_user, t)| format!("{}: {t}\n", if *is_user { "LEARNER" } else { "YOU" }))
            .collect();
        let convo = if convo.is_empty() { "(nothing yet — you speak first: open the scene naturally)\n".to_string() } else { convo };
        let turns = history.iter().filter(|(u, _)| *u).count();
        let user = format!(
            "Role-play. Scenario: {title} — {scenario}. You play {ai}. The learner plays {usr}. Learner level {level:.1}/10 (CEFR {cefr}).\n\
             Conversation so far:\n{convo}\n\
             Continue as {ai}: reply in 1-3 short spoken sentences matched to the learner's level, and keep the conversation moving with a question. \
             reply_ko: Korean translation of your reply.\n\
             correction: if the learner's LAST line had a mistake or sounded unnatural, the natural way to say it (else empty string); correction_ko: brief Korean explanation (else empty).\n\
             suggestions: 2 things the learner could say next (text + ko).\n\
             The learner has spoken {turns} times. Set finished=true and wrap up politely once the goal of the scenario is reached or after about 8 learner turns; \
             when finished, summary_ko gives 2-3 sentences on how they did and what to practice. Otherwise summary_ko is empty.",
            title = sc.title,
            scenario = sc.scenario_ko,
            ai = sc.ai_role,
            usr = sc.user_role,
            cefr = cefr_of(level),
        );
        self.call(user, schema, "low")
    }

    pub fn speed_passage(&self, theme: &str, level: f32, known: &[String], avoid_titles: &[String]) -> Result<SpeedPassage> {
        let schema = obj(json!({"title": s(), "passage": s(), "passage_ko": s(), "chunks": arr(phrase_chunk())}));
        let words = (40.0 + level * 10.0).round() as u32;
        let user = format!(
            "Write a short passage for SPEED READING ALOUD practice (repeated reading for fluency).\n\
             Theme: {theme}. Learner level {level:.1}/10 (CEFR {cefr}). Length: about {words} words.\n\
             It must sound like natural spoken {lang} (a short story, a monologue or a mini talk), use mostly words the learner knows, \
             with short-to-medium sentences that are fun to say quickly. Where natural, reuse some of these learned words: {known}.\n\
             Pick a fresh topic: the learner already read passages titled {avoid}.\n\
             passage_ko: a natural Korean translation. {chunk_rule} (Here the whole passage is the sentence.)",
            cefr = cefr_of(level),
            lang = self.lang.name,
            known = known.join(", "),
            chunk_rule = CHUNK_RULE,
            avoid = list_or_none(avoid_titles),
        );
        self.call(user, schema, "low")
    }

    /// 직독직해 for sentences that didn't come with one (review items, YouTube lines, ...).
    pub fn chunk_gloss(&self, text: &str) -> Result<Vec<Chunk>> {
        #[derive(Deserialize)]
        struct G {
            chunks: Vec<Chunk>,
        }
        let schema = obj(json!({"chunks": arr(phrase_chunk())}));
        let user = format!("Sentence: \"{text}\"\n\n{CHUNK_RULE}");
        let g: G = self.call(user, schema, "low")?;
        Ok(g.chunks)
    }

    pub fn pronunciation_coach(&self, target: &str, transcript: &str, missed: &[String]) -> Result<CoachTip> {
        let schema = obj(json!({"tip_ko": s(), "focus_words": arr(s())}));
        let user = format!(
            "The learner tried to read aloud: \"{target}\"\nSpeech recognition heard: \"{transcript}\"\nWords not recognized: {missed:?}\n\n\
             Diagnose the likely pronunciation problems (sounds Korean speakers often struggle with: r/l, f/p, v/b, th, z, final consonants, word stress, linking). \
             tip_ko: 2-4 short, practical sentences on how to fix it (mouth position, stress, linking). focus_words: the words to practice."
        );
        self.call(user, schema, "low")
    }

    pub fn check_word_use(&self, word: &str, meaning_ko: &str, transcript: &str) -> Result<WordUseCheck> {
        let schema = obj(json!({"ok": json!({"type": "boolean"}), "feedback_ko": s(), "better": s()}));
        let user = format!(
            "Homework: the learner had to say an original sentence using \"{word}\" ({meaning_ko}).\nThey said: \"{transcript}\"\n\n\
             ok=true if they used the word (or a form of it) correctly in a meaningful sentence; small grammar slips are fine. \
             feedback_ko: one or two short sentences. better: a natural version of their sentence."
        );
        self.call(user, schema, "low")
    }

    /// Corrects a short piece of writing (diary / answer to a prompt).
    pub fn writing_feedback(&self, prompt: &str, text: &str, level: f32) -> Result<WritingFeedback> {
        let schema = obj(json!({
            "score": json!({"type": "integer"}),
            "feedback_ko": s(),
            "corrections": arr(obj(json!({"original": s(), "corrected": s(), "explanation_ko": s()}))),
            "natural_version": s(),
            "natural_version_ko": s(),
        }));
        let user = format!(
            "The learner (level {level:.1}/10, CEFR {cefr}) wrote this in English.\nPrompt: {prompt}\nTheir text: \"{text}\"\n\n\
             score 0-100 for their level (meaning, naturalness, accuracy). corrections: the most important fixes (max 6), each original → corrected \
             with a short Korean explanation. natural_version: how a native speaker would write the same ideas in everyday, spoken-style English \
             (they will read it aloud afterwards). natural_version_ko: its Korean translation. feedback_ko: 2 encouraging, concrete sentences.",
            cefr = cefr_of(level)
        );
        self.call(user, schema, "low")
    }

    /// Checks a Korean→English translation; alternatives with the same meaning are fine.
    pub fn check_translation(&self, korean: &str, reference: &str, answer: &str) -> Result<TranslationCheck> {
        let schema = obj(json!({"ok": json!({"type": "boolean"}), "feedback_ko": s(), "better": s()}));
        let user = format!(
            "Korean: \"{korean}\"\nReference English: \"{reference}\"\nLearner wrote: \"{answer}\"\n\n\
             ok=true if the learner's sentence says the same thing in natural English (it doesn't have to match the reference word for word; \
             minor spelling slips are fine). feedback_ko: one short sentence. better: the most natural way to say it (may be the reference)."
        );
        self.call(user, schema, "low")
    }

    /// Rewrites a page of a book in easier English for the learner's level.
    pub fn simplify_page(&self, paragraphs: &[String], level: f32) -> Result<SimplifiedPage> {
        let schema = obj(json!({"paragraphs": arr(s()), "summary_ko": s()}));
        let user = format!(
            "Rewrite this page of a story for an English learner at level {level:.1}/10 (CEFR {cefr}). Keep the story, the characters and \
             the order of events, one output paragraph per input paragraph, but use simpler words and shorter sentences that sound natural \
             when read aloud. summary_ko: a 2-3 sentence Korean summary of the page.\n\n{text}",
            cefr = cefr_of(level),
            text = paragraphs.join("\n\n")
        );
        self.call(user, schema, "low")
    }

    /// Korean → English at three levels (easy / intermediate / advanced) for the translator.
    pub fn translate_levels(&self, korean: &str) -> Result<Translation> {
        let version = obj(json!({"level_ko": s(), "english": s(), "note_ko": s()}));
        let schema = obj(json!({"versions": arr(version), "meaning_ko": s()}));
        let user = format!(
            "A Korean learner wants to say this in English: \"{korean}\"\n\n\
             Give exactly 3 versions in natural, spoken English as natives really say it, in this order: \
             level_ko \"쉽게\" (short, simple words a beginner can say right away), \"중간\" (everyday native style with common phrasal verbs/idioms), \
             \"고급\" (fluent, nuanced, polished — still natural speech, not stiff). \
             note_ko: one short Korean tip about the key expression or nuance of that version. \
             meaning_ko: how you understood the Korean (one short line), in case it was ambiguous or the speech recognition was off."
        );
        self.call(user, schema, "low")
    }

    /// Meaning of a word as used in a sentence.
    pub fn word_in_context(&self, word: &str, sentence: &str) -> Result<WordMeaning> {
        let schema = obj(json!({"meaning_ko": s(), "note_ko": s(), "example": s(), "example_ko": s()}));
        let user = format!(
            "Word: \"{word}\"\nSentence: \"{sentence}\"\n\nmeaning_ko: its Korean meaning in this sentence (short). note_ko: one short note \
             (base form, nuance, or if it's old-fashioned). example: a different, everyday example sentence. example_ko: its translation."
        );
        self.call(user, schema, "low")
    }

    pub fn assess_level(&self, readings: &[(String, String, f32)], question: &str, answer: &str, current: f32) -> Result<LevelAssessment> {
        let schema = obj(json!({
            "level": json!({"type": "number"}), "summary_ko": s(),
            "strengths_ko": arr(s()), "weaknesses_ko": arr(s()), "focus_next_ko": s(),
        }));
        let reads: String = readings
            .iter()
            .map(|(t, h, sc)| format!("- target: \"{t}\" | heard: \"{h}\" | match {sc:.0}%\n"))
            .collect();
        let user = format!(
            "Assess the learner's {lang} speaking level.\nCurrent recorded level: {current:.1}/10.\n\
             Reading-aloud results (sentences go from A1 to C1 difficulty):\n{reads}\n\
             Free-speech question: {question}\nTheir answer (transcript): \"{answer}\"\n\n\
             level: a number from 1.0 to 10.0 on this scale: 1-2 A1, 2-3.5 A2, 3.5-5 B1, 5-6.5 B1+, 6.5-8 B2, 8-9 B2+, 9-10 C1. \
             Weigh free speech more than reading. summary_ko: 2-3 sentences. 2-3 strengths and weaknesses. focus_next_ko: what to focus on next month.",
            lang = self.lang.name
        );
        let mut a: LevelAssessment = self.call(user, schema, "medium")?;
        a.level = a.level.clamp(1.0, 10.0);
        Ok(a)
    }
}

pub struct LessonRequest {
    pub day: i64,
    pub week: u32,
    pub phase: String,
    pub theme: String,
    pub business: String,
    pub grammar: String,
    pub level: f32,
    pub known: Vec<String>,
    pub weak: Vec<String>,
    pub n_words: u32,
    pub n_sentences: u32,
    /// Content from earlier lessons that must not be repeated.
    pub avoid_sentences: Vec<String>,
    pub avoid_questions: Vec<String>,
    pub avoid_scenarios: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Correction {
    pub original: String,
    pub corrected: String,
    pub explanation_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SpeechFeedback {
    pub fluency: i32,
    pub grammar: i32,
    pub vocabulary: i32,
    pub overall: i32,
    pub feedback_ko: String,
    pub corrections: Vec<Correction>,
    pub natural_version: String,
    pub new_expressions: Vec<Phrase>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct RoleplayTurn {
    pub reply: String,
    pub reply_ko: String,
    pub correction: String,
    pub correction_ko: String,
    pub suggestions: Vec<Phrase>,
    pub finished: bool,
    pub summary_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SpeedPassage {
    pub title: String,
    pub passage: String,
    pub passage_ko: String,
    #[serde(default)]
    pub chunks: Vec<Chunk>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct CoachTip {
    pub tip_ko: String,
    pub focus_words: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct WordUseCheck {
    pub ok: bool,
    pub feedback_ko: String,
    pub better: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct LevelAssessment {
    pub level: f32,
    pub summary_ko: String,
    pub strengths_ko: Vec<String>,
    pub weaknesses_ko: Vec<String>,
    pub focus_next_ko: String,
}

fn list_or_none(v: &[String]) -> String {
    if v.is_empty() { "(none)".into() } else { v.join(" | ") }
}

fn api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

fn parse_response<T: DeserializeOwned>(body: &str) -> Result<T> {
    let v: Value = serde_json::from_str(body)?;
    match v["stop_reason"].as_str() {
        Some("refusal") => bail!("Claude가 이 요청을 처리하지 않았습니다. 다른 내용으로 다시 시도해 주세요."),
        Some("max_tokens") => bail!("응답이 너무 길어 잘렸습니다. 다시 시도해 주세요."),
        _ => {}
    }
    // Thinking blocks may precede the answer; the structured JSON is in the text block(s).
    let text: String = v["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect()
        })
        .unwrap_or_default();
    serde_json::from_str(&text).map_err(|e| anyhow!("Claude 응답을 해석하지 못했습니다: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_block_after_thinking() {
        let body = r#"{"stop_reason":"end_turn","content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"tip_ko\":\"혀를 이 사이에\",\"focus_words\":[\"think\"]}"}]}"#;
        let t: CoachTip = parse_response(body).unwrap();
        assert_eq!(t.focus_words, vec!["think"]);
    }

    #[test]
    fn refusal_is_an_error() {
        let body = r#"{"stop_reason":"refusal","content":[]}"#;
        assert!(parse_response::<CoachTip>(body).is_err());
    }

    /// Hits the real API with a bad key: checks connectivity, headers and error mapping.
    #[test]
    #[ignore]
    fn bad_key_gives_korean_auth_error() {
        let c = Claude { url: API_URL.into(), api_key: "sk-ant-invalid".into(), model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH };
        let err = c.pronunciation_coach("think", "sink", &["think".into()]).unwrap_err().to_string();
        println!("{err}");
        assert!(err.contains("API 키"), "{err}");
    }

    /// Every Claude feature against the real API (needs ANTHROPIC_API_KEY):
    /// cargo test real_api -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_api_all_features() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH };
        let t = std::time::Instant::now();
        let req = LessonRequest {
            day: 3, week: 1, phase: "1단계".into(), theme: "Introducing yourself".into(),
            business: "Introducing yourself at work".into(), grammar: "be verbs".into(), level: 3.0,
            known: vec!["introduce".into()], weak: vec!["Nice to meet you.".into()], n_words: 4, n_sentences: 3,
            avoid_sentences: vec![], avoid_questions: vec!["Tell me about your typical day.".into()], avoid_scenarios: vec![],
        };
        let l = c.generate_lesson(&req).expect("lesson");
        println!("lesson ({:?}): words={:?}", t.elapsed(), l.words.iter().map(|w| &w.word).collect::<Vec<_>>());
        assert_eq!(l.words.len(), 4);
        assert_eq!(l.sentences.len(), 3);
        for s in &l.sentences {
            let ok = crate::storage::chunks_match(&s.text, &s.chunks);
            println!("  {} | {}", s.text, s.chunks.iter().map(|c| format!("{}={}", c.en, c.ko)).collect::<Vec<_>>().join(" / "));
            assert!(ok, "lesson chunks must spell the sentence: {}", s.text);
        }
        let g = c.chunk_gloss("I have been working here for three years.").expect("gloss");
        println!("gloss: {}", g.iter().map(|c| format!("{}={}", c.en, c.ko)).collect::<Vec<_>>().join(" / "));
        assert!(crate::storage::chunks_match("I have been working here for three years.", &g));
        let f = c.speech_feedback("What do you do?", "i am work in company as designer since two year", 3.0).expect("feedback");
        println!("feedback: overall={} natural={:?} corrections={}", f.overall, f.natural_version, f.corrections.len());
        assert!(!f.corrections.is_empty() && !f.natural_version.is_empty());
        let sc = l.business.clone();
        let turn = c.roleplay_turn(&sc, &[(false, sc.opening_line.clone()), (true, "yesterday i go to meeting with my boss".into())], 3.0).expect("roleplay");
        println!("roleplay: {:?} / correction {:?}", turn.reply, turn.correction);
        assert!(!turn.reply.is_empty());
        let open = c.roleplay_turn(&BusinessScenario { title: "Hotel".into(), scenario_ko: "호텔 예약 누락 항의".into(), ai_role: "hotel manager".into(), user_role: "guest".into(), ..Default::default() }, &[], 3.0).expect("open");
        println!("custom scene opening: {:?}", open.reply);
        let sp = c.speed_passage("Daily routines", 3.0, &["usually".into()], &["My Morning".into()]).expect("passage");
        println!("speed passage: {} words, chunks ok={}", sp.passage.split_whitespace().count(), crate::storage::chunks_match(&sp.passage, &sp.chunks));
        let w = c.check_word_use("deadline", "마감일", "the deadline of project is next monday").expect("word");
        println!("word use: ok={} {}", w.ok, w.feedback_ko);
        assert!(w.ok);
        let tip = c.pronunciation_coach("I think so.", "I sink so", &["think".into()]).expect("coach");
        println!("coach: {} {:?}", tip.tip_ko, tip.focus_words);
        let a = c.assess_level(&[("Hello, my name is Alex.".into(), "hello my name is alex".into(), 100.0)], "Introduce yourself.", "i am engineer. i like english but speaking is hard", 2.0).expect("assess");
        println!("assess: level {:.1} {}", a.level, a.summary_ko);
        println!("total {:?}", t.elapsed());
    }

    #[test]
    fn schema_requires_all_properties() {
        let v = obj(json!({"a": s(), "b": s()}));
        assert_eq!(v["required"].as_array().unwrap().len(), 2);
        assert_eq!(v["additionalProperties"], false);
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct WritingFeedback {
    pub score: i32,
    pub feedback_ko: String,
    pub corrections: Vec<Correction>,
    pub natural_version: String,
    pub natural_version_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct TranslationCheck {
    pub ok: bool,
    pub feedback_ko: String,
    pub better: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SimplifiedPage {
    pub paragraphs: Vec<String>,
    pub summary_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct WordMeaning {
    pub meaning_ko: String,
    pub note_ko: String,
    pub example: String,
    pub example_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct TranslationVersion {
    pub level_ko: String,
    pub english: String,
    pub note_ko: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Translation {
    pub versions: Vec<TranslationVersion>,
    pub meaning_ko: String,
}
