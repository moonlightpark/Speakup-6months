//! Functional tests: the real app (real screens, storage, scoring, SRS, HTTP client) driven
//! through its UI with egui_kittest. Two things are replaced:
//! - the learner's voice: a script of "what the learner says" feeds the simulated
//!   recorder + recognizer (real whisper recognition is covered by `stt` tests);
//! - the Claude API: a local mock server that answers the app's real HTTP requests with
//!   schema-shaped JSON, and records every request for inspection.

use crate::app::{App, Screen};
use crate::audio::{Player, Recorder, Script};
use crate::models::*;
use crate::storage::Store;
use crate::stt::Stt;
use crate::voice::{Speaker, Voice};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ mock Claude API

#[derive(Clone, Default)]
struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
}

fn ch(en: &str, ko: &str) -> Value {
    json!({"en": en, "ko": ko, "role": "subject"})
}

/// Splits each numbered Korean text into words: first = subject, last = verb, rest = object.
fn ko_roles_json(prompt: &str) -> Value {
    let texts = prompt.rsplit("\n\n").next().unwrap_or("").lines().filter_map(|l| l.split_once(". ").map(|(_, t)| t));
    let items: Vec<Value> = texts
        .map(|t| {
            let words: Vec<&str> = t.split(' ').collect();
            let segs: Vec<Value> = words
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let role = if i == 0 { "subject" } else if i + 1 == words.len() { "verb" } else { "object" };
                    json!({"text": if i + 1 < words.len() { format!("{w} ") } else { w.to_string() }, "role": role})
                })
                .collect();
            json!({"segments": segs})
        })
        .collect();
    json!({"items": items})
}

fn lesson_json() -> Value {
    json!({
        "words": [
            {"word": "deadline", "meaning_ko": "마감일", "example": "The deadline is Friday.", "example_ko": "마감은 금요일이에요."},
            {"word": "schedule", "meaning_ko": "일정", "example": "Let me check my schedule.", "example_ko": "제 일정을 확인해 볼게요."}
        ],
        "sentences": [
            {"text": "I am a boy.", "ko": "나는 소년이다.", "tip_ko": "am a 를 이어서", "chunks": [ch("I","나"), ch("am","이다"), ch("a","한"), ch("boy.","소년")]},
            {"text": "I wake up early.", "ko": "나는 일찍 일어난다.", "tip_ko": "wake up 연결", "chunks": [ch("I","나는"), ch("wake up","일어난다"), ch("early.","일찍")]}
        ],
        "self_expression": {"question": "What do you do?", "question_ko": "무슨 일 하세요?",
            "useful_phrases": [{"text": "I work as a ...", "ko": "저는 ...로 일해요"}], "model_answer": "I work as a designer."},
        "business": {"title": "Weekly meeting", "scenario_ko": "주간 회의", "ai_role": "Sarah, team lead", "user_role": "team member",
            "opening_line": "Let's get started. Any updates?", "key_phrases": [{"text": "We're on track.", "ko": "순조로워요."}]},
        "listening": {"search_queries": ["BBC Learning English meetings"], "focus_ko": "연음에 집중"}
    })
}

/// A lesson with chunks and the day's expression (phrases come first on the words screen).
fn lesson_with_chunks_json() -> Value {
    let mut l = lesson_json();
    l["phrases"] = json!([{"word": "It depends on", "meaning_ko": "~에 따라 달라요", "example": "It depends on the weather.", "example_ko": "날씨에 따라 달라요."}]);
    l["expressions"] = json!([{"word": "my bad", "meaning_ko": "내 실수야", "example": "Oops, my bad.", "example_ko": "앗, 미안."}]);
    l
}

fn respond(body: &Value) -> Value {
    let prompt = body["messages"][0]["content"].as_str().unwrap_or("").to_string();
    if prompt.starts_with("Create today's lesson") {
        if prompt.contains("chunks-test") { lesson_with_chunks_json() } else { lesson_json() }
    } else if prompt.contains("answered this question out loud") {
        json!({"fluency": 70, "grammar": 60, "vocabulary": 75, "overall": 68, "feedback_ko": "좋아요!",
            "corrections": [{"original": "I working designer", "corrected": "I work as a designer.", "explanation_ko": "동사 work + as"}],
            "natural_version": "I work as a designer. I really love my job.",
            "new_expressions": [{"text": "I really love my job.", "ko": "제 일을 정말 좋아해요."}]})
    } else if prompt.starts_with("Role-play.") {
        let learner_turns = prompt.matches("LEARNER:").count();
        if prompt.contains("(nothing yet") {
            json!({"reply": "Welcome! How can I help you today?", "reply_ko": "어서 오세요!", "correction": "", "correction_ko": "",
                "suggestions": [{"text": "I'd like to complain.", "ko": "항의하고 싶어요."}], "finished": false, "summary_ko": ""})
        } else if learner_turns >= 2 {
            json!({"reply": "Great, thanks everyone.", "reply_ko": "좋아요.", "correction": "", "correction_ko": "",
                "suggestions": [], "finished": true, "summary_ko": "잘했어요. 과거형을 연습하세요."})
        } else {
            json!({"reply": "What happened yesterday?", "reply_ko": "어제 무슨 일이?", "correction": "Yesterday I went to the client.",
                "correction_ko": "과거형 went", "suggestions": [{"text": "We finished it.", "ko": "끝냈어요."}], "finished": false, "summary_ko": ""})
        }
    } else if prompt.contains("SPEED READING") {
        json!({"title": "Coffee", "passage": "I like coffee. It is warm.", "passage_ko": "나는 커피를 좋아한다. 따뜻하다.",
            "chunks": [ch("I","나는"), ch("like","좋아한다"), ch("coffee.","커피를"), ch("It","그것은"), ch("is","이다"), ch("warm.","따뜻한")]})
    } else if prompt.starts_with("Sentence: \"") {
        let sentence = prompt.trim_start_matches("Sentence: \"").split("\"\n").next().unwrap_or("");
        json!({"chunks": sentence.split_whitespace().map(|w| ch(w, "뜻")).collect::<Vec<_>>()})
    } else if prompt.starts_with("Listen-and-respond practice.") {
        let qs: Vec<Value> = (0..6)
            .map(|i| json!({"question": format!("How was your weekend {i}?"), "question_ko": "주말 어땠어요?", "sample_answer": "It was great."}))
            .collect();
        json!({"questions": qs})
    } else if prompt.starts_with("Listen-and-respond check.") {
        let ok = prompt.contains("answer (speech recognition transcript): \"It was great");
        json!({"understood": ok, "score": if ok { 85 } else { 20 }, "feedback_ko": if ok { "잘 이해했어요" } else { "주말이 어땠는지 물었어요" }, "better_answer": "It was great, thanks!"})
    } else if prompt.starts_with("Korean grammar roles.") {
        ko_roles_json(&prompt)
    } else if prompt.starts_with("Assess the learner") {
        json!({"level": 4.6, "summary_ko": "중급 수준입니다.", "strengths_ko": ["발음"], "weaknesses_ko": ["시제"], "focus_next_ko": "과거형"})
    } else if prompt.starts_with("Homework:") {
        let ok = prompt.to_lowercase().contains("they said: \"the deadline");
        json!({"ok": ok, "feedback_ko": if ok { "정확해요" } else { "단어를 넣어 주세요" }, "better": "The deadline is tomorrow."})
    } else if prompt.starts_with("Korean: \"") {
        let ok = prompt.contains("Learner wrote: \"I really like coffee");
        json!({"ok": ok, "feedback_ko": if ok { "같은 뜻이에요" } else { "다시 써 보세요" }, "better": "I like coffee."})
    } else if prompt.contains("wrote this in English") {
        json!({"score": 72, "feedback_ko": "좋아요!", "corrections": [{"original": "I go park", "corrected": "I went to the park", "explanation_ko": "과거 시제"}],
               "natural_version": "I went to the park and had a coffee.", "natural_version_ko": "공원에 가서 커피를 마셨어요."})
    } else if prompt.starts_with("A Korean learner wants to say") {
        json!({"meaning_ko": "회의 연기 요청", "versions": [
            {"level_ko": "쉽게", "english": "Can we move the meeting?", "note_ko": "move = 옮기다"},
            {"level_ko": "중간", "english": "Could we push the meeting back a bit?", "note_ko": "push back = 미루다"},
            {"level_ko": "고급", "english": "Would it be possible to reschedule today's meeting?", "note_ko": "정중한 표현"}]})
    } else if prompt.starts_with("Rewrite this page") {
        json!({"paragraphs": ["A lion slept. A mouse ran on him."], "summary_ko": "사자와 생쥐"})
    } else if prompt.contains("Diagnose") {
        json!({"tip_ko": "th는 혀를 이 사이에", "focus_words": ["think"]})
    } else {
        json!({})
    }
}

fn start_mock() -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mock = Mock { url: format!("http://{}/v1/messages", listener.local_addr().unwrap()), requests: Arc::default() };
    let reqs = mock.requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf).unwrap();
            let body: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
            let answer = respond(&body);
            reqs.lock().unwrap().push(body);
            let msg = json!({"id": "msg_test", "type": "message", "role": "assistant", "stop_reason": "end_turn",
                "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": answer.to_string()}]})
            .to_string();
            let mut out = stream;
            let _ = write!(out, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", msg.len(), msg);
        }
    });
    mock
}

// ------------------------------------------------------------------ harness helpers

struct T {
    h: Harness<'static, App>,
    script: Script,
    mock: Mock,
    dir: PathBuf,
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("speakup-ft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn launch_at(dir: PathBuf, with_key: bool, prepare: impl FnOnce(&mut Store)) -> T {
    let mut store = Store::open_at(dir.clone()).unwrap();
    store.settings.api_key = if with_key { "test-key".into() } else { String::new() };
    prepare(&mut store);
    let script: Script = Arc::default();
    let voice = Voice {
        speaker: Speaker::silent(),
        recorder: Recorder::simulated(script.clone()),
        player: Player::silent(),
        stt: Stt::scripted(script.clone()),
        stt_ko: Stt::scripted(script.clone()),
        base_rate: 1.0,
        last_recording: Vec::new(),
    };
    let mock = start_mock();
    let url = mock.url.clone();
    let h = Harness::builder().with_size(egui::vec2(1280.0, 2400.0)).build_eframe(move |cc| {
        let mut app = App::new_with(cc, store, voice);
        app.api_url = url;
        app
    });
    let mut t = T { h, script, mock, dir };
    t.steps(3);
    t
}

/// A learner who has finished placement (most scenarios start here).
fn launch(name: &str) -> T {
    launch_at(temp_dir(name), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        s.record_level(3.0, "test setup");
        s.profile.today = no_idioms_today(mock_word_set());
    })
}

/// Today's progress with the mock word set and no idiom sentences (normally 30 from the built-in list),
/// so 문장 읽기 starts with the mock lesson's sentences.
fn no_idioms_today(words: WordSet) -> DayProgress {
    DayProgress { date: Some(today()), word_set: Some(words), idioms: Some(Vec::new()), ..Default::default() }
}

