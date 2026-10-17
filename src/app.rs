//! Application state, navigation, dashboard, onboarding/level test and settings.
//! The practice screens live in `screens/`.

use crate::claude::{Claude, LessonRequest};
use crate::content;
use crate::curriculum::{self, TOTAL_DAYS};
use crate::language::{self, LanguagePack};
use crate::models::*;
use crate::screens::{audio_check::AudioCheckState, feedback::FeedbackState, speed::SpeedState, listening::ListeningState, practice::DrillQueue, speaking::{RoleplayState, SelfExprState}, homework::HomeworkState, test::LevelTest};
use crate::srs;
use crate::storage::Store;
use crate::stt::{self, Stt, SttStatus};
use crate::voice::{Speaker, Voice};
use crate::widgets::*;
use chrono::Duration as CDuration;
use egui::{Color32, RichText, Vec2};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Onboarding,
    Home,
    Homework,
    Review,
    Words,
    Reading,
    Speed,
    Listening,
    SelfExpression,
    Business,
    Vocabulary,
    Feedback,
    History,
    Progress,
    LevelTest,
    Settings,
}

/// The daily routine, in order. Keys are stored in `profile.today.done`.
pub const STEPS: &[(&str, &str, Screen)] = &[
    ("homework", "📝 숙제 확인", Screen::Homework),
    ("review", "🔁 복습 (배운 단어·문장)", Screen::Review),
    ("words", "📚 새 단어", Screen::Words),
    ("reading", "📖 문장 소리내어 읽기", Screen::Reading),
    ("speed", "⚡ 스피드 리딩·말하기", Screen::Speed),
    ("listening", "🎧 리스닝 · 구간반복", Screen::Listening),
    ("speaking", "💬 자기 표현 말하기", Screen::SelfExpression),
    ("business", "💼 비즈니스 회화", Screen::Business),
];

