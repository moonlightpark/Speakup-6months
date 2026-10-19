//! Claude API client (Messages API over HTTPS; there is no official Rust SDK).
//! Every call uses structured outputs (`output_config.format` = JSON schema), so responses are
//! parsed straight into Rust structs. Claude provides all of the app's "intelligence":
//! lesson generation, speaking feedback, role-play partner, homework checking, level assessment.

use crate::language::LanguagePack;
use crate::models::*;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Set when the API can't be used at all (no credit, bad key, no network) and cleared by
    /// the next answer; shared with the app so the speaking screens switch to offline scripts.
    pub down: Arc<AtomicBool>,
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
fn role() -> Value {
    json!({"type": "string", "enum": crate::grammar::role_ids()})
}
fn phrase_chunk() -> Value {
    obj(json!({"en": s(), "ko": s(), "role": role()}))
}

/// The 직독직해 rule plus the grammatical role of each chunk (for colouring).
fn chunk_rule() -> String {
    format!("{CHUNK_RULE} For each chunk, {}", crate::grammar::ROLE_RULE)
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
        let mut network_failed = false;
        let t0 = std::time::Instant::now();
        let what: String = user.chars().take(60).collect::<String>().replace('\n', " ");
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
                    crate::applog!("claude: 네트워크 오류 · {e}");
                    last_err = anyhow!("네트워크 오류: {e}");
                    network_failed = true;
                    continue;
                }
            };
            let status = resp.status().as_u16();
            let text = resp.text().unwrap_or_default();
            crate::applog!("claude: {status} · {:.1}초 · {}B · \"{what}…\"", t0.elapsed().as_secs_f32(), text.len());
            network_failed = false;
            match status {
                200 => {
                    self.down.store(false, Ordering::Relaxed);
                    return parse_response(&text);
                }
                401 | 403 => {
                    self.down.store(true, Ordering::Relaxed);
                    bail!("Claude API 키가 올바르지 않습니다 (설정에서 확인하세요).")
                }
                400 | 404 => {
                    let msg = api_error_message(&text);
                    if msg.contains("credit balance") {
                        self.down.store(true, Ordering::Relaxed);
                    }
                    bail!("Claude API 요청 오류 ({status}): {msg}")
                }
                429 | 500..=599 => {
                    last_err = anyhow!("Claude API가 일시적으로 바쁩니다 ({status}). 잠시 후 다시 시도하세요.");
                    continue;
                }
                _ => bail!("Claude API 오류 ({status}): {}", api_error_message(&text)),
            }
        }
        if network_failed {
            self.down.store(true, Ordering::Relaxed);
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
            "phrases": arr(obj(json!({"word": s(), "meaning_ko": s(), "example": s(), "example_ko": s()}))),
        }));
        // Idioms/slang stay at ~5% of new material: one a day, classic and recent in turn.
        let expression_kind = if req.day % 2 == 1 {
            "one recent slang term (2010s-2020s social media, e.g. 'no cap', 'low-key')"
        } else {
            "one classic everyday expression (still common, may be from past decades, e.g. 'my bad', 'rain check')"
        };
        // The ~2,000 most frequent words cover 80-90% of everyday conversation; go beyond them after B1.
        let frequency_band = if req.level < 5.0 {
            "from the 2,000 most frequent words of everyday spoken English"
        } else {
            "from the next frequency band (roughly the 2,000th-5,000th most frequent words of spoken English)"
        };
        let user = format!(
            "Create today's lesson.\n\
             Program day {day} of 182 (week {week}). Phase: {phase}.\n\
             Weekly theme: {theme}. Business focus: {business}. Background grammar (never teach it explicitly): {grammar}.\n\
             Learner level: {level:.1}/10 (CEFR {cefr}).\n\
             Words the learner already knows (do NOT repeat them): {known}\n\
             Chunks the learner already knows (do NOT repeat them): {known_phrases}\n\
             Items the learner keeps getting wrong (reuse some of them inside the new example sentences so they get extra practice): {weak}\n\
             Already studied on earlier days — every part of today's lesson must be NEW, so do not repeat or closely paraphrase any of these:\n\
             - sentences: {avoid_s}\n\
             - self-expression questions: {avoid_q}\n\
             - role-play scenarios: {avoid_r}\n\n\
             Style: everyday English exactly as native speakers really say it — contractions, phrasal verbs, fillers, \
             natural reactions and idioms. Do not write textbook or grammar-drill sentences; the goal is listening and speaking, not grammar.\n\
             Requirements:\n\
             - phrases: exactly {np} high-frequency spoken chunks / sentence frames that natives use all the time and the learner can reuse in many \
             situations (e.g. 'I was wondering if', 'It depends on', 'I'm thinking about', 'The thing is'), fitting the theme where possible. \
             Write the chunk itself without '...' (the example shows how it continues); meaning_ko: natural Korean meaning and when to use it; \
             example: a short everyday sentence that contains the chunk exactly.\n\
             - words: exactly {nw} new high-frequency words {frequency_band}, for the theme, each with a short natural example sentence. \
             Write them in lower case, and never use names of places, people or brands as words.\n\
             - sentences: exactly {ns} easy-to-read, longer natural sentences (about 12-20 words, at or slightly below the learner's level) for reading aloud / shadowing right after the day's short idiom sentences; tip_ko is one pronunciation or linking tip. {chunk_rule}\n\
             - self_expression: one open question on the theme that makes the learner talk about their own life for 30-60 seconds; 4 useful_phrases; a model_answer of 3-5 sentences at the learner's level.\n\
             - business: a realistic role-play for the business focus; ai_role is who you will play; opening_line is your first line; 4 key_phrases the learner can use.\n\
             - expressions: exactly 1 item the learner hasn't seen: {expression_kind}. \
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
            known_phrases = list_or_none(&req.known_phrases),
            np = crate::content::CHUNKS_PER_DAY,
            weak = if req.weak.is_empty() { "(none)".into() } else { req.weak.join(" | ") },
            nw = req.n_words,
            ns = req.n_sentences,
            chunk_rule = chunk_rule(),
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
             The learner has spoken {turns} times. {plan}Set finished=true and wrap up politely once the goal of the scenario is reached or after about 8 learner turns; \
             when finished, summary_ko gives 2-3 sentences on how they did and what to practice. Otherwise summary_ko is empty.",
            plan = if sc.guide.is_empty() {
                String::new()
            } else {
                format!(
                    "Follow this plan: ask these questions one at a time in this order (react briefly and naturally to each answer first, rephrase freely): {}. After the learner answers the last one, close the conversation warmly and set finished=true. ",
                    sc.guide.iter().enumerate().map(|(i, q)| format!("{}) {q}", i + 1)).collect::<Vec<_>>().join(" ")
                )
            },
            title = sc.title,
            scenario = sc.scenario_ko,
            ai = sc.ai_role,
            usr = sc.user_role,
            cefr = cefr_of(level),
        );
        self.call(user, schema, "low")
    }

    /// The day's 2-minute read-aloud script for 스피드 리딩, on the week's theme and grammar.
    pub fn speed_passage(&self, day: u32, theme: &str, grammar: &str, level: f32, words: u32, known: &[String], avoid_titles: &[String]) -> Result<SpeedPassage> {
        let schema = obj(json!({"title": s(), "passage": s(), "passage_ko": s(), "chunks": arr(phrase_chunk())}));
        let user = format!(
            "Write a script for SPEED READING ALOUD practice (read along, then read the whole of it in 2 minutes) — day {day} of the course.\n\
             Theme: {theme}. Practise this grammar naturally and often: {grammar}. Learner level {level:.1}/10 (CEFR {cefr}).\n\
             Length: {words} words (±10) — about 2 minutes read aloud at the learner's target pace.\n\
             It must sound like natural spoken {lang} (a first-person story, monologue or mini talk), use mostly words the learner knows, \
             with short-to-medium sentences (each ending with . ? or !) that are fun to say quickly. Where natural, reuse some of these learned words: {known}.\n\
             Pick a fresh topic: the learner already read passages titled {avoid}.\n\
             passage_ko: a natural Korean translation. {chunk_rule} (Here the whole passage is the sentence.)",
            cefr = cefr_of(level),
            lang = self.lang.name,
            known = known.join(", "),
            chunk_rule = chunk_rule(),
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
        let user = format!("Sentence: \"{text}\"\n\n{}", chunk_rule());
        let g: G = self.call(user, schema, "low")?;
        Ok(g.chunks)
    }

    /// Grammatical roles in Korean passages (translations, prompts) for colouring; one list of
    /// segments per text, in order.
    pub fn ko_roles(&self, texts: &[String]) -> Result<Vec<Vec<crate::grammar::KoSeg>>> {
        #[derive(Deserialize)]
        struct Item {
            segments: Vec<crate::grammar::KoSeg>,
        }
        #[derive(Deserialize)]
        struct R {
            items: Vec<Item>,
        }
        let schema = obj(json!({"items": arr(obj(json!({"segments": arr(obj(json!({"text": s(), "role": role()})))})))}));
        let list = texts.iter().enumerate().map(|(i, t)| format!("{}. {t}", i + 1)).collect::<Vec<_>>().join("\n");
        let user = format!(
            "Korean grammar roles. For each numbered Korean text below, return one item (same order) whose segments split the text \
             into its words/phrases so that concatenating every segment's text reproduces the text exactly — spaces and punctuation \
             included (put each space in the segment before it). Keep a particle or ending with its word (나는, 소년이다). \
             Each segment's {}\n\n{list}",
            crate::grammar::ROLE_RULE
        );
        let r: R = self.call(user, schema, "low")?;
        Ok(r.items.into_iter().map(|i| i.segments).collect())
    }

    /// Spoken questions for "listen and respond": heard only (no text), answered out loud.
    pub fn listen_questions(&self, level: f32, theme: &str, avoid: &[String]) -> Result<Vec<ListenQuestion>> {
        #[derive(Deserialize)]
        struct R {
            questions: Vec<ListenQuestion>,
        }
        let schema = obj(json!({"questions": arr(obj(json!({"question": s(), "question_ko": s(), "sample_answer": s()})))}));
        let user = format!(
            "Listen-and-respond practice. Write exactly 6 short questions or remarks a native speaker would say to the learner \
             in real conversation (theme: {theme}), at the learner's listening level {level:.1}/10 (CEFR {cefr}) — natural speed \
             and wording, contractions included. The learner only HEARS each one and must answer out loud. Mix question types \
             (yes/no, wh-, opinion, follow-up). question_ko: Korean translation (shown only after answering); \
             sample_answer: a natural 1-2 sentence answer at the learner's level. Do not repeat: {avoid}",
            cefr = cefr_of(level),
            avoid = list_or_none(avoid),
        );
        let r: R = self.call(user, schema, "low")?;
        Ok(r.questions)
    }

    /// Did the learner understand the heard question, and was the spoken answer a fitting reply?
    pub fn judge_answer(&self, question: &str, transcript: &str, level: f32) -> Result<AnswerJudgement> {
        let schema = obj(json!({
            "understood": {"type": "boolean"}, "score": {"type": "integer"},
            "feedback_ko": s(), "better_answer": s(),
        }));
        let user = format!(
            "Listen-and-respond check. The learner (level {level:.1}/10) heard this question without seeing it: \"{question}\"\n\
             Their spoken answer (speech recognition transcript): \"{transcript}\"\n\n\
             understood: did the answer show they understood the question? score 0-100: how well it works as a reply \
             (understanding and fit first, then naturalness; small grammar slips matter little). feedback_ko: 1-2 encouraging \
             sentences in Korean — if they misunderstood, say what the question actually asked. better_answer: a natural reply \
             a native speaker might give, keeping the learner's idea if it fits, at most one level above theirs."
        );
        self.call(user, schema, "low")
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

    /// 시제 3문장 for new words: one idea per word in three tenses with fixed frames.
    pub fn tense_sentences(&self, words: &[(String, String)], level: f32) -> Result<Vec<crate::tenses::TenseSet>> {
        let schema = obj(json!({"items": arr(obj(json!({
            "word": s(), "present": s(), "present_ko": s(), "past": s(), "past_ko": s(), "future": s(), "future_ko": s(),
        })))}));
        let list: String = words.iter().map(|(w, ko)| format!("- {w} ({ko})\n")).collect();
        let user = format!(
            "Tense frames: for each English word or phrase below write three short, natural spoken sentences that use it, all with the same idea, \
             for a learner at level {level:.1}/10 (CEFR {cefr}), 5-10 words each:\n\
             present = \"I'm thinking of …ing …\" (Korean: ~할까 생각 중이에요), past = \"I was thinking of …ing …\" (Korean: ~할까 생각했어요), \
             future = \"I'll think about …ing …\" (Korean: ~할까 생각할 거예요). The word or phrase itself must appear in each sentence (any form; a phrase that is a question or a frame can be quoted or embedded, \
             e.g. \"I'm thinking of asking, 'Do you mind if I sit here?'\"). word: exactly as given. \
             *_ko: natural Korean translations ending with those Korean frames.\n\nWords and phrases:\n{list}",
            cefr = cefr_of(level)
        );
        #[derive(Deserialize)]
        struct Out {
            items: Vec<crate::tenses::TenseSet>,
        }
        let out: Out = self.call(user, schema, "low")?;
        Ok(out.items)
    }

    /// 오늘의 문장 만들기: an original sentence with one of the day's words, on the week's theme,
    /// practising the week's grammar.
    pub fn check_sentence_mission(&self, word: &str, meaning_ko: &str, theme: &str, grammar: &str, transcript: &str, level: f32) -> Result<WordUseCheck> {
        let schema = obj(json!({"ok": json!({"type": "boolean"}), "feedback_ko": s(), "better": s()}));
        let user = format!(
            "Sentence mission: the learner (level {level:.1}/10, CEFR {cefr}) had to make an original spoken sentence using \"{word}\" ({meaning_ko}), \
             about this week's theme \"{theme}\", ideally practising this week's grammar ({grammar}).\nThey said: \"{transcript}\"\n\n\
             ok=true if they used the word (or a form of it) correctly in a meaningful sentence; small grammar slips are fine and the theme is a suggestion. \
             feedback_ko: one or two short, encouraging sentences in Korean; mention the week's grammar if they used it or could have. \
             better: a natural version of their sentence (keep their idea), using the week's grammar where it fits.",
            cefr = cefr_of(level)
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
        let schema = obj(json!({"meaning_ko": s(), "note_ko": s(), "example": s(), "example_ko": s(), "proper_noun": json!({"type": "boolean"})}));
        let user = format!(
            "Word: \"{word}\"\nSentence: \"{sentence}\"\n\nmeaning_ko: its Korean meaning in this sentence (short). note_ko: one short note \
             (base form, nuance, or if it's old-fashioned). example: a different, everyday example sentence. example_ko: its translation. \
             proper_noun: true if it is a name here (a place, person, brand or organization), not an ordinary word."
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
    /// Chunks already saved (not to be repeated).
    pub known_phrases: Vec<String>,
    pub weak: Vec<String>,
    pub n_words: u32,
    pub n_sentences: u32,
    /// Content from earlier lessons that must not be repeated.
    pub avoid_sentences: Vec<String>,
    pub avoid_questions: Vec<String>,
    pub avoid_scenarios: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ListenQuestion {
    pub question: String,
    pub question_ko: String,
    pub sample_answer: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct AnswerJudgement {
    pub understood: bool,
    pub score: i32,
    pub feedback_ko: String,
    pub better_answer: String,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
        let c = Claude { url: API_URL.into(), api_key: "sk-ant-invalid".into(), model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let err = c.pronunciation_coach("think", "sink", &["think".into()]).unwrap_err().to_string();
        println!("{err}");
        assert!(err.contains("API 키"), "{err}");
    }

    /// cargo test real_api_proper_noun -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_api_proper_noun() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let name = c.word_in_context("Daegu", "I grew up in Daegu, but now I live in Seoul.").unwrap();
        let word = c.word_in_context("grew", "I grew up in Daegu, but now I live in Seoul.").unwrap();
        println!("Daegu → {name:?}\ngrew → {word:?}");
        assert!(name.proper_noun && !word.proper_noun);
    }

    /// Every Claude feature against the real API (needs ANTHROPIC_API_KEY):
    /// cargo test real_api -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_api_listen_and_respond() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let qs = c.listen_questions(3.0, "Shopping and money", &[]).expect("questions");
        for q in &qs {
            println!("Q: {} / {} / {}", q.question, q.question_ko, q.sample_answer);
        }
        assert_eq!(qs.len(), 6);
        let good = c.judge_answer("How was your weekend?", "it was good I went to the park with my family", 3.0).expect("judge");
        let bad = c.judge_answer("How was your weekend?", "I am thirty years old", 3.0).expect("judge");
        println!("good: {good:?}\nbad: {bad:?}");
        assert!(good.understood && !bad.understood);
    }

    #[test]
    #[ignore]
    fn real_api_lesson_chunks() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let req = LessonRequest {
            day: 4, week: 1, phase: "1단계".into(), theme: "Shopping and money".into(), business: "Prices".into(), grammar: "how much/many".into(),
            level: 3.0, known: vec![], known_phrases: vec!["It depends on".into()], weak: vec![], n_words: 8, n_sentences: 5,
            avoid_sentences: vec![], avoid_questions: vec![], avoid_scenarios: vec![],
        };
        let l = c.generate_lesson(&req).expect("lesson");
        for p in &l.phrases {
            println!("chunk: {} = {} | {}", p.word, p.meaning_ko, p.example);
            assert!(p.example.to_lowercase().contains(&p.word.to_lowercase()), "example uses the chunk");
        }
        println!("words: {:?}", l.words.iter().map(|w| &w.word).collect::<Vec<_>>());
        println!("expressions: {:?}", l.expressions.iter().map(|w| &w.word).collect::<Vec<_>>());
        assert_eq!(l.phrases.len(), 4);
        assert_eq!(l.expressions.len(), 1);
        assert!(!l.phrases.iter().any(|p| p.word == "It depends on"));
    }

    #[test]
    #[ignore]
    fn real_api_grammar_roles() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let g = c.chunk_gloss("I am a boy.").expect("gloss");
        println!("{}", g.iter().map(|c| format!("{}={}({})", c.en, c.ko, c.role)).collect::<Vec<_>>().join(" / "));
        assert_eq!(g[0].role, "subject");
        assert_eq!(g[1].role, "verb");
        let texts = vec!["나는 커피를 좋아해.".to_string(), "제 일정을 확인해 볼게요.".to_string()];
        let r = c.ko_roles(&texts).expect("ko roles");
        for (t, segs) in texts.iter().zip(&r) {
            println!("{t} → {}", segs.iter().map(|s| format!("[{}:{}]", s.text, s.role)).collect::<String>());
            assert!(crate::grammar::segments_match(t, segs), "segments spell the text");
        }
    }

    #[test]
    #[ignore]
    fn real_api_all_features() {
        let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY");
        let c = Claude { url: API_URL.into(), api_key: key, model: "claude-opus-5-5".into(), lang: &crate::language::ENGLISH, down: Default::default() };
        let t = std::time::Instant::now();
        let req = LessonRequest {
            day: 3, week: 1, phase: "1단계".into(), theme: "Introducing yourself".into(),
            business: "Introducing yourself at work".into(), grammar: "be verbs".into(), level: 3.0,
            known: vec!["introduce".into()], known_phrases: vec!["It depends on".into()], weak: vec!["Nice to meet you.".into()], n_words: 4, n_sentences: 3,
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
        let sp = c.speed_passage(8, "Daily routines", "present simple", 3.0, 210, &["usually".into()], &["My Morning".into()]).expect("passage");
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
    /// A place/person name: shown, but not saved as vocabulary.
    #[serde(default)]
    pub proper_noun: bool,
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