/// Today's 새 단어 set fixed to the mock lesson's two words (normally picked from the word bank).
fn mock_word_set() -> WordSet {
    WordSet { words: serde_json::from_value(lesson_json()["words"].clone()).unwrap(), ..Default::default() }
}

impl T {
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.h.step();
        }
    }
    fn app(&self) -> &App {
        self.h.state()
    }
    fn app_mut(&mut self) -> &mut App {
        self.h.state_mut()
    }
    fn has(&self, text: &str) -> bool {
        self.h.query_all(By::new().label_contains(text).include_labels()).next().is_some()
    }
    fn has_exact(&self, text: &str) -> bool {
        self.h.query_all(By::new().label(text).include_labels()).next().is_some()
    }
    #[track_caller]
    fn click(&mut self, label: &str) {
        self.h.get_by_label(label).click();
        self.steps(3);
    }
    #[track_caller]
    fn click_contains(&mut self, part: &str) {
        self.h.get_by_label_contains(part).click();
        self.steps(3);
    }
    /// Clicks a side-bar entry (the same text may also appear in the home checklist).
    fn nav(&mut self, label: &str) {
        self.h.get_all_by_label(label).next().unwrap_or_else(|| panic!("no {label}")).click();
        self.steps(3);
    }
    /// Waits (real time) for background work such as Claude calls.
    #[track_caller]
    fn wait(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        let t0 = Instant::now();
        while !cond(self.app()) {
            assert!(t0.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
            self.h.step();
            std::thread::sleep(Duration::from_millis(15));
        }
        self.steps(3);
    }
    /// The learner presses the mic, says `text` (taking `secs` seconds), and stops.
    #[track_caller]
    fn say(&mut self, text: &str, secs: f32) {
        self.script.lock().unwrap().push_back((text.to_string(), secs));
        self.click("🎤 말하기");
        self.click("⏹ 녹음 끝내기");
        self.steps(4);
    }
    fn prompts(&self) -> Vec<String> {
        self.mock.requests.lock().unwrap().iter().map(|r| r["messages"][0]["content"].as_str().unwrap_or("").to_string()).collect()
    }
}

impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ------------------------------------------------------------------ scenarios

#[test]
fn ft01_first_run_placement_test_records_level() {
    let dir = temp_dir("placement");
    let mut t = launch_at(dir.clone(), true, |_| {});
    assert_eq!(t.app().screen, Screen::Onboarding);
    assert!(t.has("말하고, 연습하고, 활용하세요."));
    t.click("레벨 테스트 시작 ▶");
    assert_eq!(t.app().screen, Screen::LevelTest);
    // Six graded sentences: read the first four well, skip the hard ones.
    for (s, _) in crate::content::PLACEMENT.iter().take(4) {
        t.say(s, 2.0);
    }
    t.click("모르겠어요 (건너뛰기)");
    t.click("모르겠어요 (건너뛰기)");
    assert!(t.has("자유롭게 말하기"));
    t.say("I am a software engineer and I want to talk with foreign clients", 5.0);
    t.wait("level assessment", |a| a.test.as_ref().is_some_and(|x| x.result.is_some()));
    assert!(t.has("중급"));
    t.click("학습 시작하기 ▶");
    let p = &t.app().store.profile;
    assert_eq!(t.app().screen, Screen::Home);
    assert!(p.placement_done);
    assert!((p.level - 4.6).abs() < 0.01);
    let csv = std::fs::read_to_string(dir.join("level_log.csv")).unwrap();
    assert!(csv.contains("4.60,B1,\"레벨 테스트 (시작)\""), "{csv}");
    // The assessment request carried the reading results and the free answer.
    let prompt = t.prompts().into_iter().find(|p| p.starts_with("Assess the learner")).unwrap();
    assert!(prompt.contains("match 100%") && prompt.contains("match 0%") && prompt.contains("foreign clients"));
}

#[test]
fn ft02_skip_placement_and_home_dashboard() {
    let mut t = launch_at(temp_dir("skip"), false, |_| {});
    t.click("테스트 없이 초급(A2)으로 시작");
    assert_eq!(t.app().screen, Screen::Home);
    for text in ["Day 1 / 182", "1주차", "자기소개 · Introducing yourself", "📚 새 단어", "⚡ 스피드 리딩·말하기", "🎧 리스닝 · 구간반복", "초중급", "0 / 60분", "▶  오늘의 학습 시작", "Claude API 키가 없어"] {
        assert!(t.has(text), "home should show {text:?}");
    }
    // Review/homework steps only appear when there is something to do.
    assert!(!t.has("🔁 복습 (배운 단어·문장)"));
    assert!(!t.has("📝 숙제 확인"));
}

#[test]
fn ft03_lesson_generation_request_shape() {
    let mut t = launch("lessonreq");
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.is_some());
    // Other background requests (Korean colour roles) may arrive first.
    let req = t.mock.requests.lock().unwrap().iter().find(|r| r["messages"][0]["content"].as_str().unwrap_or("").starts_with("Create today's lesson")).cloned().expect("lesson request");
    assert_eq!(req["model"], "claude-opus-5-5");
    assert_eq!(req["fallbacks"], "default");
    assert_eq!(req["output_config"]["format"]["type"], "json_schema");
    let schema = &req["output_config"]["format"]["schema"];
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"]["sentences"]["items"]["properties"]["chunks"].is_object(), "lesson asks for 직독직해 chunks");
    let prompt = req["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("Weekly theme: Introducing yourself") && prompt.contains("직독직해"));
    // The lesson is cached for the day and glosses are stored.
    assert!(t.dir.join("lessons").join(format!("{}.json", today())).exists());
    assert_eq!(t.app().store.gloss_for("I wake up early.").len(), 3);
}

#[test]
fn ft04_new_words_repeat_until_passed_then_saved_to_srs() {
    let mut t = launch("words");
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.is_some());
    assert!(t.has("deadline") && t.has("마감일"));
    t.nav("▶ 시작하기");
    // Word: needs 3 passing repetitions. One failed attempt in between doesn't count.
    t.say("deadline", 0.8);
    assert!(t.has("100점"));
    t.say("dead lion", 0.8);
    t.say("deadline", 0.8);
    assert!(t.query_next().is_none(), "two passes are not enough");
    t.say("deadline", 0.8);
    t.click("다음 ▶");
    // Example sentence (1 rep)
    t.say("The deadline is Friday.", 1.5);
    t.click("다음 ▶");
    // Second word + example
    for _ in 0..3 {
        t.say("schedule", 0.8);
    }
    t.click("다음 ▶");
    t.say("Let me check my schedule.", 1.5);
    t.click("다음 ▶");
    assert!(t.has("잘했어요!"));
    // The buttons sit below the celebration animation (300 px), not on top of it.
    let score_bottom = t.h.query_by_label_contains("첫 시도 평균").and_then(|n| n.raw_bounds()).unwrap().y1;
    let home_top = t.h.get_by_label("🏠 홈으로").raw_bounds().unwrap().y0;
    assert!(home_top > score_bottom + 150.0, "buttons below the animation: score ends at {score_bottom}, button starts at {home_top}");
    let items = &t.app().store.items;
    let d = items.iter().find(|i| i.text == "deadline").expect("word saved to SRS");
    assert_eq!(d.kind, ItemKind::Word);
    assert_eq!(d.due, today() + chrono::Duration::days(1), "first review tomorrow");
    assert!(t.app().step_done("words"));
    assert_eq!(t.app().store.profile.streak, 1);
    // All attempts were logged.
    let log = std::fs::read_to_string(t.dir.join("attempts.jsonl")).unwrap();
    assert!(log.lines().count() >= 9 && log.contains("dead lion"));
}

impl T {
    fn query_next(&self) -> Option<()> {
        self.h.query_by_label("다음 ▶").map(|_| ())
    }
}

#[test]
fn ft05_reading_shows_direct_reading_and_failures_become_homework() {
    let mut t = launch("reading");
    t.nav("📖  문장 읽기");
    t.wait("lesson", |a| a.lesson.is_some());
    t.nav("▶ 시작하기");
    // 직독직해: each chunk with its Korean meaning, separated by slashes.
    for text in ["I", "am", "boy.", "나", "이다", "소년", "/"] {
        assert!(t.has_exact(text), "chunk {text:?} visible");
    }
    // First try is poor (missing words) → item becomes weak.
    t.say("I a toy", 1.0);
    assert!(t.has("점"));
    for _ in 0..3 {
        t.say("I am a boy.", 1.0);
    }
    t.click("다음 ▶");
    for _ in 0..3 {
        t.say("I wake up early.", 1.2);
    }
    t.click("다음 ▶");
    // Then each sentence once more from the Korean only.
    assert!(t.has("영어로 말해 보세요") && t.has("나는 소년이다."));
    t.say("I am a boy.", 1.0);
    t.click("다음 ▶");
    assert!(t.has("나는 일찍 일어난다."));
    t.say("I wake up early.", 1.2);
    t.click("다음 ▶");
    assert!(t.has("어려웠던 항목"));
    let boy = t.app().store.items.iter().find(|i| i.text == "I am a boy.").unwrap().clone();
    assert!(boy.weak && boy.lapses == 1);
    // Mispronounced words were collected for the feedback center ("toy" ≠ "boy").
    assert!(t.app().store.mistakes.iter().any(|m| m.word == "boy"), "{:?}", t.app().store.mistakes);
    // Wrap up the day → homework for the weak item.
    t.nav("🏠  홈");
    t.click("🌙 오늘 학습 마무리");
    let hw = &t.app().store.homework;
    assert_eq!(hw.len(), 1);
    assert_eq!(hw[0].tasks[0].text, "I am a boy.");
    assert_eq!(hw[0].tasks[0].reps, 5);
}

#[test]
fn ft06_homework_is_checked_next_day() {
    let mut t = launch_at(temp_dir("homework"), true, |s| {
        s.profile.placement_done = true;
        let yesterday = today() - chrono::Duration::days(1);
        s.items.push(StudyItem {
            id: "w:deadline".into(),
            kind: ItemKind::Word,
            text: "deadline".into(),
            meaning_ko: "마감일".into(),
            example: "The deadline is Friday.".into(),
            example_ko: "마감은 금요일".into(),
            weak: true,
            lapses: 2,
            due: today() + chrono::Duration::days(1),
            ..Default::default()
        });
        s.homework.push(Homework {
            id: "hw".into(),
            assigned: yesterday,
            completed: None,
            tasks: vec![
                HomeworkTask { kind: TaskKind::ReadAloud, item_id: "w:deadline".into(), text: "The deadline is Friday.".into(), instruction_ko: "예문 읽기".into(), reps: 2, ..Default::default() },
                HomeworkTask { kind: TaskKind::UseWord, item_id: "w:deadline".into(), text: "deadline".into(), meaning_ko: "마감일".into(), instruction_ko: "나만의 문장".into(), reps: 1, ..Default::default() },
            ],
        });
    });
    assert!(t.has("📝 숙제 확인"), "homework from yesterday is today's first step");
    t.nav("📝  숙제");
    t.h.get_all_by_label("하기").next().unwrap().click();
    t.steps(3);
    t.say("The deadline is Friday.", 1.5);
    t.say("The deadline is Friday.", 1.5);
    t.click("다음 ▶");
    // Now the "use the word" task, checked by Claude.
    t.say("I have no idea", 1.0);
    t.wait("word check", |a| a.store.homework[0].tasks[1].feedback.contains("단어를"));
    t.say("The deadline for the report is tomorrow", 2.0);
    t.wait("word check ok", |a| a.store.homework[0].tasks[1].passed > 0);
    t.click("다음 ▶");
    let hw = &t.app().store.homework[0];
    assert_eq!(hw.completed, Some(today()));
    assert!(t.app().step_done("homework"));
}