pub struct App {
    pub store: Store,
    pub lang: &'static LanguagePack,
    pub voice: Voice,
    pub screen: Screen,
    pub lesson: Option<Lesson>,
    pub lesson_job: Pending<Lesson>,
    pub lesson_error: Option<String>,
    pub queue: Option<DrillQueue>,
    pub homework: HomeworkState,
    pub selfx: SelfExprState,
    pub roleplay: RoleplayState,
    pub listening: ListeningState,
    pub speed: SpeedState,
    pub feedback: FeedbackState,
    pub audio_check: AudioCheckState,
    pub test: Option<LevelTest>,
    pub toast: Option<(String, Instant)>,
    pub vocab_filter: String,
    pub vocab_mode: usize,
    pub show_key: bool,
    edit_name: bool,
    edit_key: bool,
    pub ytdlp_job: Pending<std::path::PathBuf>,
    pub api_url: String,
    last_activity: Instant,
    last_tick: Instant,
    last_save: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let store = Store::open().expect("데이터 폴더를 만들 수 없습니다");
        let lang = language::by_code(&store.settings.target_language);
        let speaker = Speaker::new(lang, &store.settings.tts_voice_id);
        let stt = Stt::new(lang.code);
        let mut store = store;
        let mut model_file = stt::model_path(&store.models_dir(), &store.settings.whisper_model);
        if !model_file.exists() {
            // A model downloaded earlier is reused even if a different one is selected.
            if let Some((id, path)) = lang.whisper_models.iter().map(|(id, _)| (id, stt::model_path(&store.models_dir(), id))).find(|(_, p)| p.exists()) {
                store.settings.whisper_model = id.to_string();
                store.save_settings();
                model_file = path;
            }
        }
        if model_file.exists() {
            stt.load(model_file);
        }
        let voice = Voice {
            speaker,
            recorder: crate::audio::Recorder::new(store.settings.input_device.clone()),
            player: crate::audio::Player::new(store.settings.output_device.clone()),
            stt,
            base_rate: store.settings.tts_rate,
            last_recording: Vec::new(),
        };
        Self::new_with(cc, store, voice)
    }

    /// Builds the app from an opened store and a voice bundle (tests pass simulated audio).
    pub fn new_with(cc: &eframe::CreationContext<'_>, store: Store, voice: Voice) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        let lang = language::by_code(&store.settings.target_language);
        let screen = if store.profile.placement_done { Screen::Home } else { Screen::Onboarding };
        let mut app = Self {
            lesson: store.load_lesson(today()),
            store,
            lang,
            voice,
            screen,
            lesson_job: Pending::default(),
            lesson_error: None,
            queue: None,
            homework: HomeworkState::default(),
            selfx: SelfExprState::default(),
            roleplay: RoleplayState::default(),
            listening: ListeningState::default(),
            speed: SpeedState::default(),
            feedback: FeedbackState::default(),
            audio_check: AudioCheckState::default(),
            test: None,
            toast: None,
            vocab_filter: String::new(),
            vocab_mode: 0,
            show_key: false,
            edit_name: false,
            edit_key: false,
            ytdlp_job: Pending::default(),
            api_url: crate::claude::API_URL.to_string(),
            last_activity: Instant::now(),
            last_tick: Instant::now(),
            last_save: Instant::now(),
        };
        app.roll_day();
        app.assign_homework();
        // Ask for the microphone up front so the first speaking drill doesn't record silence.
        #[cfg(not(test))]
        app.audio_check.request_if_needed();
        app
    }

    pub fn claude(&self) -> Option<Claude> {
        self.store.settings.effective_api_key().map(|k| Claude { url: self.api_url.clone(), api_key: k, model: self.store.settings.model.clone(), lang: self.lang })
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn pass_score(&self) -> f32 {
        self.store.settings.pass_score
    }

    // ------------------------------------------------------------ daily bookkeeping

    /// Starts a fresh day's progress record when the date changes.
    fn roll_day(&mut self) {
        let t = today();
        if self.store.profile.today.date != Some(t) {
            self.update_history(); // close out the previous day
            self.store.profile.today = DayProgress { date: Some(t), ..Default::default() };
            self.store.save_profile();
            if self.lesson.as_ref().and_then(|l| l.date) != Some(t) {
                self.lesson = self.store.load_lesson(t);
            }
        }
    }

    /// Called whenever the learner finishes a step: marks it done and maintains the streak.
    pub fn complete_step(&mut self, key: &str) {
        let t = today();
        let p = &mut self.store.profile;
        if p.last_study_date != Some(t) {
            p.streak = if p.last_study_date == Some(t - CDuration::days(1)) { p.streak + 1 } else { 1 };
            p.last_study_date = Some(t);
            p.days_studied += 1;
        }
        if !p.today.done.iter().any(|d| d == key) {
            p.today.done.push(key.to_string());
            let label = STEPS.iter().find(|s| s.0 == key).map(|s| s.1).unwrap_or(key);
            self.toast(format!("✅ 완료: {label}"));
        }
        self.store.save_profile();
        self.update_history();
    }

    /// Writes the current day's record to history.json (called whenever progress changes).
    pub fn update_history(&mut self) {
        let p = &self.store.profile;
        let Some(date) = p.today.date else { return };
        let lesson = self.lesson.as_ref().filter(|l| l.date == Some(date));
        let added: Vec<&StudyItem> = self.store.items.iter().filter(|i| i.added == date).collect();
        let scores = &p.today.scores;
        let rec = DayRecord {
            date: Some(date),
            program_day: ((date - p.start_date).num_days() + 1).max(1),
            steps: p.today.done.clone(),
            minutes: (p.today.study_seconds / 60) as u32,
            attempts: scores.len() as u32,
            avg_score: if scores.is_empty() { 0.0 } else { scores.iter().sum::<f32>() / scores.len() as f32 },
            level: p.level,
            theme: curriculum::week(curriculum::week_number(p)).theme_ko.to_string(),
            new_words: added.iter().filter(|i| i.kind == ItemKind::Word).map(|i| i.text.clone()).collect(),
            new_sentences: added.iter().filter(|i| i.kind == ItemKind::Sentence).count() as u32,
            reviewed: self.store.items.iter().filter(|i| i.added < date && i.attempts > 0 && i.due > date).count() as u32,
            question: lesson.map(|l| l.self_expression.question.clone()).unwrap_or_default(),
            scenario: lesson.map(|l| l.business.title.clone()).unwrap_or_default(),
            best_wpm: p.speed_history.iter().filter(|r| r.date == date && r.kind == "reading").map(|r| r.wpm).fold(0.0, f32::max),
        };
        if rec.attempts > 0 || !rec.steps.is_empty() || rec.minutes > 0 {
            self.store.upsert_day(rec);
        }
    }

    pub fn record_score(&mut self, category: &str, target: &str, transcript: &str, score: f32) {
        self.store.profile.today.scores.push(score);
        self.store.log_attempt(category, target, transcript, score);
    }

    /// Records a drill attempt: score, log line, and any words that weren't recognized.
    pub fn record_attempt(&mut self, category: &str, target: &str, a: &Attempt) {
        self.record_score(category, target, &a.transcript, a.score);
        // Only count mispronunciations on attempts that were mostly right; a mostly-wrong
        // attempt usually means the learner said something else entirely.
        if a.score >= 40.0 {
            self.store.add_mistakes(&a.missed);
        }
    }

    /// Steps that apply today (homework only if there is homework from an earlier day,
    /// review only if something is due).
    pub fn todays_steps(&self) -> Vec<(&'static str, &'static str, Screen)> {
        let has_homework = self.store.homework.iter().any(|h| h.completed.is_none() && h.assigned < today());
        let done = &self.store.profile.today.done;
        let has_review = srs::due_count(&self.store.items) > 0 || done.iter().any(|d| d == "review");
        STEPS
            .iter()
            .copied()
            .filter(|(k, _, _)| match *k {
                "homework" => has_homework || done.iter().any(|d| d == "homework"),
                "review" => has_review,
                _ => true,
            })
            .collect()
    }

    pub fn step_done(&self, key: &str) -> bool {
        self.store.profile.today.done.iter().any(|d| d == key)
    }

    /// Creates/extends homework from weak items (things the learner keeps getting wrong).
    pub fn assign_homework(&mut self) {
        let in_open: Vec<String> = self
            .store
            .homework
            .iter()
            .filter(|h| h.completed.is_none())
            .flat_map(|h| h.tasks.iter().map(|t| t.item_id.clone()))
            .collect();
        let weak: Vec<StudyItem> = self.store.items.iter().filter(|i| i.weak && !in_open.contains(&i.id)).take(6).cloned().collect();
        if weak.is_empty() {
            return;
        }
        let has_claude = self.claude().is_some();
        let reps = self.store.settings.reps_weak;
        let mut tasks = Vec::new();
        for i in &weak {
            match i.kind {
                ItemKind::Sentence => tasks.push(HomeworkTask {
                    kind: TaskKind::ReadAloud,
                    item_id: i.id.clone(),
                    text: i.text.clone(),
                    meaning_ko: i.meaning_ko.clone(),
                    instruction_ko: format!("소리 내어 {reps}번 정확하게 읽기"),
                    reps,
                    ..Default::default()
                }),
                ItemKind::Word => {
                    let practice = if i.example.is_empty() { i.text.clone() } else { i.example.clone() };
                    tasks.push(HomeworkTask {
                        kind: TaskKind::ReadAloud,
                        item_id: i.id.clone(),
                        text: practice,
                        meaning_ko: if i.example_ko.is_empty() { i.meaning_ko.clone() } else { i.example_ko.clone() },
                        instruction_ko: format!("'{}' 예문을 {reps}번 읽기", i.text),
                        reps,
                        ..Default::default()
                    });
                    if has_claude {
                        tasks.push(HomeworkTask {
                            kind: TaskKind::UseWord,
                            item_id: i.id.clone(),
                            text: i.text.clone(),
                            meaning_ko: i.meaning_ko.clone(),
                            instruction_ko: format!("'{}'({})를 넣어 나만의 문장 말하기", i.text, i.meaning_ko),
                            reps: 1,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if let Some(open) = self.store.homework.iter_mut().find(|h| h.completed.is_none() && h.assigned == today()) {
            open.tasks.extend(tasks);
        } else {
            self.store.homework.push(Homework { id: format!("hw-{}", today()), assigned: today(), tasks, completed: None });
        }
        self.store.save_homework();
    }

    /// End-of-day: assigns homework and nudges the level by today's average score.
    pub fn wrap_up(&mut self) {
        self.assign_homework();
        let scores = self.store.profile.today.scores.clone();
        if scores.len() >= 10 && !self.store.profile.today.wrapped_up {
            let avg = scores.iter().sum::<f32>() / scores.len() as f32;
            let delta = ((avg - 75.0) / 25.0 * 0.05).clamp(-0.05, 0.05);
            if delta.abs() >= 0.01 {
                let lvl = self.store.profile.level + delta;
                self.store.record_level(lvl, &format!("일일 학습 평균 {avg:.0}점 ({}회 발화)", scores.len()));
            }
        }
        self.store.profile.today.wrapped_up = true;
        self.store.save_profile();
        self.update_history();
        self.toast("오늘 학습을 마무리했어요. 숙제를 확인하세요!");
    }

    // ------------------------------------------------------------ today's lesson

    pub fn ensure_lesson(&mut self) {
        if self.lesson.as_ref().and_then(|l| l.date) == Some(today()) || self.lesson_job.running() {
            return;
        }
        let p = &self.store.profile;
        let week = curriculum::week_number(p);
        let day = curriculum::day_number(p);
        let phase = curriculum::phase_of_week(week);
        let n_words = if self.store.settings.new_words_per_day > 0 { self.store.settings.new_words_per_day } else { phase.new_words };
        match self.claude() {
            Some(c) => {
                let wk = curriculum::week(week);
                let past = self.store.past_lessons(60);
                let mut avoid_sentences: Vec<String> =
                    self.store.items.iter().rev().filter(|i| i.kind == ItemKind::Sentence).take(80).map(|i| i.text.clone()).collect();
                avoid_sentences.extend(past.iter().flat_map(|l| l.sentences.iter().map(|s| s.text.clone())).take(40));
                avoid_sentences.dedup();
                let req = LessonRequest {
                    day,
                    week,
                    phase: phase.name_ko.to_string(),
                    theme: wk.theme.to_string(),
                    business: wk.business.to_string(),
                    grammar: wk.grammar.to_string(),
                    level: p.level,
                    known: self.store.items.iter().filter(|i| i.kind == ItemKind::Word).map(|i| i.text.clone()).collect(),
                    weak: self.store.items.iter().filter(|i| i.weak).take(8).map(|i| i.text.clone()).collect(),
                    n_words,
                    n_sentences: phase.sentences,
                    avoid_sentences,
                    avoid_questions: past.iter().map(|l| l.self_expression.question.clone()).filter(|q| !q.is_empty()).collect(),
                    avoid_scenarios: past.iter().map(|l| l.business.title.clone()).filter(|q| !q.is_empty()).collect(),
                };
                self.lesson_error = None;
                self.lesson_job.spawn(move || c.generate_lesson(&req));
            }
            None => self.use_builtin_lesson(),
        }
    }

    pub fn use_builtin_lesson(&mut self) {
        let p = &self.store.profile;
        let week = curriculum::week_number(p);
        let phase = curriculum::phase_of_week(week);
        let n_words = if self.store.settings.new_words_per_day > 0 { self.store.settings.new_words_per_day } else { phase.new_words };
        let past = self.store.past_lessons(400);
        let l = content::builtin_lesson(week, curriculum::day_number(p), &self.store.items, &past, n_words as usize, phase.sentences as usize);
        self.store.save_lesson(&l);
        self.lesson = Some(l);
    }

    fn poll_jobs(&mut self) {
        if let Some(r) = self.lesson_job.poll() {
            match r {
                Ok(l) => {
                    for s in &l.sentences {
                        self.store.put_gloss(&s.text, &s.chunks);
                    }
                    self.store.save_lesson(&l);
                    self.lesson = Some(l);
                }
                Err(e) => self.lesson_error = Some(format!("{e:#}")),
            }
        }
        if let Some(r) = self.ytdlp_job.poll() {
            match r {
                Ok(p) => {
                    self.store.settings.ytdlp_path = p.to_string_lossy().to_string();
                    self.store.save_settings();
                    self.toast("yt-dlp 설치 완료");
                }
                Err(e) => self.toast(format!("yt-dlp 설치 실패: {e:#}")),
            }
        }
    }

    /// Shows a spinner / error while the lesson is being prepared. Returns true when ready.
    pub fn lesson_ready_ui(&mut self, ui: &mut egui::Ui) -> bool {
        self.ensure_lesson();
        if self.lesson.as_ref().and_then(|l| l.date) == Some(today()) {
            return true;
        }
        card(ui, |ui| {
            if self.lesson_job.running() {
                spinner_label(ui, "Claude가 오늘의 레슨을 만들고 있어요… (10~40초)");
            }
            if let Some(e) = self.lesson_error.clone() {
                error_label(ui, &e);
                ui.horizontal(|ui| {
                    if ui.button("다시 시도").clicked() {
                        self.lesson_error = None;
                        self.ensure_lesson();
                    }
                    if ui.button("내장 레슨으로 진행").clicked() {
                        self.lesson_error = None;
                        self.use_builtin_lesson();
                    }
                });
            }
        });
        false
    }

    // ------------------------------------------------------------ navigation

    pub fn go(&mut self, s: Screen) {
        if self.voice.recorder.is_recording() {
            self.voice.recorder.stop();
        }
        self.voice.player.stop();
        self.voice.speaker.stop();
        if s != self.screen {
            self.queue = None;
        }
        self.screen = s;
    }

    fn nav_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(44.0), egui::Sense::hover());
            crate::art::gradient_rect(ui.painter(), r, 13.0, ACCENT, crate::art::VIOLET);
            let t = ui.input(|i| i.time) as f32;
            crate::art::mascot(ui.painter(), r.center() - Vec2::new(0.0, 2.0), 28.0, crate::art::Mood::Happy, t);
            ui.vertical(|ui| {
                ui.label(RichText::new("SpeakUp 6M").size(19.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(format!("{} 듣기·말하기 6개월", self.lang.name_ko)).size(12.5).color(MUTED));
            });
        });
        ui.add_space(16.0);
        let groups: &[(&str, &[(Screen, &str)])] = &[
            ("오늘의 학습", &[(Screen::Home, "🏠  홈"), (Screen::Homework, "📝  숙제"), (Screen::Review, "🔁  복습")]),
            (
                "연습",
                &[
                    (Screen::Words, "📚  새 단어"),
                    (Screen::Reading, "📖  문장 읽기"),
                    (Screen::Speed, "⚡  스피드 트레이닝"),
                    (Screen::Listening, "🎧  리스닝 랩"),
                    (Screen::SelfExpression, "💬  자기 표현"),
                    (Screen::Business, "💼  회화 · 비즈니스"),
                ],
            ),
            ("내 학습", &[(Screen::Feedback, "🔬  피드백 센터"), (Screen::Vocabulary, "📒  내 단어장"), (Screen::History, "📅  학습 기록"), (Screen::Progress, "📈  레벨 · 평가"), (Screen::Settings, "⚙  설정")]),
        ];
        for (group, items) in groups {
            ui.label(RichText::new(*group).size(12.0).color(MUTED).strong());
            ui.add_space(2.0);
            for (s, label) in *items {
                let selected = self.screen == *s;
                let text = RichText::new(*label).size(15.5).color(if selected { Color32::WHITE } else { Color32::from_rgb(200, 208, 224) });
                let bg = ui.painter().add(egui::Shape::Noop);
                let btn = egui::Button::new(if selected { text.strong() } else { text })
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(10.0)
                    .min_size(Vec2::new(ui.available_width(), 36.0));
                let resp = ui.add(btn);
                if selected {
                    let r = resp.rect;
                    ui.painter().set(bg, crate::art::gradient_rect_shape(r, 10.0, ACCENT.gamma_multiply(0.75), crate::art::VIOLET.gamma_multiply(0.45)));
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min + Vec2::new(-14.0, 8.0), Vec2::new(4.0, r.height() - 16.0)), 2.0, crate::art::CYAN);
                } else if resp.hovered() {
                    ui.painter().set(bg, egui::Shape::rect_filled(resp.rect, 10.0, Color32::from_white_alpha(10)));
                }
                if resp.clicked() {
                    self.go(*s);
                }
            }
            ui.add_space(12.0);
        }
        // Status lines at the bottom, or right after the menu when the window is short.
        let status_h = 52.0;
        if ui.available_height() > status_h {
            ui.add_space(ui.available_height() - status_h);
        }
        ui.vertical(|ui| {
            let (txt, col) = match self.voice.stt.status() {
                SttStatus::Ready => ("● 음성 인식 준비됨", GOOD),
                SttStatus::Downloading => ("● 음성 인식 모델 받는 중", WARN),
                SttStatus::Loading => ("● 음성 인식 로딩 중", WARN),
                SttStatus::Missing => ("● 음성 인식 모델 없음", BAD),
                SttStatus::Error(_) => ("● 음성 인식 오류", BAD),
            };
            ui.label(RichText::new(txt).color(col).size(12.5));
            let api = if self.claude().is_some() { ("● Claude 연결됨", GOOD) } else { ("● Claude API 키 없음", BAD) };
            ui.label(RichText::new(api.0).color(api.1).size(12.5));
        });
    }

    // ------------------------------------------------------------ home / dashboard

    fn home_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.store.profile.clone();
        let day = curriculum::day_number(&p);
        let week = curriculum::week_number(&p);
        let phase = curriculum::phase_of_week(week);
        let wk = curriculum::week(week);
        let steps = self.todays_steps();
        let done = steps.iter().filter(|s| self.step_done(s.0)).count();

        // Hero: greeting, program progress, daily goal ring — over a gradient with the
        // six-month journey drawn in the background.
        let hero_bg = ui.painter().add(egui::Shape::Noop);
        let hero = egui::Frame::new()
            .corner_radius(22.0)
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 150.0).max(200.0));
                        let hello = if p.name.is_empty() { "오늘도 영어로 말해 볼까요?".to_string() } else { format!("{}님, 오늘도 영어로 말해 볼까요?", p.name) };
                        ui.label(RichText::new(hello).size(26.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new(format!("Day {day} / {TOTAL_DAYS}  ·  {week}주차  ·  {}", phase.name_ko)).size(15.0).color(Color32::from_rgb(200, 214, 255)));
                        ui.add_space(8.0);
                        ui.add(egui::ProgressBar::new(day as f32 / TOTAL_DAYS as f32).desired_height(10.0).fill(Color32::from_rgb(120, 170, 255)).corner_radius(5.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("🎯 {}", phase.goal_ko)).color(Color32::from_rgb(200, 214, 255)));
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let first_open = steps.iter().find(|s| !self.step_done(s.0)).map(|s| s.2);
                            if let Some(s) = first_open {
                                if primary_button(ui, if done == 0 { "▶  오늘의 학습 시작" } else { "▶  오늘의 학습 이어하기" }).clicked() {
                                    self.go(s);
                                }
                            } else {
                                chip(ui, "🎉 오늘 학습 완료!", GOOD);
                            }
                            if !self.store.profile.today.wrapped_up && done > 0 && ui.add(egui::Button::new(RichText::new("🌙 오늘 학습 마무리").size(15.0)).min_size(Vec2::new(0.0, 46.0)).corner_radius(12.0)).clicked() {
                                self.wrap_up();
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        let all_done = done == steps.len();
                        crate::art::mascot_widget(ui, 54.0, if all_done { crate::art::Mood::Cheer } else { crate::art::Mood::Talking });
                        ring(ui, done as f32 / steps.len().max(1) as f32, &format!("{done}/{}", steps.len()), 120.0);
                    });
                });
            });
        {
            let r = hero.response.rect;
            let art = egui::Rect::from_min_max(egui::pos2(r.left() + r.width() * 0.40, r.top() + r.height() * 0.42), egui::pos2(r.right() - 190.0, r.bottom()));
            let bg = crate::art::gradient_rect_shape(r, 22.0, Color32::from_rgb(40, 70, 190), Color32::from_rgb(98, 52, 170));
            ui.painter().set(hero_bg, egui::Shape::Vec(vec![bg, crate::art::journey(art, day as f32 / TOTAL_DAYS as f32)]));
        }
        if curriculum::program_finished(&p) {
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.label(RichText::new("🎉 6개월 과정을 모두 마쳤어요! 최종 레벨 평가를 보고 복습을 계속하세요.").size(17.0));
            });
        }
        ui.add_space(12.0);

        // Stat tiles
        let goal = self.store.settings.daily_goal_minutes.max(1) as u64;
        let mins = p.today.study_seconds / 60;
        ui.horizontal_wrapped(|ui| {
            crate::screens::history::stat_tile(ui, &format!("🔥 {}일", p.streak), "연속 학습", WARN);
            crate::screens::history::stat_tile(ui, &format!("{mins} / {goal}분"), "오늘 학습 시간", if mins >= goal { GOOD } else { Color32::WHITE });
            crate::screens::history::stat_tile(ui, &format!("{} {}", level_name_of(p.level), cefr_of(p.level)), &format!("레벨 {:.1} / 10", p.level), ACCENT);
            crate::screens::history::stat_tile(ui, &self.store.items.len().to_string(), "배운 단어·문장", Color32::from_rgb(190, 140, 255));
        });
        ui.add_space(12.0);

        // Today's routine
        let mut next: Option<Screen> = None;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("오늘의 루틴").size(19.0).strong());
                chip(ui, &format!("{} · {}", wk.theme_ko, wk.theme), ACCENT);
            });
            ui.label(RichText::new(format!("비즈니스: {}  ·  문법: {}", wk.business, wk.grammar)).color(MUTED));
            ui.add_space(8.0);
            for (key, label, screen) in &steps {
                let is_done = self.step_done(key);
                let extra = match *key {
                    "review" => format!("{}개", srs::due_count(&self.store.items)),
                    "listening" => format!("쉐도잉 {}/5", self.store.profile.today.listening_passes.min(5)),
                    "speed" => format!("{}/3 라운드", self.store.profile.today.speed_rounds.min(3)),
                    _ => String::new(),
                };
                let row = egui::Frame::new()
                    .fill(if is_done { GOOD.gamma_multiply(0.08) } else { Color32::from_rgb(27, 34, 52) })
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(*label).size(16.5).color(if is_done { MUTED } else { Color32::WHITE }));
                            if !extra.is_empty() {
                                ui.label(RichText::new(extra).color(MUTED));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if is_done {
                                    chip(ui, "✓ 완료", GOOD);
                                } else {
                                    chip(ui, "시작 ›", ACCENT);
                                }
                            });
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if row.clicked() {
                    next = Some(*screen);
                }
                ui.add_space(4.0);
            }
        });
        if let Some(s) = next {
            self.go(s);
        }

        let open_hw: usize = self.store.homework.iter().filter(|h| h.completed.is_none()).map(|h| h.progress().1 - h.progress().0).sum();
        let weak = self.store.items.iter().filter(|i| i.weak).count();
        let mistakes = self.store.mistakes.iter().filter(|m| m.fixed_streak < 3).count();
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            chip(ui, &format!("약점 {weak}개"), if weak > 0 { WARN } else { MUTED });
            chip(ui, &format!("남은 숙제 {open_hw}개"), if open_hw > 0 { WARN } else { MUTED });
            if let Some(w) = curriculum::checkpoint_due(&p) {
                if ui.button(format!("📋 {w}주차 레벨 평가 보기")).clicked() {
                    self.test = Some(LevelTest::new(false, w));
                    self.go(Screen::LevelTest);
                }
            } else if let Some(w) = curriculum::next_checkpoint(&p) {
                chip(ui, &format!("다음 레벨 평가 {w}주차"), MUTED);
            }
            if mistakes > 0 && ui.link(RichText::new(format!("발음 실수 {mistakes}개 연습하기 ›")).color(WARN)).clicked() {
                self.go(Screen::Feedback);
            }
        });
        if self.claude().is_none() {
            ui.add_space(6.0);
            ui.label(RichText::new("⚠ Claude API 키가 없어 내장 콘텐츠로 학습 중입니다. AI 피드백·회화·레슨 생성을 쓰려면 설정에서 키를 입력하세요.").color(WARN));
        }
    }

    // ------------------------------------------------------------ onboarding

    fn onboarding_ui(&mut self, ui: &mut egui::Ui) {
        crate::art::onboarding_hero(ui);
        ui.add_space(16.0);
        // Anything entered here is saved immediately and never asked again on the next launch.
        let has_name = !self.store.profile.name.trim().is_empty();
        let has_key = self.claude().is_some();
        card(ui, |ui| {
            ui.label(RichText::new("1. 이름 (선택)").strong());
            if has_name && !self.edit_name {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("✅ {}", self.store.profile.name)).color(GOOD));
                    if ui.small_button("변경").clicked() {
                        self.edit_name = true;
                    }
                });
            } else if ui.text_edit_singleline(&mut self.store.profile.name).changed() {
                self.store.save_profile();
            }
            ui.add_space(8.0);
            ui.label(RichText::new("2. Claude API 키").strong());
            if has_key && !self.edit_key {
                ui.horizontal(|ui| {
                    let from_env = self.store.settings.api_key.trim().is_empty();
                    ui.label(RichText::new(if from_env { "✅ 환경변수 ANTHROPIC_API_KEY 사용" } else { "✅ 저장됨" }).color(GOOD));
                    if ui.small_button("변경").clicked() {
                        self.edit_key = true;
                    }
                });
            } else {
                ui.label(RichText::new("레슨 생성·말하기 피드백·AI 회화 상대에 사용됩니다. 환경변수 ANTHROPIC_API_KEY 도 사용할 수 있어요.").color(MUTED));
                if ui.add(egui::TextEdit::singleline(&mut self.store.settings.api_key).password(true).hint_text("sk-ant-...")).changed() {
                    self.store.save_settings();
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new("3. 음성 인식 모델 (오프라인, 최초 1회 다운로드)").strong());
            self.stt_setup_ui(ui);
            ui.add_space(8.0);
            ui.label(RichText::new("4. 마이크·스피커 점검").strong());
            if self.audio_check_ui(ui) {
                self.store.save_settings();
            }
        });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            let ready = self.voice.stt.is_ready();
            if !ready {
                ui.label(RichText::new("음성 인식 모델이 준비되면 레벨 테스트를 시작할 수 있어요.").color(MUTED));
            }
            if ui.add_enabled(ready, egui::Button::new(RichText::new("레벨 테스트 시작 ▶").size(18.0)).fill(ACCENT).min_size(Vec2::new(240.0, 46.0))).clicked() {
                self.store.save_settings();
                self.store.save_profile();
                self.test = Some(LevelTest::new(true, 0));
                self.screen = Screen::LevelTest;
            }
            if ui.small_button("테스트 없이 초급(A2)으로 시작").clicked() {
                self.store.save_settings();
                self.store.profile.placement_done = true;
                self.store.profile.start_date = today();
                self.store.record_level(2.5, "시작 (레벨 테스트 생략)");
                self.screen = Screen::Home;
            }
        });
    }

    pub fn stt_setup_ui(&mut self, ui: &mut egui::Ui) {
        let models = self.lang.whisper_models;
        ui.horizontal(|ui| {
            for (id, label) in models {
                ui.radio_value(&mut self.store.settings.whisper_model, id.to_string(), format!("{id} ({label})"));
            }
        });
        let path = stt::model_path(&self.store.models_dir(), &self.store.settings.whisper_model);
        match self.voice.stt.status() {
            SttStatus::Downloading => {
                let d = self.voice.stt.downloaded.load(std::sync::atomic::Ordering::Relaxed) as f32;
                let t = self.voice.stt.total.load(std::sync::atomic::Ordering::Relaxed).max(1) as f32;
                ui.add(egui::ProgressBar::new(d / t).text(format!("{:.0} / {:.0} MB", d / 1e6, t / 1e6)));
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            SttStatus::Loading => spinner_label(ui, "모델 로딩 중…"),
            SttStatus::Ready if path.exists() => {
                ui.label(RichText::new("✅ 음성 인식 준비 완료").color(GOOD));
            }
            status => {
                if let SttStatus::Error(e) = status {
                    error_label(ui, &e);
                }
                if path.exists() {
                    if ui.button("이 모델 불러오기").clicked() {
                        self.store.save_settings();
                        self.voice.stt.load(path);
                    }
                } else if ui.button("⬇ 모델 다운로드").clicked() {
                    self.store.save_settings();
                    self.voice.stt.download(self.store.models_dir(), self.store.settings.whisper_model.clone());
                }
            }
        }
    }

    // ------------------------------------------------------------ settings

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "⚙ 설정", "API 키, 음성, 학습 강도, 유튜브 도구를 설정합니다.");
        let mut changed = false;
        card(ui, |ui| {
            ui.label(RichText::new("Claude API").strong());
            ui.horizontal(|ui| {
                changed |= ui.add(egui::TextEdit::singleline(&mut self.store.settings.api_key).password(!self.show_key).desired_width(360.0).hint_text("sk-ant-...")).changed();
                ui.checkbox(&mut self.show_key, "표시");
            });
            ui.label(RichText::new("키는 이 컴퓨터의 settings.json 에만 저장됩니다. 비워두면 ANTHROPIC_API_KEY 환경변수를 사용합니다.").color(MUTED).small());
            egui::ComboBox::from_label("모델")
                .selected_text(self.store.settings.model.clone())
                .show_ui(ui, |ui| {
                    for (id, label) in crate::claude::MODELS {
                        changed |= ui.selectable_value(&mut self.store.settings.model, id.to_string(), *label).changed();
                    }
                });
        });
        card(ui, |ui| {
            ui.label(RichText::new("음성").strong());
            if let Some(e) = &self.voice.speaker.error {
                error_label(ui, e);
            }
            let voices = self.voice.speaker.voices.clone();
            let current = voices.iter().find(|v| v.0 == self.store.settings.tts_voice_id).map(|v| v.1.clone()).unwrap_or("기본".into());
            egui::ComboBox::from_label("원어민 음성 (TTS)").selected_text(current).show_ui(ui, |ui| {
                for (id, label) in &voices {
                    if ui.selectable_label(self.store.settings.tts_voice_id == *id, label).clicked() {
                        self.store.settings.tts_voice_id = id.clone();
                        self.voice.speaker.set_voice(id);
                        changed = true;
                    }
                }
            });
            if ui.add(egui::Slider::new(&mut self.store.settings.tts_rate, 0.6..=1.4).text("말하기 속도")).changed() {
                self.voice.base_rate = self.store.settings.tts_rate;
                changed = true;
            }
            if ui.button("🔊 테스트").clicked() {
                self.voice.say("Hello! Let's practice speaking English together.");
            }
            ui.add_space(6.0);
            ui.label("음성 인식 모델");
            self.stt_setup_ui(ui);
        });
        card(ui, |ui| {
            ui.label(RichText::new("오디오 장치 · 마이크 점검").strong());
            changed |= self.audio_check_ui(ui);
        });
        card(ui, |ui| {
            ui.label(RichText::new("학습").strong());
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.daily_goal_minutes, 10..=120).text("일일 목표 학습 시간(분)")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.pass_score, 60.0..=95.0).text("통과 점수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.reps_new, 1..=6).text("새 항목 반복 횟수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.reps_review, 1..=5).text("복습 반복 횟수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.reps_weak, 2..=10).text("약점 항목 반복 횟수")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.store.settings.new_words_per_day, 0..=20).text("하루 새 단어 (0 = 단계별 자동)")).changed();
            ui.horizontal(|ui| {
                ui.label("학습 언어:");
                for l in language::LANGUAGES {
                    let _ = ui.radio(self.lang.code == l.code, l.name_ko);
                }
                ui.label(RichText::new("(다른 언어는 추후 지원)").color(MUTED));
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("유튜브 (yt-dlp)").strong());
            let found = crate::youtube::find_ytdlp(&self.store.settings.ytdlp_path, &self.store.dir.join("bin"));
            match &found {
                Some(p) => {
                    ui.label(RichText::new(format!("✅ {}", p.display())).color(GOOD));
                }
                None => {
                    ui.label(RichText::new("yt-dlp 가 없습니다. 유튜브 원어 영상으로 구간반복하려면 설치하세요.").color(WARN));
                }
            }
            ui.horizontal(|ui| {
                ui.label("경로:");
                changed |= ui.text_edit_singleline(&mut self.store.settings.ytdlp_path).changed();
            });
            if self.ytdlp_job.running() {
                spinner_label(ui, "yt-dlp 내려받는 중…");
            } else if ui.button(if found.is_some() { "yt-dlp 최신 버전으로 업데이트" } else { "⬇ yt-dlp 자동 설치 (GitHub 공식 릴리스)" }).clicked() {
                let bin = self.store.dir.join("bin");
                self.ytdlp_job.spawn(move || crate::youtube::install_ytdlp(&bin));
            }
        });
        card(ui, |ui| {
            ui.label(RichText::new("데이터").strong());
            ui.label(format!("저장 위치: {}", self.store.dir.display()));
            ui.label(RichText::new("profile.json(레벨·기록), level_log.csv, items.json(단어·문장), homework.json(숙제), attempts.jsonl(발화 기록)").color(MUTED).small());
            if ui.button("📂 폴더 열기").clicked() {
                let _ = open::that(&self.store.dir);
            }
        });
        if changed {
            self.store.save_settings();
        }
    }

    // ------------------------------------------------------------ frame

    fn track_time(&mut self, ctx: &egui::Context) {
        let active = ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving()) || self.voice.recorder.is_recording();
        if active {
            self.last_activity = Instant::now();
        }
        let now = Instant::now();
        let dt = now - self.last_tick;
        if dt >= Duration::from_secs(1) {
            self.last_tick = now;
            let studying = !matches!(self.screen, Screen::Settings | Screen::Onboarding | Screen::Progress | Screen::History);
            if studying && now - self.last_activity < Duration::from_secs(120) {
                self.store.profile.study_seconds += dt.as_secs();
                self.store.profile.today.study_seconds += dt.as_secs();
            }
        }
        if now - self.last_save > Duration::from_secs(30) {
            self.last_save = now;
            self.store.save_profile();
            self.update_history();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.roll_day();
        self.poll_jobs();
        self.track_time(ctx);
        if self.lesson_job.running() || self.ytdlp_job.running() || self.voice.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Keep a slow heartbeat so background results show up even without input.
        ctx.request_repaint_after(Duration::from_millis(500));

        if !matches!(self.screen, Screen::Onboarding | Screen::LevelTest) {
            egui::SidePanel::left("nav")
                .exact_width(232.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(14, 0)).stroke(egui::Stroke::new(1.0_f32, BORDER)))
                .show(ctx, |ui| self.nav_ui(ui));
        }
        self.mic_permission_banner(ctx);
        if self.voice.player.output_stuck() {
            egui::TopBottomPanel::top("audio_warn").show(ctx, |ui| {
                let fix = if cfg!(target_os = "macos") {
                    "터미널에서 `sudo killall coreaudiod` 를 실행하면 오디오가 재시작됩니다."
                } else {
                    "출력 장치(스피커·헤드폰) 연결과 시스템 사운드 설정을 확인하세요."
                };
                ui.label(RichText::new(format!("🔇 시스템 오디오가 응답하지 않아 소리가 나지 않습니다. {fix}")).color(WARN).strong());
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 32, right: 24, top: 22, bottom: 10 })).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(900.0);
                match self.screen {
                    Screen::Onboarding => self.onboarding_ui(ui),
                    Screen::Home => self.home_ui(ui),
                    Screen::Homework => self.homework_ui(ui),
                    Screen::Review => self.review_ui(ui),
                    Screen::Words => self.words_ui(ui),
                    Screen::Reading => self.reading_ui(ui),
                    Screen::Speed => self.speed_ui(ui),
                    Screen::Listening => self.listening_ui(ui),
                    Screen::SelfExpression => self.self_expression_ui(ui),
                    Screen::Business => self.business_ui(ui),
                    Screen::Feedback => self.feedback_ui(ui),
                    Screen::Vocabulary => self.vocabulary_ui(ui),
                    Screen::History => self.history_ui(ui),
                    Screen::Progress => self.progress_ui(ui),
                    Screen::LevelTest => self.level_test_ui(ui),
                    Screen::Settings => self.settings_ui(ui),
                }
            });
        });
        if let Some((msg, at)) = &self.toast {
            if at.elapsed() < Duration::from_secs(3) {
                egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -30.0)).show(ctx, |ui| {
                    egui::Frame::new().fill(Color32::from_rgb(30, 40, 64)).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
                        ui.label(RichText::new(msg).size(16.0));
                    });
                });
            } else {
                self.toast = None;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.update_history();
        self.store.save_profile();
        self.store.save_items();
        self.store.save_homework();
        self.store.save_settings();
    }
}
