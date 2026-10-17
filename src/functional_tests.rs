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

fn respond(body: &Value) -> Value {
    let prompt = body["messages"][0]["content"].as_str().unwrap_or("").to_string();
    if prompt.starts_with("Create today's lesson") {
        lesson_json()
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
    })
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
    for text in ["Day 1 / 182", "1주차", "자기소개 · Introducing yourself", "📚 새 단어", "⚡ 스피드 리딩·말하기", "🎧 리스닝 · 구간반복", "초중급", "0 / 30분", "▶  오늘의 학습 시작", "Claude API 키가 없어"] {
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
    let req = t.mock.requests.lock().unwrap()[0].clone();
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
    t.click("▶ 시작하기");
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
    t.click("▶ 시작하기");
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
    // Recall: Korean shown, English hidden.
    assert!(t.has("영어로 말해 보세요") && t.has("일정"));
    assert!(!t.has_exact("schedule"));
    t.click("💡 힌트");
    assert!(t.has("s_______"));
    t.say("I think it's schedule", 1.0); // said the word → recall success
    assert!(t.has("100점"));
    t.say("schedule", 0.8);
    t.click("다음 ▶");
    t.say("Nice to meet you.", 1.0);
    t.say("Nice to meet you.", 1.0);
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
    assert!(t.has("정확도 100%"));
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
    assert_eq!(l.words.len(), 8, "phase 1 = 8 words");
    assert!(t.has("introduce"));
    t.nav("💼  회화 · 비즈니스");
    assert!(t.has("Claude API 키가 필요합니다"));
    assert!(t.prompts().is_empty(), "no API calls without a key");
}

#[test]
fn ft17_history_records_each_day_and_screen_shows_it() {
    let mut t = launch("history");
    t.nav("📚  새 단어");
    t.wait("lesson", |a| a.lesson.is_some());
    t.click("▶ 시작하기");
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
    assert!(t.has("100점"));
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
    // Total time against the whole program (182 days × 30 min = 5,460 min).
    for text in ["2시간 5분", "6개월 목표 5,460분", "2.3% 완료", "초중급 A2", "레벨 3.0 / 10", "0 / 1,000", "배운 단어 / A2 필수"] {
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