#[test]
fn ft07_review_recall_with_hint_then_repeat() {
    let mut t = launch_at(temp_dir("review"), true, |s| {
        s.profile.placement_done = true;
        for (text, ko, kind) in [("schedule", "일정", ItemKind::Word), ("Nice to meet you.", "만나서 반가워요.", ItemKind::Sentence)] {
            let mut it = StudyItem { kind, text: text.into(), meaning_ko: ko.into(), example: "Let me check my schedule.".into(), ..Default::default() };
            it.id = crate::srs::make_id(kind, text);
            it.due = today();
            it.reps = 1;
            it.interval_days = 1;
            s.items.push(it);
        }
    });
    assert!(t.has_exact("🔁 복습 (배운 단어·문장)") && t.has_exact("2개"));
    t.nav("🔁  복습");
    t.click_contains("복습 시작");
    // Sentences and chunks come before single words.
    assert!(t.has("영어로 말해 보세요") && t.has("만나서 반가워요."));
    t.say("Nice to meet you.", 1.0);
    t.say("Nice to meet you.", 1.0);
    t.click("다음 ▶");
    // Recall: Korean shown, English hidden.
    assert!(t.has("영어로 말해 보세요") && t.has("일정"));
    assert!(!t.has_exact("schedule"));
    t.click("💡 힌트");
    assert!(t.has("s_______"));
    t.say("I think it's schedule", 1.0); // said the word → recall success
    assert!(t.has("100점"));
    t.say("schedule", 0.8);
    t.click("다음 ▶");
    assert!(t.app().step_done("review"));
    let s = t.app().store.items.iter().find(|i| i.text == "schedule").unwrap();
    assert_eq!(s.interval_days, 3, "second successful review → 3 days");
}

#[test]
fn ft08_self_expression_feedback_corrections_and_natural_version() {
    let mut t = launch("selfexpr");
    t.nav("💬  자기 표현");
    t.wait("lesson", |a| a.lesson.is_some());
    assert!(t.has("What do you do?"));
    t.say("I working designer", 3.0);
    t.wait("feedback", |a| a.selfx.feedback.is_some());
    for text in ["68", "유창성", "✖ I working designer", "✓ I work as a designer.", "원어민처럼 말하면"] {
        assert!(t.has(text), "{text}");
    }
    assert_eq!(t.app().store.corrections.len(), 1, "correction saved to the feedback center");
    t.click("▶ 자연스러운 버전 소리 내어 읽기");
    // natural version split into sentences + the new expression
    for s in ["I work as a designer.", "I really love my job."] {
        for _ in 0..3 {
            t.say(s, 1.5);
        }
        t.click("다음 ▶");
    }
    for _ in 0..3 {
        t.say("I really love my job.", 1.5);
    }
    t.click("다음 ▶");
    assert!(t.app().step_done("speaking"));
    assert!(t.app().store.items.iter().any(|i| i.text == "I work as a designer." && i.category == "self-expression"));
}

#[test]
fn ft09_business_roleplay_conversation() {
    let mut t = launch("roleplay");
    t.nav("💼  회화 · 비즈니스");
    t.wait("lesson", |a| a.lesson.is_some());
    assert!(t.has("Weekly meeting") && t.has("We're on track."));
    t.click("▶ 대화 시작");
    assert!(t.has("Let's get started. Any updates?"));
    t.say("Yesterday I go to the client", 2.0);
    t.wait("ai reply", |a| a.roleplay.lines.len() >= 3);
    assert!(t.has("What happened yesterday?"));
    assert!(t.has("✓ 더 자연스럽게: Yesterday I went to the client."));
    assert!(t.has("💡 이렇게 말해 보세요") && t.has("We finished it."));
    t.say("We finished it", 1.5);
    t.wait("finish", |a| a.roleplay.finished);
    assert!(t.has("🎉 대화 완료!") && t.has("과거형을 연습하세요"));
    assert!(t.app().store.corrections.iter().any(|c| c.source == "회화" && c.corrected == "Yesterday I went to the client."));
    assert_eq!(t.app().store.word_stat("대화: Weekly meeting").passes, 1, "a finished conversation counts toward the scene's colour");
    // The request contained the scenario and the conversation so far.
    let p = t.prompts().into_iter().filter(|p| p.starts_with("Role-play.")).last().unwrap();
    assert!(p.contains("Sarah, team lead") && p.contains("LEARNER: Yesterday I go to the client"));
}

#[test]
fn ft10_custom_scene_ai_opens_and_collection_exists() {
    let mut t = launch("custom");
    t.nav("💼  회화 · 비즈니스");
    t.wait("lesson", |a| a.lesson.is_some());
    t.click("🎭 상황 모음");
    assert!(t.has("Job interview") && t.has("🎲 랜덤"));
    t.click("✏ 나만의 상황");
    assert!(t.h.query_by_label("▶ 대화 시작").is_none(), "needs a situation first");
    t.app_mut().roleplay.custom = ("호텔 예약 누락 항의".into(), "hotel manager".into(), "guest".into());
    t.steps(3);
    t.click("▶ 대화 시작");
    t.wait("AI opening line", |a| !a.roleplay.lines.is_empty());
    assert!(t.has("Welcome! How can I help you today?"));
    let p = t.prompts().into_iter().filter(|p| p.starts_with("Role-play.")).last().unwrap();
    assert!(p.contains("호텔 예약 누락 항의") && p.contains("You play hotel manager") && p.contains("you speak first"));
}

#[test]
fn ft11_speed_reading_wpm_and_quick_response() {
    let mut t = launch("speed");
    t.nav("⚡  스피드 트레이닝");
    t.click("▶ 지문 받기");
    t.wait("passage", |a| a.speed.passage.is_some());
    assert!(t.has_exact("좋아한다"), "passage shown with 직독직해");
    // Without the gloss, each sentence sits on its own line (Order.md), translation too.
    t.click("📖 직독직해");
    assert!(t.has_exact("I like coffee.") && t.has_exact("It is warm."), "one label per sentence");
    t.click("해석");
    assert!(t.has_exact("나는 커피를 좋아한다.") && t.has_exact("따뜻하다."));
    t.click("해석");
    t.click("📖 직독직해");
    // 6 words in 2.4 s of speech = 150 WPM
    t.say("I like coffee. It is warm.", 2.4);
    let first = t.app().store.profile.speed_history[0].wpm;
    assert!((first - 150.0).abs() < 4.0, "{first}");
    assert!(t.has_exact(&format!("{first:.0} WPM")), "WPM shown");
    assert!(t.has("정확도 100%") && t.has(&format!("🎯 목표 달성! {first:.0} WPM")), "animated cheer for reaching the goal");
    t.click("↻ 같은 지문 더 빠르게");
    t.say("I like coffee. It is warm.", 1.8);
    let hist = t.app().store.profile.speed_history.clone();
    assert_eq!(hist.len(), 2);
    assert!((hist[1].wpm - 200.0).abs() < 5.0, "{}", hist[1].wpm);
    assert!(t.has(&format!("반복 읽기: 1회 {:.0}(100%) → 2회 {:.0}(100%)", hist[0].wpm, hist[1].wpm)));
    // Quick response: mic opens by itself when the Korean prompt appears.
    t.click("💨 순발력 말하기");
    t.click("▶ 시작 (10문제)");
    assert!(t.app().voice.recorder.is_recording(), "mic auto-started");
    let target = t.app().speed.queue[0].0.clone();
    t.script.lock().unwrap().push_back((target.clone(), 1.0));
    t.click("⏹ 녹음 끝내기");
    t.steps(3);
    assert!(t.has("✅ 통과"), "fast and correct answer passes");
    assert!(t.app().store.profile.today.speed_rounds >= 2);
    // The end of a round: the celebration (here the last of 10 answers, the others skipped).
    let n = t.app().speed.queue.len();
    t.app_mut().speed.stats.resize(n - 1, (1.0, 100.0, true));
    t.app_mut().speed.idx = n - 1;
    t.app_mut().speed.result = Some((String::new(), crate::scoring::score("go", "go"), 1.0, true));
    t.steps(2);
    t.click("다음 ▶");
    assert!(t.has("🎉 완벽해요! 번개 같아요!") && t.has(&format!("빠르게 말하기 {}/{} 통과", n - 1, n - 1)));
}

#[test]
fn ft12_feedback_center_pronunciation_practice_clears_mistakes() {
    let mut t = launch_at(temp_dir("feedback"), true, |s| {
        s.profile.placement_done = true;
        s.add_mistakes(&["think".into(), "think".into(), "world".into()]);
    });
    assert!(t.has("발음 실수 2개 연습하기"));
    t.nav("🔬  피드백 센터");
    assert!(t.has("think") && t.has("2회"));
    t.click_contains("발음 연습");
    for _ in 0..3 {
        t.say("think", 0.6);
    }
    t.click("다음 ▶");
    let m = t.app().store.mistakes.iter().find(|m| m.word == "think").unwrap();
    assert_eq!(m.fixed_streak, 3, "three correct in a row → solved");
    assert!(t.has("남은 단어 1개"));
}

#[test]
fn ft13_listening_lab_segments_loop_and_shadowing() {
    let dir = temp_dir("listening");
    // A downloaded video: 2 s of audio + parsed subtitle segments.
    let vdir = dir.join("youtube").join("abcdefghijk");
    std::fs::create_dir_all(&vdir).unwrap();
    let wav = vdir.join("audio.wav");
    let mut w = hound::WavWriter::create(&wav, hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
    for i in 0..32000 {
        w.write_sample(((i as f32 * 0.05).sin() * 8000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
    let video = crate::youtube::Video {
        id: "abcdefghijk".into(),
        title: "Test Talk".into(),
        url: "https://www.youtube.com/watch?v=abcdefghijk".into(),
        audio: wav,
        segments: vec![
            crate::youtube::Segment { start: 0.0, end: 0.9, text: "Hello everyone.".into() },
            crate::youtube::Segment { start: 1.0, end: 1.9, text: "Welcome back to the show.".into() },
        ],
    };
    std::fs::write(vdir.join("video.json"), serde_json::to_string(&video).unwrap()).unwrap();
    let mut t = launch_at(dir, true, |s| s.profile.placement_done = true);
    t.nav("🎧  리스닝 랩");
    t.click_contains("Test Talk");
    assert!(t.has("Hello everyone.") && t.has("1 / 2"));
    t.click("▶ 재생");
    t.wait("segment decoded", |a| !a.listening.clip.running());
    t.click("자막 숨기기");
    assert!(t.has("자막 숨김"));
    t.click("자막 숨기기");
    t.click("💬 따라 말하기 (쉐도잉)");
    t.wait("segment decoded", |a| !a.listening.clip.running());
    t.say("Hello everyone", 1.0);
    t.say("Hello everyone", 1.0);
    assert_eq!(t.app().store.profile.today.listening_passes, 2);
    t.click("다음 ▶");
    assert!(t.has("2 / 2") && t.has("Welcome back to the show."));
    t.click("⭐ 이 문장 저장");
    assert!(t.app().store.items.iter().any(|i| i.text == "Welcome back to the show." && i.category == "youtube"));
}

#[test]
fn ft14_settings_persist_and_data_survives_restart() {
    let dir = temp_dir("persist");
    {
        let mut t = launch_at(dir.clone(), true, |s| s.profile.placement_done = true);
        t.nav("⚙  설정");
        assert!(t.has("Claude API") && t.has("통과 점수") && t.has("일일 목표 학습 시간"));
        t.app_mut().store.settings.daily_goal_minutes = 45;
        t.app_mut().store.save_settings();
        t.app_mut().store.add_mistakes(&["world".into()]);
        t.app_mut().complete_step("words");
        t.dir = PathBuf::new(); // keep the data for the second launch
    }
    let t = launch_at(dir.clone(), true, |s| {
        assert_eq!(s.settings.daily_goal_minutes, 45);
        assert_eq!(s.mistakes.len(), 1);
        assert_eq!(s.profile.today.done, vec!["words".to_string()]);
    });
    assert!(t.has("0 / 45분"));
    assert!(t.has("✓ 완료"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ft15_new_day_resets_progress_and_keeps_streak() {
    let mut t = launch_at(temp_dir("newday"), true, |s| {
        s.profile.placement_done = true;
        s.profile.streak = 4;
        s.profile.last_study_date = Some(today() - chrono::Duration::days(1));
        s.profile.today = DayProgress { date: Some(today() - chrono::Duration::days(1)), done: vec!["words".into()], ..Default::default() };
    });
    assert!(t.app().store.profile.today.done.is_empty(), "yesterday's checklist is reset");
    assert!(t.has_exact("🔥 4일"));
    t.app_mut().complete_step("listening");
    assert_eq!(t.app().store.profile.streak, 5, "studying today extends the streak");
}

#[test]
fn ft16_offline_mode_uses_builtin_lesson() {
    let mut t = launch_at(temp_dir("offline"), false, |s| s.profile.placement_done = true);
    t.nav("📚  새 단어");
    t.steps(3);
    let l = t.app().lesson.clone().expect("built-in lesson without API key");
    assert_eq!(l.source, "built-in");
    // Words come from the built-in 6,000-word database, picked for the learner's level.
    assert_eq!(l.words.len(), 35, "35 new words a day");
    let first = crate::wordbank::pick(t.app().store.profile.level, 1, |_| false)[0];
    assert_eq!(l.words[0].word, first.word);
    assert!(t.has(&first.word) && t.has(&first.meaning_ko));
    assert_eq!(t.app().store.profile.today.word_set.as_ref().map(|w| w.words.clone()), Some(l.words.clone()), "lesson uses the same words");
    t.nav("💼  회화 · 비즈니스");
    assert!(t.has("📴 오프라인 대본 모드") && t.has("Claude API 키가 없어서"), "offline scripts without a key");
    assert!(t.prompts().is_empty(), "no API calls without a key");
}

#[test]
fn ft17_history_records_each_day_and_screen_shows_it() {
    let mut t = launch("history");
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.is_some());
    t.nav("▶ 시작하기");
    for _ in 0..3 {
        t.say("deadline", 0.8);
    }
    t.click("다음 ▶");
    t.app_mut().complete_step("words");
    let raw = std::fs::read_to_string(t.dir.join("history.json")).expect("history.json written");
    let hist: Vec<DayRecord> = serde_json::from_str(&raw).unwrap();
    assert_eq!(hist.len(), 1);
    let r = &hist[0];
    assert_eq!(r.date, Some(today()));
    assert_eq!(r.steps, vec!["words".to_string()]);
    assert_eq!(r.attempts, 3);
    assert_eq!(r.new_words, vec!["deadline".to_string()]);
    assert_eq!(r.question, "What do you do?");
    t.nav("📅  학습 기록");
    for text in ["학습한 날", "6개월 학습 달력", "Day 1 · 자기소개", "📚 새 단어 1개: deadline", "💬 자기 표현 질문: What do you do?", "💼 회화 상황: Weekly meeting"] {
        assert!(t.has(text), "history screen shows {text:?}");
    }
}

#[test]
fn ft18_next_day_lesson_must_not_repeat_previous_content() {
    let dir = temp_dir("norepeat");
    let yesterday = today() - chrono::Duration::days(1);
    let mut t = launch_at(dir, true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = yesterday;
        // Yesterday's lesson and what was learned from it.
        let mut l: Lesson = serde_json::from_value(lesson_json()).unwrap();
        l.date = Some(yesterday);
        l.source = "claude".into();
        s.save_lesson(&l);
        crate::srs::add_item(&mut s.items, StudyItem { kind: ItemKind::Sentence, text: "I am a boy.".into(), ..Default::default() });
        crate::srs::add_item(&mut s.items, StudyItem { kind: ItemKind::Word, text: "deadline".into(), ..Default::default() });
    });
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.as_ref().is_some_and(|l| l.date == Some(today())));
    let prompt = t.prompts().into_iter().find(|p| p.starts_with("Create today's lesson")).unwrap();
    assert!(prompt.contains("Program day 2 of 182"));
    assert!(prompt.contains("must be NEW"));
    assert!(prompt.contains("Words the learner already knows (do NOT repeat them): deadline"), "{prompt}");
    assert!(prompt.contains("self-expression questions: What do you do?"));
    assert!(prompt.contains("role-play scenarios: Weekly meeting"));
    assert!(prompt.contains("I am a boy."));
}

#[test]
fn ft19_offline_lessons_rotate_questions_and_scenarios() {
    let dir = temp_dir("offline-rotate");
    let store = Store::open_at(dir.clone()).unwrap();
    let day1 = crate::content::builtin_lesson(1, 1, &store.items, &[], 8, 5);
    let day2 = crate::content::builtin_lesson(1, 2, &[], &[day1.clone()], 8, 5);
    assert_ne!(day1.self_expression.question, day2.self_expression.question);
    assert_ne!(day1.business.title, day2.business.title);
    // Words already learned are never served again.
    let learned: Vec<StudyItem> = day1.words.iter().map(|w| StudyItem { kind: ItemKind::Word, text: w.word.clone(), ..Default::default() }).collect();
    let day2b = crate::content::builtin_lesson(1, 2, &learned, &[day1.clone()], 8, 5);
    assert!(day2b.words.iter().all(|w| !day1.words.iter().any(|o| o.word == w.word)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ft19_saved_name_and_key_are_not_asked_again() {
    let dir = temp_dir("remember");
    {
        // First run: the learner types a name and key, then quits before the level test.
        let mut t = launch_at(dir.clone(), false, |_| {});
        assert_eq!(t.app().screen, Screen::Onboarding);
        t.app_mut().store.profile.name = "Tyler".into();
        t.app_mut().store.settings.api_key = "sk-ant-test".into();
        t.app_mut().store.save_profile();
        t.app_mut().store.save_settings();
        t.dir = temp_dir("remember-scratch"); // keep the real data folder when `t` is dropped
    }
    let store = Store::open_at(dir.clone()).unwrap();
    assert_eq!(store.profile.name, "Tyler");
    assert_eq!(store.settings.api_key, "sk-ant-test");
    let script: Script = Arc::default();
    let voice = Voice {
        speaker: Speaker::silent(),
        recorder: Recorder::simulated(script.clone()),
        player: Player::silent(),
        stt: Stt::scripted(script.clone()),
        stt_ko: Stt::scripted(script.clone()),
        base_rate: 1.0,
        last_recording: Vec::new(),
    };
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 2400.0)).build_eframe(move |cc| App::new_with(cc, store, voice));
    h.run_steps(3);
    // Second run: both are shown as saved instead of empty inputs.
    assert!(h.query_by_label_contains("✅ Tyler").is_some());
    assert!(h.query_by_label_contains("✅ 저장됨").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ft20_library_reads_a_story_and_tracks_progress() {
    let mut t = launch("library");
    t.nav("📘  리딩 라이브러리");
    assert!(t.has("The Lion and the Mouse") && t.has("동화"));
    t.h.get_all_by_label("읽기 ›").next().unwrap().click();
    t.steps(3);
    assert!(t.has("A big lion was sleeping under a tree."));
    // Selecting a sentence opens its tools.
    t.click("A big lion was sleeping under a tree.");
    assert!(t.has("🎤 소리 내어 읽기") && t.has("📖 직독직해"));
    t.click("📖 직독직해");
    t.wait("gloss", |a| !a.store.gloss_for("A big lion was sleeping under a tree.").is_empty());
    t.click("⭐ 문장 저장 (복습)");
    assert!(t.app().store.items.iter().any(|i| i.text == "A big lion was sleeping under a tree." && i.category == "reading"));
    // Easier rewrite by Claude.
    t.click("✨ 쉬운 영어로 다시 쓰기");
    t.wait("simplified", |a| a.library.open.as_ref().is_some_and(|r| r.paragraphs_shown().len() == 1));
    t.click("✓ 다 읽었어요");
    let app = t.app();
    assert!(app.library.open.is_none());
    assert_eq!(app.store.profile.today.paragraphs_read, 6);
    assert!(app.store.library["lion-mouse"].finished);
    assert!(t.dir.join("library.json").exists());
}

#[test]
fn ft21_writing_dictation_translation_and_diary() {
    let mut t = launch_at(temp_dir("writing"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        s.items.push(StudyItem { id: "s:1".into(), kind: ItemKind::Sentence, text: "I like coffee.".into(), meaning_ko: "나는 커피를 좋아해.".into(), ..Default::default() });
    });
    t.nav("✏  쓰기");
    // Dictation: exact answer scores 100.
    t.app_mut().writing.input = "I like coffee".into();
    t.click("확인");
    assert!(t.has("100점") && t.has("🎉 완벽해요! 정답이에요!"), "animated cheer for a right answer");
    t.click("다음 ▶");
    assert_eq!(t.app().store.profile.today.writings_done, 1);
    // Translation: a different but correct wording is accepted by Claude.
    t.click("🔁 한→영 작문");
    assert!(t.has("나는 커피를 좋아해."));
    t.app_mut().writing.input = "I really like coffee".into();
    t.click("확인");
    t.wait("translation check", |a| t_done(a));
    assert!(t.has("같은 뜻이에요"));
    t.click("다음 ▶");
    // Diary: Claude corrects, the correction goes to the feedback center, then read it aloud.
    t.click("📝 영어 일기");
    // A2–B1: 40 easy example diary sentences to look at; not shown from B1+.
    t.click("📒 예제 일기 문장 40개 보기 (초등학교 수준)");
    assert!(t.has("I got up at seven o'clock this morning.") && t.has("It was a good day."));
    t.app_mut().store.profile.level = 6.0;
    t.steps(2);
    assert!(!t.has("📒 예제 일기"), "B1+ and up: no examples");
    t.app_mut().store.profile.level = 3.0;
    t.app_mut().writing.input = "Today I go park and drink coffee.".into();
    t.steps(2);
    t.click("✨ Claude 교정 받기");
    t.wait("diary feedback", |a| a.writing.diary_result.is_some());
    assert!(t.has("72점") && t.has("I went to the park and had a coffee."));
    assert!(t.app().store.corrections.iter().any(|c| c.corrected == "I went to the park"));
    t.say("I went to the park and had a coffee.", 3.0);
    assert_eq!(t.app().store.profile.today.writings_done, 3);
    assert_eq!(t.app().store.writings.len(), 3);
}

fn t_done(a: &App) -> bool {
    a.writing.translation_done()
}

#[test]
fn ft22_study_time_is_split_by_skill() {
    let t = launch("skills");
    assert!(t.has("오늘의 학습 비중") && t.has("🎤 말하기") && t.has("🎧 듣기") && t.has("📘 읽기") && t.has("✏ 쓰기"));
    assert_eq!(crate::app::skill_of(Screen::Listening), Some(1));
    assert_eq!(crate::app::skill_of(Screen::Library), Some(2));
    assert_eq!(crate::app::skill_of(Screen::Writing), Some(3));
    assert_eq!(crate::app::skill_of(Screen::Business), Some(0));
    let w: f32 = crate::app::SKILL_WEIGHTS.iter().map(|s| s.1).sum();
    let weights: Vec<f32> = crate::app::SKILL_WEIGHTS.iter().map(|s| s.1).collect();
    assert!((w - 1.0).abs() < 1e-6 && weights == [0.4, 0.3, 0.15, 0.15]);
    // The routine has a reading and a writing step.
    assert!(t.has("📘 리딩 (동화·소설)") && t.has("✏ 쓰기 (받아쓰기·작문)"));
}

#[test]
fn ft23_translator_korean_speech_to_three_levels() {
    let mut t = launch("translator");
    t.nav("🌐  자동 번역기");
    // Speak Korean into the mic (the scripted Korean recognizer is ready in tests).
    t.say("오늘 회의 좀 미룰 수 있을까요?", 2.0);
    t.wait("translation", |a| a.translator.result.is_some());
    for text in ["쉽게", "중간", "고급", "Can we move the meeting?", "Could we push the meeting back a bit?", "Would it be possible to reschedule today's meeting?", "💡 push back = 미루다"] {
        assert!(t.has(text), "translator should show {text:?}");
    }
    assert!(t.prompts().iter().any(|p| p.contains("오늘 회의 좀 미룰 수 있을까요?")));
    // Repeat the everyday version out loud, and save it for review.
    t.h.get_all_by_label("🎤 따라 말하기").nth(1).unwrap().click();
    t.steps(3);
    // Two mic buttons now: the Korean one at the top and the drill's.
    t.script.lock().unwrap().push_back(("Could we push the meeting back a bit?".into(), 2.0));
    t.h.get_all_by_label("🎤 말하기").nth(1).unwrap().click();
    t.steps(3);
    t.click("⏹ 녹음 끝내기");
    t.steps(4);
    assert!(t.app().store.profile.today.scores.contains(&100.0), "drill scored the repeat");
    t.h.get_all_by_label("⭐ 저장 (복습)").nth(1).unwrap().click();
    t.steps(3);
    assert!(t.app().store.items.iter().any(|i| i.text == "Could we push the meeting back a bit?" && i.meaning_ko == "오늘 회의 좀 미룰 수 있을까요?"));
    // Typing works too.
    t.app_mut().translator.korean = "배고파 죽겠어".into();
    t.click("번역 ›");
    t.wait("second translation", |a| a.translator.result.is_some());
    assert!(t.prompts().iter().any(|p| p.contains("배고파 죽겠어")));
}

#[test]
fn ft24_home_total_time_words_and_level_retest() {
    let mut t = launch_at(temp_dir("retest"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        s.record_level(3.0, "test setup");
        s.profile.study_seconds = 125 * 60; // studied 2h 5m so far
    });
    // Total time against the whole program (182 days × 60 min = 10,920 min).
    for text in ["2시간 5분", "6개월 목표 10,920분", "1.1% 완료", "초중급 A2", "레벨 3.0 / 10", "0 / 1,000", "배운 단어 / A2 필수"] {
        assert!(t.has(text), "home should show {text:?}");
    }
    // Retest from the home screen moves straight to the assessed level.
    t.click("📝 레벨 테스트");
    assert_eq!(t.app().screen, Screen::LevelTest);
    for (s, _) in crate::content::PLACEMENT.iter().take(4) {
        t.say(s, 2.0);
    }
    t.click("모르겠어요 (건너뛰기)");
    t.click("모르겠어요 (건너뛰기)");
    t.say("I am a software engineer and I want to talk with foreign clients", 5.0);
    t.wait("level assessment", |a| a.test.as_ref().is_some_and(|x| x.result.is_some()));
    assert!(t.has("현재 초중급 A2 (3.0) → 새 레벨 중급 B1 (4.6)"));
    t.click("이 레벨로 이동 ▶");
    assert_eq!(t.app().screen, Screen::Home);
    assert!((t.app().store.profile.level - 4.6).abs() < 0.01, "level replaced, not blended");
    assert!(t.has("중급 B1") && t.has("0 / 2,000"));
    let csv = std::fs::read_to_string(t.dir.join("level_log.csv")).unwrap();
    assert!(csv.contains("4.60,B1,\"레벨 테스트 (홈에서 재측정)\""), "{csv}");
}

#[test]
fn level_labels_agree() {
    // Name, CEFR grade and vocabulary target come from one table.
    assert_eq!((level_name_of(8.2), cefr_of(8.2), level_words(8.2)), ("고급+", "B2+", 5000));
    assert_eq!((level_name_of(1.0), cefr_of(1.0), level_words(1.0)), ("초급", "A1", 500));
    assert_eq!((level_name_of(10.0), cefr_of(10.0), level_words(10.0)), ("능숙", "C1", 8000));
}

#[test]
fn place_and_person_names_are_not_vocabulary() {
    for name in ["Daegu", "Seoul", "seoul", "Minji", "New York", "Bank of America"] {
        assert!(is_proper_noun(name, ""), "{name} is a name");
    }
    assert!(is_proper_noun("daegu", "대구 (도시)"));
    for word in ["nice to meet you", "be from", "IT", "CEO", "I'm", "My bad", "Netflix and chill", "city"] {
        assert!(!is_proper_noun(word, "도시"), "{word} is vocabulary");
    }
    let item = |text: &str, kind| StudyItem { kind, text: text.into(), ..Default::default() };
    let items = [item("Daegu", ItemKind::Word), item("Seoul", ItemKind::Word), item("be from", ItemKind::Word), item("I'm from Seoul.", ItemKind::Sentence)];
    assert_eq!(items.iter().filter(|i| i.is_vocab_word()).count(), 1);
}

#[test]
fn ft25_korean_passages_and_gloss_are_coloured_by_role() {
    let mut t = launch_at(temp_dir("roles"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        s.items.push(StudyItem { id: "s:1".into(), kind: ItemKind::Sentence, text: "I like coffee.".into(), meaning_ko: "나는 커피를 좋아해.".into(), ..Default::default() });
    });
    t.nav("✏  쓰기");
    t.click("🔁 한→영 작문");
    // The Korean prompt is annotated in the background and cached for next time.
    let cache = t.dir.join("ko_roles.json");
    t.wait("korean roles", |_| std::fs::read_to_string(&cache).is_ok_and(|c| c.contains("나는 커피를 좋아해.")));
    let saved: Vec<(String, Vec<crate::grammar::KoSeg>)> = serde_json::from_str(&std::fs::read_to_string(&cache).unwrap()).unwrap();
    let segs = &saved.iter().find(|(k, _)| k == "나는 커피를 좋아해.").unwrap().1;
    assert_eq!(segs.iter().map(|s| s.role.as_str()).collect::<Vec<_>>(), ["subject", "object", "verb"]);
    assert!(t.has("나는 커피를 좋아해."), "coloured text still reads as one sentence");
    let req = t.mock.requests.lock().unwrap().iter().find(|r| r["messages"][0]["content"].as_str().unwrap_or("").starts_with("Korean grammar roles.")).cloned().unwrap();
    let roles = &req["output_config"]["format"]["schema"]["properties"]["items"]["items"]["properties"]["segments"]["items"]["properties"]["role"]["enum"];
    assert!(roles.as_array().unwrap().iter().any(|r| r == "complement"));
    // 직독직해 requests ask for a role per chunk, and role-less old glosses are fetched again.
    let mut store = Store::open_at(t.dir.clone()).unwrap();
    store.gloss.insert("I am a boy.".into(), vec![Chunk { en: "I am a boy.".into(), ko: "나는 소년".into(), role: String::new() }]);
    assert!(store.gloss_for("I am a boy.").is_empty());
}

#[test]
fn ft26_korean_colours_can_be_switched_off() {
    let mut t = launch_at(temp_dir("roles-off"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        s.settings.ko_role_colors = false;
        s.items.push(StudyItem { id: "s:1".into(), kind: ItemKind::Sentence, text: "I like coffee.".into(), meaning_ko: "나는 커피를 좋아해.".into(), ..Default::default() });
    });
    t.nav("✏  쓰기");
    t.click("🔁 한→영 작문");
    t.steps(30);
    assert!(t.has("나는 커피를 좋아해."));
    assert!(!t.prompts().iter().any(|p| p.starts_with("Korean grammar roles.")), "no annotation requests while switched off");
    assert!(!t.dir.join("ko_roles.json").exists());
    // The switch is in settings, and turning it back on colours the passage.
    t.nav("⚙  설정");
    t.click("한글 지문에 문장 성분 색상 표시 (주어·동사·목적어…)");
    assert!(t.app().store.settings.ko_role_colors);
    t.nav("✏  쓰기");
    t.click("🔁 한→영 작문");
    let cache = t.dir.join("ko_roles.json");
    t.wait("korean roles", |_| cache.exists());
}

#[test]
fn ft27_chunks_first_high_frequency_words_and_one_expression_a_day() {
    let mut t = launch_at(temp_dir("chunks"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        // A known word that makes the mock lesson include chunks; due later so it isn't reviewed.
        s.items.push(StudyItem { id: "w:chunks-test".into(), kind: ItemKind::Word, text: "chunks-test".into(), due: today() + chrono::Duration::days(9), ..Default::default() });
    });
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.is_some());
    let req = t.mock.requests.lock().unwrap().iter().find(|r| r["messages"][0]["content"].as_str().unwrap_or("").starts_with("Create today's lesson")).cloned().unwrap();
    let prompt = req["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("phrases: exactly 4 high-frequency spoken chunks"));
    assert!(prompt.contains("from the 2,000 most frequent words"), "beginners get the top-2,000 band");
    assert!(prompt.contains("expressions: exactly 1 item"), "idioms/slang ≈ 5%: one a day");
    assert!(req["output_config"]["format"]["schema"]["properties"]["phrases"].is_object());
    // The 새 단어 set itself is picked locally: 4 chunks, 1 expression, words from the word bank.
    let ws = t.app().store.profile.today.word_set.clone().expect("word set picked");
    let chunks = crate::expressions::CHUNKS;
    assert_eq!(ws.phrases.iter().map(|w| w.word.as_str()).collect::<Vec<_>>(), chunks[..4].iter().map(|c| c.0).collect::<Vec<_>>());
    assert_eq!(ws.expressions.len(), 1);
    assert_eq!(ws.words[0].word, crate::wordbank::pick(t.app().store.profile.level, 1, |_| false)[0].word);
    assert!(t.has(&format!("🔗 {}", chunks[0].0)));
    // The chunk is drilled first, then its example, then the next chunk.
    t.nav("▶ 시작하기");
    for _ in 0..3 {
        t.say(chunks[0].0, 1.0);
    }
    t.click("다음 ▶");
    t.say(chunks[0].2, 1.5);
    t.click("다음 ▶");
    assert!(t.has(chunks[1].0));
    let item = t.app().store.items.iter().find(|i| i.text == chunks[0].0).cloned().expect("chunk saved for review");
    assert!(item.is_chunk() && !item.is_vocab_word() && item.kind == ItemKind::Sentence);
}

#[test]
fn ft28_listen_and_respond() {
    let mut t = launch("listenrespond");
    t.nav("🎧  리스닝 랩");
    t.click("👂 듣고 대답하기");
    t.click("▶ 질문 6개 받기");
    t.wait("questions", |a| a.coach.listen.questions.len() == 6);
    // The question is only heard: no text until answering.
    assert!(t.has("🙈 질문 글은 대답한 뒤에 보여요") && !t.has("How was your weekend 0?"));
    t.say("It was great, I went hiking.", 2.0);
    t.wait("judgement", |a| a.coach.listen.result.is_some());
    assert!(t.has("✅ 질문을 이해했어요") && t.has("How was your weekend 0?") && t.has("It was great, thanks!"));
    assert_eq!(t.app().store.profile.today.listening_passes, 1, "an understood answer counts toward listening");
    t.click("다음 ▶");
    t.say("I like pizza.", 1.0);
    t.wait("judgement", |a| a.coach.listen.result.is_some());
    assert!(t.has("❌ 질문을 다르게 이해했어요") && t.has("주말이 어땠는지 물었어요"));
    assert_eq!(t.app().store.profile.today.listening_passes, 1);
}

#[test]
fn ft29_four_three_two_speaking() {
    let mut t = launch("ftt");
    t.nav("⚡  스피드 트레이닝");
    t.click("⏱ 4/3/2 말하기");
    t.nav("▶ 시작하기");
    t.steps(3);
    assert!(t.has("라운드 1/3 · 4분"));
    t.say("I usually get up at seven and go to work by bus", 20.0);
    assert!(t.has("라운드 2/3 · 3분"));
    t.say("I usually get up at seven and go to work by bus", 12.0);
    t.say("I usually get up at seven and go to work by bus", 8.0);
    let r = &t.app().coach.ftt.results;
    assert_eq!(r.len(), 3);
    assert!(r[2].wpm > r[0].wpm, "same words in less time → faster");
    assert!(t.has("말하기 속도"));
    assert_eq!(t.app().store.profile.today.speed_rounds, 3);
    assert!(t.app().step_done("speed"));
}

#[test]
fn ft30_weekly_recording_tutor_and_mock_tests() {
    let mut t = launch("weekly");
    // Home lists this week's to-dos outside the routine.
    assert!(t.has("이번 주 할 일") && t.has("📼 이번 주 1분 녹음을 아직 안 했어요"));
    assert!(!t.has("🎓 이번 주 원어민 화상 수업 (주 1~2회 권장)"), "tutor lessons are suggested from month 3");
    t.nav("📈  레벨 · 평가");
    assert!(t.has("📼 주간 녹음 비교") && t.has("Tell me about yourself and what you did this week."));
    t.say("Hi, I'm Minji. This week I studied English every day.", 30.0);
    let rec = t.app().store.profile.weekly_recordings[0].clone();
    assert_eq!((rec.week, rec.words), (1, 10));
    let wav = t.dir.join("recordings").join(&rec.file);
    let (samples, rate) = crate::audio::decode_segment(&wav, 0.0, 75.0).expect("recording saved as WAV");
    assert_eq!(rate, 16_000);
    assert!(samples.len() as f32 / 16_000.0 > 29.0);
    // Re-recording in the same week replaces it.
    t.say("This week I went to work and studied.", 20.0);
    assert_eq!(t.app().store.profile.weekly_recordings.len(), 1);
    t.click("✅ 오늘 수업했어요");
    assert_eq!(t.app().store.profile.tutor_sessions, vec![today()]);
    t.app_mut().coach.test_result = "IM2".into();
    t.click("기록");
    assert_eq!(t.app().store.profile.external_tests[0].result, "IM2");
    assert!(t.has("다음 모의고사까지 14일"));
    t.nav("🏠  홈");
    assert!(!t.has("이번 주 할 일"), "nothing left this week");
}

#[test]
fn ft31_korean_support_fades_over_the_months() {
    // Month 4 (Day 100): 직독직해 off by default, Korean colours switched off once.
    let mut t = launch_at(temp_dir("stage1"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today() - chrono::Duration::days(99);
        s.profile.today = no_idioms_today(WordSet::default());
    });
    t.steps(2);
    assert!(!t.app().store.settings.ko_role_colors && t.app().store.settings.ko_colors_faded);
    t.nav("📖  문장 읽기");
    t.wait("lesson", |a| a.lesson.is_some());
    t.nav("▶ 시작하기");
    assert!(!t.has_exact("/"), "no 직독직해 until asked");
    assert!(t.has("나는 소년이다."), "Korean meaning still shown in months 3-4");
    t.click("📖 직독직해");
    assert!(t.has_exact("/"));
    // Month 6 (Day 150): English only; Korean on request or after an attempt.
    let mut t = launch_at(temp_dir("stage2"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today() - chrono::Duration::days(149);
        s.profile.today = no_idioms_today(WordSet::default());
    });
    t.nav("📖  문장 읽기");
    t.wait("lesson", |a| a.lesson.is_some());
    assert!(!t.has("나는 소년이다."), "list shows English only");
    t.nav("▶ 시작하기");
    assert!(!t.has("나는 소년이다.") && t.has("한국어 뜻 보기"));
    t.say("I am a boy.", 1.0);
    assert!(t.has("나는 소년이다."), "Korean shown when checking the answer");
    assert!(t.has("점수 = 알아듣기 쉬운 정도 (원어민 발음과 비교한 점수가 아니에요)"));
}

#[test]
fn ft32_old_30_minute_goal_moves_to_60() {
    let dir = temp_dir("goal60");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.json"), r#"{"daily_goal_minutes": 30}"#).unwrap();
    let t = launch_at(dir.clone(), true, |s| s.profile.placement_done = true);
    assert_eq!(t.app().store.settings.daily_goal_minutes, 60);
    assert!(t.has("0 / 60분") && t.has("6개월 목표: B1"));
}

#[test]
fn ft33_progress_card_shows_each_activity() {
    let t = launch_at(temp_dir("activity"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today() - chrono::Duration::days(5);
        s.profile.today.date = Some(today());
        s.profile.today.done = vec!["words".into(), "writing".into()];
        let day = |n: i64, steps: &[&str]| DayRecord { date: Some(today() - chrono::Duration::days(n)), steps: steps.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        // Days before the program started don't count.
        s.history = vec![day(3, &["words", "speed"]), day(2, &["words"]), day(9, &["words"])];
        // 새 단어 / 문장 읽기 count learned items: 3 words (1 memorized) and 2 idioms.
        for w in ["apple", "river", "window"] {
            crate::srs::add_item(&mut s.items, StudyItem { kind: ItemKind::Word, text: w.into(), meaning_ko: "뜻".into(), category: "vocab".into(), ..Default::default() });
        }
        for t in ["It's my first time", "What's done is done"] {
            crate::srs::add_item(&mut s.items, StudyItem { kind: ItemKind::Sentence, text: t.into(), category: "idiom".into(), ..Default::default() });
        }
        s.word_stats.insert("apple".into(), WordStat { passes: 10, attempts: 10, last: None });
    });
    assert!(t.has("📊 진척율"));
    for (key, n) in [("words", 3), ("speed", 1), ("writing", 1), ("business", 0), ("library", 0)] {
        assert_eq!(t.app().activity_days(key), n, "{key}");
    }
    let words = crate::wordbank::all().len();
    let words_line = format!("{:.1}%  (3/{}단어 · 기억 완료 1) · 오늘 ✓", 300.0 / words as f32, crate::app::thousands(words as u64));
    for text in [words_line.as_str(), "0.2%  (2/1,300문장 · 기억 완료 0)", "0.5%  (1/182일) · 오늘 ✓", "0.5%  (1/182일)", "0.0%  (0/182일)", "📘 리딩 (동화·소설)", "💼 비즈니스 회화"] {
        assert!(t.has(text), "progress card should show {text:?}");
    }
}

#[test]
fn ft34_passed_words_are_said_once_after_a_restart() {
    let dir = temp_dir("restart-words");
    // First run: pass "deadline" 3 times, then quit in the middle of the list.
    {
        let mut t = launch_at(dir.clone(), true, |s| {
            s.profile.placement_done = true;
            s.profile.today = DayProgress { date: Some(today()), word_set: Some(mock_word_set()), ..Default::default() };
        });
        t.nav("📚  새 단어");
        t.nav("▶ 시작하기");
        for _ in 0..3 {
            t.say("deadline", 0.8);
        }
        t.click("다음 ▶");
        assert!(t.app().store.items.iter().any(|i| i.text == "deadline"));
        t.app().store.save_profile();
        t.dir = temp_dir("restart-words-first"); // keep the data folder for the second run
    }
    // Restart: same words today; "deadline" once (no example), then "schedule" as usual.
    let mut t = launch_at(dir, true, |_| {});
    assert_eq!(t.app().store.profile.today.word_set, Some(mock_word_set()), "today's words survive a restart");
    t.nav("📚  새 단어");
    t.nav("▶ 시작하기");
    assert!(t.has("통과한 단어"));
    let reviews = t.app().store.items.iter().find(|i| i.text == "deadline").unwrap().attempts;
    t.say("deadline", 0.8);
    t.click("다음 ▶");
    assert!(t.has("schedule") && !t.has("The deadline is Friday."), "passed word: one pass, no example");
    assert_eq!(t.app().store.items.iter().find(|i| i.text == "deadline").unwrap().attempts, reviews, "not reviewed again today");
    for _ in 0..2 {
        t.say("schedule", 0.8);
    }
    assert!(t.query_next().is_none(), "new words still need 3 passes");
}

#[test]
fn ft35_reading_starts_with_30_idioms_and_words_review_50_learned() {
    let mut t = launch_at(temp_dir("idioms"), true, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = today();
        // 60 words learned on earlier days, all due.
        for n in 0..60 {
            let mut i = StudyItem { kind: ItemKind::Word, text: format!("learned{n}"), meaning_ko: format!("뜻{n}"), category: "vocab".into(), ..Default::default() };
            i.id = crate::srs::make_id(ItemKind::Word, &i.text);
            i.added = today() - chrono::Duration::days(3);
            i.due = today();
            s.items.push(i);
        }
    });
    // 문장 읽기: today's 30 idiom sentences from the built-in 1,300, before the lesson's sentences.
    t.nav("📖  문장 읽기");
    let picked = t.app().store.profile.today.idioms.clone().expect("idioms picked");
    assert_eq!(picked, (1..=30).collect::<Vec<u32>>());
    assert!(t.has("💬 It's my first time") && t.has("이번이 처음이야"));
    t.wait("lesson", |a| a.lesson.is_some());
    t.steps(2);
    assert!(!t.has("I am a boy.") && t.has("1 / 2 쪽"), "25 per page");
    t.click("다음 ▶");
    assert!(t.has("I am a boy."), "the longer sentences follow the idioms");
    t.nav("▶ 시작하기");
    assert!(t.has("It's my first time"));
    t.say("It's my first time", 1.0);
    t.click("다음 ▶");
    let saved = t.app().store.items.iter().find(|i| i.text == "It's my first time").cloned().expect("idiom saved for review");
    assert_eq!(saved.category, "idiom");
    assert!(t.has("What's done is done"), "next idiom after one pass");
    // 새 단어: 35 new words, then 50 learned words to review.
    t.click("그만하기");
    t.nav("📚  새 단어");
    assert_eq!(t.app().store.profile.today.word_set.as_ref().unwrap().words.len(), 35);
    assert_eq!(t.app().word_review_ids().len(), 50);
    assert!(t.has("🔁 배운 단어 복습  0/50"));
    t.click("▶ 배운 단어 50개 복습");
    let first = t.app().word_review_ids()[0].clone();
    t.say("learned0", 0.8);
    t.click("다음 ▶");
    assert_eq!(t.app().store.profile.today.words_reviewed, vec![first]);
    assert_eq!(t.app().word_review_ids().len(), 49, "reviewed words are not offered again today");
}

#[test]
fn ft36_word_list_pages_of_25_coloured_by_practice() {
    let mut t = launch_at(temp_dir("wordlist"), true, |s| {
        s.profile.placement_done = true;
        // 40 words today: two pages. Practice so far: 0, 4 and 12 passes.
        let words: Vec<LessonWord> = (0..40).map(|n| LessonWord { word: format!("word{n:02}"), meaning_ko: format!("뜻{n}"), example: format!("Say word{n:02}."), example_ko: "예문".into() }).collect();
        s.profile.today = DayProgress { date: Some(today()), word_set: Some(WordSet { words, ..Default::default() }), idioms: Some(Vec::new()), ..Default::default() };
        s.word_stats.insert("word01".into(), WordStat { passes: 4, attempts: 5, last: None });
        s.word_stats.insert("word02".into(), WordStat { passes: 12, attempts: 12, last: None });
    });
    t.nav("📚  새 단어");
    for text in ["기억 완료 1/40", "1 / 2 쪽 (40개 중 1~25)", "미완료", "4/10회", "✓ 완료", "word24"] {
        assert!(t.has(text), "word list shows {text:?}");
    }
    assert!(!t.has("word25"), "25 per page");
    t.click("다음 ▶");
    assert!(t.has("word39") && !t.has("word00") && t.has("2 / 2 쪽 (40개 중 26~40)"));
    // Colours: red → orange/green gradient → light green.
    use crate::screens::practice::word_color;
    assert_eq!(word_color(0), egui::Color32::from_rgb(240, 80, 80));
    assert_eq!(word_color(1), egui::Color32::from_rgb(225, 105, 20));
    assert_eq!(word_color(9), egui::Color32::from_rgb(110, 170, 30));
    assert_eq!(word_color(10), egui::Color32::from_rgb(150, 230, 90));
    // Each passing repetition is kept in word_stats.json.
    t.click("◀ 이전");
    t.nav("▶ 시작하기");
    t.say("word00", 0.8);
    t.say("word00", 0.8);
    assert_eq!(t.app().store.word_stat("word00").passes, 2);
    let saved: std::collections::HashMap<String, WordStat> = serde_json::from_str(&std::fs::read_to_string(t.dir.join("word_stats.json")).unwrap()).unwrap();
    assert_eq!(saved["word00"].passes, 2);
    assert_eq!(saved["word00"].last, Some(today()));
}

#[test]
fn ft37_click_a_word_in_the_list_to_practise_it() {
    let mut t = launch("pickword");
    t.nav("📚  새 단어");
    // "schedule" is the 2nd entry: clicking it practises just that word (3 reps + example).
    t.click("schedule");
    assert!(t.has("새 단어 · schedule") && t.has("1 / 2"));
    for _ in 0..3 {
        t.say("schedule", 0.8);
    }
    t.click("다음 ▶");
    t.say("Let me check my schedule.", 1.5);
    t.click("다음 ▶");
    assert!(t.has("잘했어요!") && !t.has("다음 단어 ▶"), "last word in the list: no next");
    assert!(t.app().store.items.iter().any(|i| i.text == "schedule"), "saved for review");
    assert_eq!(t.app().store.word_stat("schedule").passes, 3);
    assert!(!t.app().step_done("words"), "one word doesn't finish today's step");
    // Back to the list; a passed word can be practised again in full and keeps counting.
    t.click("닫기");
    t.click("schedule");
    assert!(t.has("1 / 2"), "full practice again, with the example");
    let reviews = t.app().store.items.iter().find(|i| i.text == "schedule").unwrap().attempts;
    for _ in 0..3 {
        t.say("schedule", 0.8);
    }
    assert_eq!(t.app().store.word_stat("schedule").passes, 6);
    t.click("다음 ▶");
    assert_eq!(t.app().store.items.iter().find(|i| i.text == "schedule").unwrap().attempts, reviews, "not reviewed twice");
    // "다음 단어" goes on from a clicked word to the next one in the list.
    t.click("그만하기");
    t.click("deadline");
    for _ in 0..3 {
        t.say("deadline", 0.8);
    }
    t.click("다음 ▶");
    t.say("The deadline is Friday.", 1.5);
    t.click("다음 ▶");
    t.click("다음 단어 ▶");
    assert!(t.has("새 단어 · schedule") && t.has("1 / 2"), "the next word, in full");
}

#[test]
fn ft38_speed_reading_pages_through_the_learners_own_books() {
    let dir = temp_dir("books");
    std::fs::create_dir_all(dir.join("passages")).unwrap();
    let book = |stage: u32, title: &str, pages: &[&str]| crate::passages::Book { stage, title: title.into(), pages: pages.iter().map(|p| p.to_string()).collect() };
    for (file, b) in [
        ("0.json", book(0, "The Cat", &["Natalie is a movie star."])),
        ("1.json", book(1, "Pocahontas", &["The English came to Virginia.", "John Smith met Pocahontas.", "She was a friend."])),
    ] {
        std::fs::write(dir.join("passages").join(file), serde_json::to_string(&b).unwrap()).unwrap();
    }
    let mut t = launch_at(dir, true, |s| {
        s.profile.placement_done = true;
        s.record_level(3.0, "test"); // A2 → stage 1
    });
    t.nav("⚡  스피드 트레이닝");
    assert!(t.has("책 2권"));
    t.click("▶ 지문 받기");
    assert!(t.has("Pocahontas · 1/3쪽") && t.has("The English came to Virginia."));
    assert!(t.prompts().iter().all(|p| !p.contains("speed")), "no Claude request for a book page");
    t.click("다음 쪽 ▶");
    assert!(t.has("Pocahontas · 2/3쪽") && t.has("John Smith met Pocahontas."));
    assert_eq!(t.app().store.profile.speed_book_pages.get("Pocahontas"), Some(&1), "where the learner left off");
    t.click("◀ 이전 쪽");
    assert!(t.has("1 / 3쪽"));
}

#[test]
fn ft39_reading_list_colours_click_and_home_progress() {
    let mut t = launch_at(temp_dir("readmarks"), true, |s| {
        s.profile.placement_done = true;
        s.word_stats.insert("what's done is done".into(), WordStat { passes: 12, attempts: 12, last: None });
    });
    t.nav("📖  문장 읽기");
    assert_eq!(t.h.get_all_by_label("▶ 시작하기").count(), 2, "start button above and below the list");
    assert!(t.has("기억 완료 1/") && t.has("✓ 완료") && t.has("미완료"), "reading list coloured like 새 단어");
    // Click the first idiom: just that sentence, then it counts on the home progress card.
    t.click("💬 It's my first time");
    assert!(t.has("문장 읽기 · It's my first time"));
    for _ in 0..3 {
        t.say("It's my first time", 1.0);
    }
    t.click("다음 ▶");
    assert_eq!(t.app().store.word_stat("It's my first time").passes, 3);
    assert_eq!(t.app().item_progress("reading").map(|p| p.0), Some(1), "one idiom learned");
    // "다음 문장" goes on to the next sentence in the list.
    let second = t.app_mut().idiom_set()[1].en.clone();
    t.click("다음 문장 ▶");
    assert!(t.has(&format!("문장 읽기 · {}", second.chars().take(30).collect::<String>())), "next sentence: {second}");
    t.click("그만하기");
    t.nav("🏠  홈");
    assert!(t.has("(1/1,300문장 · 기억 완료 0)"));
}

#[test]
fn ft40_business_click_a_scene_or_key_phrase_to_start_talking() {
    let mut t = launch("bizclick");
    t.nav("💼  회화 · 비즈니스");
    t.wait("lesson", |a| a.lesson.is_some());
    // Today's business: key phrases are coloured and clicking one starts the conversation.
    assert!(t.has("핵심 표현 · 클릭하면 바로 대화 시작") && t.has("미완료"));
    t.click("We're on track.");
    assert!(t.app().roleplay.started && t.has("Let's get started. Any updates?"));
    // Using a key phrase while talking counts as practising it.
    t.say("We're on track for Friday", 1.5);
    assert_eq!(t.app().store.word_stat("We're on track.").passes, 1);
    // 상황 모음: a list coloured by finished conversations; clicking a scene starts it at once.
    t.click("🎭 상황 모음");
    assert!(t.has("완료한 상황 0/") && t.has("Job interview"));
    t.click("Ordering at a restaurant");
    assert!(t.app().roleplay.started && t.app().roleplay.scenario.as_ref().is_some_and(|s| s.title.contains("restaurant")));
    // The scene's opening request reaches Claude (background thread: wait for it, as on slow CI).
    let t0 = Instant::now();
    while !t.prompts().iter().any(|p| p.starts_with("Role-play.") && p.contains("a friendly waiter")) {
        assert!(t0.elapsed() < Duration::from_secs(20), "the clicked scene was started");
        t.steps(1);
        std::thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn ft41_settings_shows_the_app_version() {
    let mut t = launch("version");
    t.nav("⚙  설정");
    assert!(t.has(&format!("SpeakUp 버전 {}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn ft42_settings_switches_light_and_dark_theme() {
    let mut t = launch("theme");
    t.nav("⚙  설정");
    assert!(t.has("🎨 테마") && t.has("🌙 다크") && t.has("☀ 라이트"));
    t.click("☀ 라이트");
    let light_ok = t.app().store.settings.light_theme && crate::widgets::is_light() && crate::widgets::bg() == egui::Color32::from_rgb(243, 245, 250);
    let saved: Settings = serde_json::from_str(&std::fs::read_to_string(t.dir.join("settings.json")).unwrap()).unwrap();
    t.click("🌙 다크");
    let dark_ok = !t.app().store.settings.light_theme && !crate::widgets::is_light();
    crate::widgets::set_light(false); // the theme is process-wide: leave other tests in dark
    assert!(light_ok && saved.light_theme && dark_ok);
}

#[test]
fn ft43_diary_is_kept_by_date_and_read_again_from_the_calendar() {
    use chrono::Datelike;
    let past = today() - chrono::Duration::days(40);
    let mut t = launch_at(temp_dir("journal"), true, |s| {
        s.profile.placement_done = true;
        s.writings.push(WritingRecord { date: Some(past), kind: "diary".into(), prompt: "What did you do today?".into(), text: "I went to the zoo.".into(), corrected: "I went to the zoo with my family.".into(), score: 85.0 });
    });
    t.nav("✏  쓰기");
    t.click("📝 영어 일기");
    // Saved by date without Claude; saving again the same day keeps one entry.
    t.app_mut().writing.input = "I played soccer.".into();
    t.steps(2);
    t.click("💾 일기 저장");
    t.app_mut().writing.input = "I played soccer. It was fun.".into();
    t.steps(2);
    t.click("💾 일기 저장");
    assert_eq!(t.app().diary_on(today()).map(|w| w.text.as_str()), Some("I played soccer. It was fun."));
    assert_eq!(t.app().store.writings.iter().filter(|w| w.kind == "diary").count(), 2, "one diary per day");
    let saved = std::fs::read_to_string(t.dir.join("writing.json")).unwrap();
    assert!(saved.contains("It was fun.") && saved.contains(&today().to_string()));
    assert!(t.has("✅ 오늘 일기가 저장되어 있어요"));
    // Coming back to the diary tab brings today's text back.
    t.click("🎧 받아쓰기");
    t.click("📝 영어 일기");
    assert_eq!(t.app().writing.input, "I played soccer. It was fun.");
    // 📅 일기장: today's cell is marked; clicking it shows that day's diary.
    t.click("📅 일기장");
    assert!(t.has("지금까지 쓴 일기 2편"));
    t.click(&format!("{}\n📝", today().day()));
    assert!(t.has(&format!("📝 {}", today().format("%Y년 %m월 %d일"))) && t.has_exact("I played soccer. It was fun."));
    // An older month: go back with ◀ (or open it from 학습 기록) and read it with Claude's correction.
    t.app_mut().open_journal(past);
    t.steps(3);
    assert!(t.has(&format!("{}년 {}월", past.year(), past.month())) && t.has_exact("I went to the zoo.") && t.has_exact("I went to the zoo with my family.") && t.has("85점"));
}

#[test]
fn ft44_a_failed_lesson_request_is_not_repeated_until_retry() {
    let mut t = launch("lessonfail");
    t.app_mut().lesson = None;
    t.app_mut().api_url = "http://127.0.0.1:9/v1/messages".into(); // nothing listens: fails at once
    t.nav("💬  자기 표현");
    // No network (like no credit): straight on with the built-in lesson and offline scripts.
    t.wait("built-in lesson", |a| a.lesson.as_ref().is_some_and(|l| l.date == Some(today())));
    let n = t.mock.requests.lock().unwrap().len();
    for _ in 0..30 {
        t.steps(1);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!t.app().lesson_job.running() && t.mock.requests.lock().unwrap().len() == n, "no new request after the failure");
    assert_eq!(t.app().lesson.as_ref().map(|l| l.source.as_str()), Some("built-in"));
    assert!(t.app().talk_offline() && t.has("📴 오프라인 대본 모드") && t.has("Claude API를 지금 쓸 수 없어서"));
    // 🔄 Claude 다시 연결 tries the API again.
    t.click("🔄 Claude 다시 연결");
    assert!(!t.app().talk_offline());
}

#[test]
fn ft45_offline_scripts_ask_questions_and_hold_a_conversation() {
    let mut t = launch_at(temp_dir("offtalk"), false, |s| s.profile.placement_done = true);
    let level = t.app().store.profile.level;
    // 자기 표현: a built-in question for the level, with useful phrases.
    t.nav("💬  자기 표현");
    let q = crate::offline_talk::question_by_text(&t.app().lesson.as_ref().unwrap().self_expression.question).expect("today's question is a built-in one");
    assert_eq!(crate::offline_talk::band_for(level), q.band);
    assert!(t.has("📴 오프라인 대본 모드") && t.has_exact(&q.question) && t.has_exact(&q.useful_phrases[0].text));
    t.say(&format!("{} {}", q.useful_phrases[0].text.replace("...", "coffee"), q.model_answer), 8.0);
    assert!(t.has("말한 단어") && t.has("모범 답안") && t.has_exact(&q.model_answer));
    // Then it asks the follow-up questions one by one.
    for (i, f) in q.follow_ups.iter().enumerate() {
        assert!(t.has(&format!("💬 이어지는 질문 {}/3", i + 1)) && t.has_exact(&f.question));
        t.say(&f.sample_answer, 3.0);
        assert!(t.has_exact(&f.sample_answer));
        t.click(if i < 2 { "다음 질문 ▶" } else { "완료 ▶" });
    }
    assert!(t.has("✅ 이어지는 질문 3개 완료") && t.app().step_done("speaking"));
    t.click("🔀 다른 질문");
    assert!(!t.has_exact(&q.question), "another question");
    // 회화: today's scene follows its script; the app asks and the conversation goes on.
    t.nav("💼  회화 · 비즈니스");
    let title = t.app().lesson.as_ref().unwrap().business.title.clone();
    let sc = crate::offline_talk::scenario_by_title(&title).expect("today's scene has a script");
    t.click("▶ 대화 시작");
    assert!(t.has_exact(&sc.script[0].ai) && t.has_exact(&sc.script[0].replies[0].text), "opening line + reply hints");
    for k in 0..sc.script.len() - 1 {
        t.say(&sc.script[k].replies[0].text, 2.0);
        assert!(t.has_exact(&sc.script[k + 1].ai), "turn {}", k + 1);
    }
    assert!(t.has("🎉 대화 완료!") && t.app().step_done("business"));
    assert!(t.app().store.corrections.is_empty(), "script hints are not saved as mistakes");
    // 상황 모음: the level's scripted conversations.
    t.click("🎭 상황 모음");
    let scenes = crate::offline_talk::scenarios(level);
    assert!(t.has(&format!("완료한 상황 0/{}", scenes.len())) || t.has(&format!("완료한 상황 1/{}", scenes.len())));
    t.click(&scenes[1].title);
    assert!(t.has_exact(&scenes[1].script[0].ai));
    t.say("Sorry, I don't know.", 1.5);
    assert!(t.has(&scenes[1].script[0].replies.iter().map(|r| r.text.clone()).find(|r| t.has(&format!("✓ 더 자연스럽게: {r}"))).unwrap_or_default()), "a hint for a reply far off");
    assert!(t.prompts().is_empty(), "no API calls");
}

#[test]
fn ft46_words_left_unfinished_on_earlier_days_can_be_seen_and_practised() {
    let yesterday = today() - chrono::Duration::days(1);
    let lw = |w: &str, ko: &str| LessonWord { word: w.into(), meaning_ko: ko.into(), example: format!("I said {w}."), example_ko: String::new() };
    let mut t = launch_at(temp_dir("pastwords"), false, |s| {
        s.profile.placement_done = true;
        s.profile.start_date = yesterday;
        s.word_days.insert(yesterday, WordSet { words: vec![lw("company", "회사"), lw("health", "건강"), lw("reason", "이유")], ..Default::default() });
        // "reason" was passed yesterday.
        s.items.push(StudyItem { id: crate::srs::make_id(ItemKind::Word, "reason"), kind: ItemKind::Word, text: "reason".into(), ..Default::default() });
    });
    t.nav("📚  새 단어");
    assert!(t.has("📅 지난 날 못 끝낸 새 단어 2개") && t.has("1일차 ·") && t.has_exact("company") && t.has_exact("health"));
    // Today's list is new words only (not yesterday's), and is kept by date too.
    let today_words: Vec<String> = t.app().store.profile.today.word_set.as_ref().unwrap().words.iter().map(|w| w.word.clone()).collect();
    assert!(!today_words.iter().any(|w| w == "company" || w == "health"), "{today_words:?}");
    let saved = std::fs::read_to_string(t.dir.join("word_days.json")).unwrap();
    assert!(saved.contains(&today().to_string()) && saved.contains(&yesterday.to_string()));
    // Click one: practise it, then go on to the next unfinished word.
    t.click("company");
    assert!(t.has("새 단어 · company"));
    for _ in 0..3 {
        t.say("company", 0.8);
    }
    t.click("다음 ▶");
    t.say("I said company.", 1.5);
    t.click("다음 ▶");
    t.click("다음 단어 ▶");
    assert!(t.has("새 단어 · health"));
    t.click("그만하기");
    assert!(t.has("📅 지난 날 못 끝낸 새 단어 1개"), "company is done now");
}

#[test]
fn ft47_an_easy_word_can_be_marked_learned() {
    let mut t = launch_at(temp_dir("easyword"), false, |s| s.profile.placement_done = true);
    t.nav("📚  새 단어");
    let ws = t.app().store.profile.today.word_set.clone().unwrap();
    // The list starts with the chunks, then the words: mark the first entry learned.
    let first = ws.phrases.first().or(ws.words.first()).unwrap().word.clone();
    t.nav("✓ 학습완료");
    let item = t.app().store.items.iter().find(|i| i.text == first).cloned().expect("saved as learned");
    assert!(item.known && item.due > today() + chrono::Duration::days(3000), "never comes up for review");
    assert!(t.app().store.word_stat(&first).passes >= WORD_DONE_PASSES);
    assert!(t.has("기억 완료 1/"), "counted as done in the list");
    // The day's run skips it.
    t.nav("▶ 시작하기");
    assert!(!t.app().queue.as_ref().unwrap().specs.iter().any(|s| s.target == first));
}
