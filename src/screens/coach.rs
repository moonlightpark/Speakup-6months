//! Speaking coach (Study.md): listen-and-respond, 4/3/2 speaking, the weekly recording to hear
//! progress, and logs for human tutor lessons and mock tests (JLPT / JPT / OPIc 일본어).

use crate::app::{App, Screen};
use crate::claude::{AnswerJudgement, ListenQuestion};
use crate::curriculum;
use crate::models::*;
use crate::widgets::*;
use egui::{RichText, Ui};
use rand::seq::SliceRandom;

/// The same question every week, so recordings can be compared.
pub const WEEKLY_QUESTION: (&str, &str) = ("自己紹介と、今週したことを話してください。", "자기소개와 이번 주에 한 일을 1분 동안 말해 보세요.");
/// Human tutor lessons are recommended from month 3.
pub const TUTOR_FROM_DAY: i64 = 61;
pub const MOCK_TEST_EVERY_DAYS: i64 = 14;
/// 4/3/2: the same script read three times, in time limits of 4 : 3 : 2 (at most 4, 3, 2 minutes).
pub const ROUNDS: [f32; 3] = [240.0, 180.0, 120.0];

/// 4/3/2 time limits for a script: the last round is the script at the target pace for the
/// level, the first two get 4/2 and 3/2 of that (never under 10 s, never over 4/3/2 minutes).
pub fn round_limits(words: usize, target_wpm: f32) -> [f32; 3] {
    let last = (words as f32 / target_wpm * 60.0).max(10.0);
    let mut out = [0.0; 3];
    for (k, cap) in ROUNDS.iter().enumerate() {
        out[k] = (last * cap / ROUNDS[2]).min(*cap).round();
    }
    out
}

/// "1분 20초" / "40초".
fn mmss(secs: f32) -> String {
    let s = secs.round() as u32;
    match (s / 60, s % 60) {
        (0, x) => format!("{x}초"),
        (m, 0) => format!("{m}분"),
        (m, x) => format!("{m}분 {x}초"),
    }
}

/// Offline listen-and-respond questions: (question, 한국어, sample answer).
const LISTEN_QUESTIONS: &[(&str, &str, &str)] = &[
    ("お仕事は何をされているんですか。", "무슨 일 하세요?", "小さい会社でマーケティングをしています。"),
    ("週末はどうでしたか。", "주말 어땠어요?", "楽しかったです。友達と山に登りました。"),
    ("ここに来たことはありますか。", "여기 와 본 적 있어요?", "いいえ、初めてです。すごくいいところですね。"),
    ("今晩、何か予定ある？", "오늘 밤에 뭐 있어?", "特にないよ。たぶん家にいると思う。"),
    ("ちょっとコーヒーでも飲みに行かない？", "커피라도 한잔하러 갈래?", "いいね、行こう。どこにする？"),
    ("日本語を勉強してどのくらいですか。", "일본어 공부한 지 얼마나 됐어요?", "二か月ぐらいです。でも、アニメでよく聞いていました。"),
    ("どんな食べ物が好きですか。", "어떤 음식 좋아해요?", "辛い食べ物が好きです。特にキムチチゲが好きです。"),
    ("何かお困りですか。", "뭔가 도와드릴까요?", "はい、駅はどこか教えていただけますか。"),
    ("あの映画、どうだった？", "그 영화 어땠어?", "ちょっと長かったけど、最後がすごくよかった。"),
    ("次はどこに旅行したいですか。", "다음엔 어디로 여행 가고 싶어요?", "北海道に行ってみたいです。雪が見たいんです。"),
    ("金曜日までに資料を送っていただけますか。", "금요일까지 자료 보내 주실 수 있나요?", "はい、木曜日までにお送りします。"),
    ("今の仕事のいいところは何ですか。", "지금 일의 좋은 점이 뭐예요?", "チームの雰囲気がいいところです。みんな助け合っています。"),
];

/// Example scripts for 4/3/2 when nothing is pasted: (title, script).
const SAMPLE_SCRIPTS: &[(&str, &str)] = &[
    ("わたしの一日", "わたしはたいてい七時に起きます。コーヒーをいれて、メッセージをチェックします。それから地下鉄で会社に行きます。四十分ぐらいかかるので、電車の中でポッドキャストを聞きます。午前中は会議が多いです。昼ごはんのあとは、自分の仕事に集中します。七時ごろ家に帰って、簡単な晩ごはんを作ります。寝る前に三十分ぐらい本を読みます。"),
    ("思い出の旅行", "去年の春、友達二人と済州島に行きました。小さい車を借りて、海沿いをドライブしました。海はとても青くて、天気も最高でした。二日目は朝早く山に登りました。大変でしたが、上からの景色はすばらしかったです。夜は市場で新鮮な海鮮を食べました。短い旅行でしたが、今でも全部覚えています。"),
    ("わたしの仕事", "わたしはソウルの小さいデザイン会社で働いています。チームではほかの会社のウェブサイトやアプリを作っています。毎朝、短いミーティングで一日の予定を共有します。仕事の時間のほとんどは、お客さんと話したり、スケジュールを確認したりしています。締め切りが厳しいときはストレスもあります。でも、毎週新しいことが学べるので、この仕事が好きです。"),
    ("いい友達", "親友の名前はミンジです。高校で出会ったので、もう十五年の付き合いです。彼女はおもしろくて、優しくて、とても正直です。困ったことがあると、いつもまず話を聞いてくれます。今は釜山に住んでいるので、あまり会えません。でも、毎週末電話して、何時間も話します。こんな友達がいて、本当にラッキーだと思います。"),
    ("わたしの週末", "週末はゆっくりして、好きなことをするようにしています。土曜日の朝はジムに行ったり、川沿いを自転車で走ったりします。午後はよく友達とカフェで会います。日曜日はたいてい家にいて、掃除をします。夜は映画や日本のドラマを見ます。そうすると、新しい一週間の準備ができます。"),
];

#[derive(Default)]
pub struct ListenRespondState {
    pub questions: Vec<ListenQuestion>,
    pub idx: usize,
    job: Pending<Vec<ListenQuestion>>,
    pub error: Option<String>,
    pub capture: Capture,
    judge: Pending<AnswerJudgement>,
    pub transcript: Option<String>,
    pub result: Option<AnswerJudgement>,
    /// The learner looked at the question text before answering.
    pub peeked: bool,
    played: bool,
    pub understood: u32,
    pub answered: u32,
    asked: Vec<String>,
}

pub struct RoundResult {
    pub limit: f32,
    pub secs: f32,
    pub words: usize,
    pub wpm: f32,
    /// How much of the script was read right (0-100).
    pub accuracy: f32,
}

/// 4/3/2 reading: paste a Japanese script, hear it, read along sentence by sentence, then read
/// the whole of it three times in shorter and shorter time.
#[derive(Default)]
pub struct FourThreeTwoState {
    /// What is in the paste box.
    pub input: String,
    /// The script being practised, split into sentences.
    pub script: Option<Vec<String>>,
    /// ② Read along, sentence by sentence.
    pub along: crate::screens::read_along::ReadAlong,
    /// false = ② read along, true = ③ 4/3/2 rounds (one mic on screen at a time).
    pub rounds: bool,
    pub capture: Capture,
    pub results: Vec<RoundResult>,
}

#[derive(Default)]
pub struct CoachState {
    pub listen: ListenRespondState,
    pub ftt: FourThreeTwoState,
    pub weekly: Capture,
    pub weekly_error: Option<String>,
    pub test_kind: usize,
    pub test_result: String,
}

/// Length of a Japanese text in characters (the unit of speaking speed, 자/분).
fn word_count(t: &str) -> usize {
    crate::ja::char_count(t)
}

/// Days of the current program week (for "this week" counts).
fn in_this_week(p: &Profile, d: chrono::NaiveDate) -> bool {
    let start = p.start_date + chrono::Duration::days(((curriculum::week_number(p) - 1) * 7) as i64);
    d >= start && d < start + chrono::Duration::days(7)
}

pub fn tutor_this_week(p: &Profile) -> usize {
    p.tutor_sessions.iter().filter(|d| in_this_week(p, **d)).count()
}

pub fn weekly_recorded(p: &Profile) -> bool {
    let w = curriculum::week_number(p);
    p.weekly_recordings.iter().any(|r| r.week == w)
}

/// Days until the next mock test (≤ 0 = due now). Counted from the last one, or the start.
pub fn mock_test_due_in(p: &Profile) -> i64 {
    let last = p.external_tests.iter().filter_map(|t| t.date).max().unwrap_or(p.start_date);
    MOCK_TEST_EVERY_DAYS - (today() - last).num_days()
}

impl App {
    // ------------------------------------------------------------ listen and respond

    fn new_listen_questions(&mut self) {
        let claude = self.claude();
        let st = &mut self.coach.listen;
        st.error = None;
        st.idx = 0;
        st.understood = 0;
        st.answered = 0;
        st.questions.clear();
        let p = &self.store.profile;
        match claude {
            Some(c) => {
                let theme = curriculum::week(curriculum::week_number(p)).theme.to_string();
                let (level, avoid) = (p.level, st.asked.clone());
                st.job.spawn(move || c.listen_questions(level, &theme, &avoid));
            }
            None => {
                let mut pool: Vec<_> = LISTEN_QUESTIONS.iter().filter(|q| !st.asked.iter().any(|a| a == q.0)).collect();
                if pool.len() < 6 {
                    st.asked.clear();
                    pool = LISTEN_QUESTIONS.iter().collect();
                }
                pool.shuffle(&mut rand::rng());
                st.questions = pool.into_iter().take(6).map(|q| ListenQuestion { question: q.0.into(), question_ko: q.1.into(), sample_answer: q.2.into() }).collect();
            }
        }
        self.reset_listen_question();
    }

    fn reset_listen_question(&mut self) {
        let st = &mut self.coach.listen;
        st.transcript = None;
        st.result = None;
        st.peeked = false;
        st.played = false;
        st.capture = Capture::default();
    }

    /// Hear a question (no text), answer out loud, Claude checks it was understood and fits.
    pub fn listen_respond_ui(&mut self, ui: &mut Ui) {
        let claude = self.claude();
        if let Some(r) = self.coach.listen.job.poll() {
            match r {
                Ok(q) if !q.is_empty() => self.coach.listen.questions = q,
                Ok(_) => self.coach.listen.error = Some("질문을 받지 못했어요. 다시 시도해 주세요.".into()),
                Err(e) => self.coach.listen.error = Some(format!("{e:#}")),
            }
            self.reset_listen_question();
        }
        if let Some(r) = self.coach.listen.judge.poll() {
            match r {
                Ok(j) => {
                    self.coach.listen.answered += 1;
                    if j.understood {
                        self.coach.listen.understood += 1;
                        // Understood answers count toward today's listening goal.
                        self.store.profile.today.listening_passes += 1;
                        self.store.save_profile();
                        if self.store.profile.today.listening_passes >= 5 && !self.step_done("listening") {
                            self.complete_step("listening");
                        }
                    }
                    self.coach.listen.result = Some(j);
                }
                Err(e) => self.coach.listen.error = Some(format!("{e:#}")),
            }
        }

        card(ui, |ui| {
            ui.label("글자 없이 질문을 듣고, 바로 일본어로 대답하세요. 들을 수 있어야 대답할 수 있어요.");
            ui.label(RichText::new("질문 글과 해석은 대답한 뒤에 보여요. 이해한 대답은 오늘의 듣기 목표에 포함됩니다.").color(muted()));
        });
        ui.add_space(6.0);
        if let Some(e) = self.coach.listen.error.clone() {
            error_label(ui, &e);
        }
        if self.coach.listen.job.running() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label("Claude가 내 레벨에 맞는 질문을 만들고 있어요…");
            });
            return;
        }
        let n = self.coach.listen.questions.len();
        if n == 0 {
            if ui.add(egui::Button::new(RichText::new("▶ 질문 6개 받기").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                self.new_listen_questions();
            }
            return;
        }
        if self.coach.listen.idx >= n {
            let st = &self.coach.listen;
            card(ui, |ui| {
                ui.label(RichText::new(format!("👂 이해한 질문 {}/{}", st.understood, st.answered.max(n as u32))).size(26.0).strong());
                ui.label(RichText::new("못 알아들은 질문은 🔊 다시 듣고 소리 내어 따라 해 보세요.").color(muted()));
            });
            if ui.button("↻ 새 질문 받기").clicked() {
                self.new_listen_questions();
            }
            return;
        }
        let q = self.coach.listen.questions[self.coach.listen.idx].clone();
        if !self.coach.listen.played {
            self.coach.listen.played = true;
            if !self.coach.listen.asked.contains(&q.question) {
                self.coach.listen.asked.push(q.question.clone());
            }
            self.voice.say(&q.question);
        }
        let mut next = false;
        card(ui, |ui| {
            let st = &mut self.coach.listen;
            ui.label(RichText::new(format!("질문 {}/{n} — 듣고 일본어로 대답하세요", st.idx + 1)).color(muted()));
            let answered = st.result.is_some() || (st.transcript.is_some() && !st.judge.running());
            if st.peeked || answered {
                ui.label(RichText::new(&q.question).size(22.0).strong());
                crate::grammar::ko_label(ui, &q.question_ko, 16.0, muted());
            } else {
                ui.label(RichText::new("🙈 질문 글은 대답한 뒤에 보여요").size(18.0).color(muted()));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("🔊 다시 듣기").clicked() {
                    self.voice.say(&q.question);
                }
                if ui.button("🐢 천천히").clicked() {
                    self.voice.say_slow(&q.question);
                }
                if !st.peeked && !answered && ui.button("글 보기").on_hover_text("도저히 안 들릴 때만").clicked() {
                    st.peeked = true;
                }
            });
            ui.add_space(6.0);
            if st.result.is_none() && !st.judge.running() && st.transcript.is_none() {
                ui.vertical_centered(|ui| {
                    if let Some(t) = st.capture.ui(ui, &mut self.voice, 45.0) {
                        st.transcript = Some(t.clone());
                        match &claude {
                            Some(c) => {
                                let (c, question, level) = (c.clone(), q.question.clone(), self.store.profile.level);
                                st.judge.spawn(move || c.judge_answer(&question, &t, level));
                            }
                            // Offline: no judge — compare with the sample answer yourself.
                            None => st.answered += 1,
                        }
                    }
                });
            }
            if st.judge.running() {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label("Claude가 대답을 확인하고 있어요…");
                });
            }
            if let Some(t) = &st.transcript {
                ui.label(RichText::new(format!("내 대답: {t}")).color(muted()));
            }
            if let Some(r) = &st.result {
                let (msg, color) = if r.understood { ("✅ 질문을 이해했어요", GOOD) } else { ("❌ 질문을 다르게 이해했어요", BAD) };
                ui.label(RichText::new(format!("{msg} · {}점", r.score)).size(18.0).strong().color(color));
                ui.label(&r.feedback_ko);
                if !r.better_answer.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        speaker_button(ui, &mut self.voice, &r.better_answer);
                        ui.label(RichText::new(format!("더 자연스러운 대답: {}", r.better_answer)).color(gloss()));
                    });
                }
            }
            if answered {
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &q.sample_answer);
                    ui.label(RichText::new(format!("예시 대답: {}", q.sample_answer)).color(muted()));
                });
                next = ui.add(egui::Button::new("다음 ▶").min_size(crate::widgets::ACTION_BUTTON)).clicked();
            }
        });
        if next {
            self.coach.listen.idx += 1;
            self.reset_listen_question();
        }
    }

    // ------------------------------------------------------------ 4/3/2 speaking

    /// 4/3/2 reading: paste a script → ① listen → ② read along sentence by sentence → ③ read
    /// the whole script 3 times in 4 : 3 : 2 time.
    pub fn four_three_two_ui(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            ui.label("4/3/2 따라 읽기: 일본어 대본을 붙여넣으면 ① 원어민 음성으로 들려주고 ② 문장마다 따라 읽은 뒤 ③ 대본 전체를 3번, 4 : 3 : 2 시간 안에 점점 빠르게 읽어요.");
            ui.label(RichText::new("같은 대본을 더 짧은 시간에 정확하게 읽을수록 문장이 입에 붙어요. 시간이 끝나면 자동으로 멈춰요.").color(muted()));
        });
        ui.add_space(6.0);
        let Some(sentences) = self.coach.ftt.script.clone() else {
            self.ftt_paste_ui(ui);
            return;
        };
        let script = sentences.join(" ");
        let size = self.store.settings.list_size().max(18.0);
        let mut change = false;
        let mut jump: Option<usize> = None;
        // ① Listen to the whole script.
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("① 듣기").size(18.0).strong().color(ACCENT));
                ui.label(RichText::new(format!("{}문장 · {}단어", sentences.len(), word_count(&script))).color(muted()));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add(egui::Button::new("🔊 전체 듣기").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.say(&script);
                }
                if ui.add(egui::Button::new("🐢 천천히 듣기").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.say_slow(&script);
                }
                if ui.add(egui::Button::new("⏹ 멈춤").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.speaker.stop();
                }
                change = ui.add(egui::Button::new("✏ 대본 바꾸기").min_size(ACTION_BUTTON)).clicked();
            });
            ui.add_space(4.0);
            let along = std::mem::take(&mut self.coach.ftt.along);
            jump = self.script_view(ui, &sentences, &along, !self.coach.ftt.rounds, size * 0.9);
            self.coach.ftt.along = along;
        });
        if let Some(k) = jump {
            self.coach.ftt.rounds = false;
            self.coach.ftt.along.go(k);
            self.voice.say(&sentences[k]);
        }
        if change {
            self.voice.speaker.stop();
            self.coach.ftt.input = script;
            self.coach.ftt.script = None;
            return;
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let mut rounds = self.coach.ftt.rounds;
            ui.selectable_value(&mut rounds, false, RichText::new("② 문장별 따라 읽기").size(16.0));
            ui.label(RichText::new("→").color(muted()));
            ui.selectable_value(&mut rounds, true, RichText::new("③ 4/3/2 통째로 읽기").size(16.0));
            if rounds != self.coach.ftt.rounds {
                self.voice.speaker.stop();
                self.coach.ftt.rounds = rounds;
            }
        });
        ui.add_space(4.0);
        if self.coach.ftt.rounds {
            self.ftt_rounds_ui(ui, &script, size);
        } else {
            let mut along = std::mem::take(&mut self.coach.ftt.along);
            if self.read_along_ui(ui, &mut along, &sentences, size, "4/3/2 따라 읽기", "③ 통째로 읽기로 ▶") {
                self.coach.ftt.rounds = true;
            }
            self.coach.ftt.along = along;
        }
    }

    fn ftt_paste_ui(&mut self, ui: &mut Ui) {
        let mut start = false;
        card(ui, |ui| {
            ui.label(RichText::new("📋 일본어 대본 붙여넣기").size(18.0).strong());
            ui.label(RichText::new("연습할 일본어 글(대본·기사·스피치·드라마 대사 등)을 붙여넣으세요. 문장 단위로 나눠서 연습해요.").color(muted()));
            ui.add(egui::TextEdit::multiline(&mut self.coach.ftt.input).desired_rows(8).desired_width(f32::INFINITY).hint_text("ここに日本語の文章を貼り付けてください…"));
            let ok = self.coach.ftt.input.chars().filter(|c| crate::ja::is_japanese_char(*c)).count() >= 10;
            ui.horizontal_wrapped(|ui| {
                start = ui.add_enabled(ok, egui::Button::new(RichText::new("✔ 이 대본으로 시작").size(17.0).color(egui::Color32::WHITE)).fill(ACCENT).min_size(BIG_BUTTON)).clicked();
                if ui.add(egui::Button::new("📄 예시 대본 넣기").min_size(ACTION_BUTTON)).clicked() {
                    let day = curriculum::day_number(&self.store.profile) as usize;
                    self.coach.ftt.input = SAMPLE_SCRIPTS[day % SAMPLE_SCRIPTS.len()].1.to_string();
                }
                if !self.coach.ftt.input.is_empty() && ui.add(egui::Button::new("🗑 지우기").min_size(ACTION_BUTTON)).clicked() {
                    self.coach.ftt.input.clear();
                }
            });
            if !ok && !self.coach.ftt.input.trim().is_empty() {
                ui.label(RichText::new("일본어 문장을 붙여넣어 주세요.").color(warn()).small());
            }
        });
        if start {
            let sentences = crate::screens::speaking::split_sentences(&self.coach.ftt.input.split_whitespace().collect::<Vec<_>>().join(""));
            crate::applog!("4/3/2: 대본 {}문장으로 시작", sentences.len());
            self.coach.ftt = FourThreeTwoState {
                input: std::mem::take(&mut self.coach.ftt.input),
                along: crate::screens::read_along::ReadAlong::new(sentences.len()),
                script: Some(sentences),
                ..Default::default()
            };
        }
    }

    /// ③ The whole script three times, in 4 : 3 : 2 time.
    fn ftt_rounds_ui(&mut self, ui: &mut Ui, script: &str, size: f32) {
        let words = word_count(script);
        let limits = round_limits(words, crate::screens::speed::target_wpm(self.store.profile.level));
        let round = self.coach.ftt.results.len();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("③ 4/3/2 읽기").size(18.0).strong().color(ACCENT));
                ui.label(RichText::new(format!("{} → {} → {}", mmss(limits[0]), mmss(limits[1]), mmss(limits[2]))).color(muted()));
            });
            if round < ROUNDS.len() {
                let limit = limits[round];
                ui.label(RichText::new(format!("라운드 {}/3 · {} 안에 대본 전체 읽기", round + 1, mmss(limit))).size(18.0).strong());
                if self.voice.recorder.is_recording() && self.coach.ftt.capture.is_recording() {
                    let left = (limit - self.voice.recorder.elapsed()).max(0.0);
                    ui.label(RichText::new(format!("남은 시간 {}:{:02}", left as u32 / 60, left as u32 % 60)).size(22.0).strong());
                }
                egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
                    ui.label(RichText::new(script).size(size * 0.9).color(text_strong()));
                });
                ui.vertical_centered(|ui| {
                    if let Some(t) = self.coach.ftt.capture.ui(ui, &mut self.voice, limit) {
                        let secs = (self.voice.last_recording.len() as f32 / crate::audio::STT_RATE as f32).max(1.0);
                        let said = word_count(&t);
                        let accuracy = crate::scoring::score(script, &t).score;
                        self.store.log_attempt("4/3/2", &format!("라운드 {}", round + 1), &t, accuracy);
                        self.coach.ftt.results.push(RoundResult { limit, secs, words: said, wpm: said as f32 / (secs / 60.0), accuracy });
                    }
                });
            }
            for (i, r) in self.coach.ftt.results.iter().enumerate() {
                ui.label(format!(
                    "라운드 {} ({} 제한): {}:{:02} 동안 {}단어 · {:.0}자/분 · 정확도 {:.0}%",
                    i + 1,
                    mmss(r.limit),
                    r.secs as u32 / 60,
                    r.secs as u32 % 60,
                    r.words,
                    r.wpm,
                    r.accuracy
                ));
            }
        });
        if self.coach.ftt.results.len() > round {
            self.speed_round_done();
        }
        if self.coach.ftt.results.len() == ROUNDS.len() {
            let r = &self.coach.ftt.results;
            let (a, b) = (r[0].wpm, r[2].wpm);
            card(ui, |ui| {
                let gain = if a > 0.0 { (b / a - 1.0) * 100.0 } else { 0.0 };
                ui.label(RichText::new(format!("읽기 속도 {a:.0} → {b:.0}자/분 ({gain:+.0}%)")).size(22.0).strong().color(if b >= a { GOOD } else { warn() }));
                ui.label(RichText::new(format!("정확도 {:.0}% → {:.0}%", r[0].accuracy, r[2].accuracy)).size(18.0));
                ui.label(RichText::new("속도가 올라도 정확도가 유지되면 성공! 같은 대본을 내일 한 번 더 하면 더 빨라져요.").color(muted()));
            });
            if ui.button("↻ 같은 대본으로 다시").clicked() {
                self.coach.ftt.results.clear();
                self.coach.ftt.capture = Capture::default();
            }
        }
    }

    // ------------------------------------------------------------ progress page sections

    /// Weekly recording, human tutor lessons and mock tests (on the level page).
    pub fn coach_progress_ui(&mut self, ui: &mut Ui) {
        self.weekly_recording_ui(ui);
        ui.add_space(6.0);
        self.tutor_ui(ui);
        ui.add_space(6.0);
        self.mock_test_ui(ui);
    }

    fn weekly_recording_ui(&mut self, ui: &mut Ui) {
        let week = curriculum::week_number(&self.store.profile);
        let mut play: Option<String> = None;
        let mut recorded: Option<String> = None;
        card(ui, |ui| {
            ui.label(RichText::new("📼 주간 녹음 비교").size(18.0).strong());
            ui.label(RichText::new("매주 같은 질문에 1분 동안 대답해 녹음해요. 첫 달 녹음과 지금을 비교해 들으면 실력이 는 게 들려요.").color(muted()));
            ui.horizontal_wrapped(|ui| {
                speaker_button(ui, &mut self.voice, WEEKLY_QUESTION.0);
                ui.label(RichText::new(WEEKLY_QUESTION.0).size(18.0).strong());
            });
            crate::grammar::ko_label(ui, WEEKLY_QUESTION.1, 16.0, muted());
            if weekly_recorded(&self.store.profile) {
                ui.label(RichText::new(format!("✅ {week}주차 녹음 완료 — 다시 녹음하면 이번 주 녹음을 바꿔요")).color(GOOD));
            }
            ui.vertical_centered(|ui| {
                recorded = self.coach.weekly.ui(ui, &mut self.voice, 60.0);
            });
            if let Some(e) = &self.coach.weekly_error {
                error_label(ui, e);
            }
            let recs = self.store.profile.weekly_recordings.clone();
            if recs.len() >= 2 {
                let (first, last) = (&recs[0], &recs[recs.len() - 1]);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(format!("▶ 처음 ({}주차)", first.week)).clicked() {
                        play = Some(first.file.clone());
                    }
                    if ui.button(format!("▶ 최근 ({}주차)", last.week)).clicked() {
                        play = Some(last.file.clone());
                    }
                    ui.label(RichText::new(format!("1분에 {}단어 → {}단어", first.words, last.words)).strong().color(if last.words >= first.words { GOOD } else { warn() }));
                });
            }
            for r in recs.iter().rev() {
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("▶").clicked() {
                        play = Some(r.file.clone());
                    }
                    ui.label(format!("{}주차 · {} · {}단어 · {:.0}자/분", r.week, r.date.map(|d| d.to_string()).unwrap_or_default(), r.words, r.wpm));
                    ui.label(RichText::new(&r.transcript).small().color(muted()));
                });
            }
        });
        if let Some(t) = recorded {
            match self.save_weekly_recording(week, &t) {
                Ok(()) => self.toast(format!("{week}주차 녹음을 저장했어요")),
                Err(e) => self.coach.weekly_error = Some(format!("{e:#}")),
            }
        }
        if let Some(f) = play {
            match crate::audio::decode_segment(&self.store.recordings_dir().join(f), 0.0, 75.0) {
                Ok((samples, rate)) => self.voice.player.play_samples(samples, rate),
                Err(e) => self.coach.weekly_error = Some(format!("{e:#}")),
            }
        }
    }

    fn save_weekly_recording(&mut self, week: u32, transcript: &str) -> anyhow::Result<()> {
        let file = format!("week-{week:02}.wav");
        let audio = &self.voice.last_recording;
        crate::audio::write_wav(&self.store.recordings_dir().join(&file), audio, crate::audio::STT_RATE)?;
        let secs = (audio.len() as f32 / crate::audio::STT_RATE as f32).max(1.0);
        let words = word_count(transcript) as u32;
        let rec = WeeklyRecording { week, date: Some(today()), file, transcript: transcript.to_string(), words, wpm: words as f32 / (secs / 60.0) };
        let recs = &mut self.store.profile.weekly_recordings;
        recs.retain(|r| r.week != week);
        recs.push(rec);
        recs.sort_by_key(|r| r.week);
        self.store.save_profile();
        Ok(())
    }

    fn tutor_ui(&mut self, ui: &mut Ui) {
        let p = &self.store.profile;
        let day = curriculum::day_number(p);
        let (this_week, total) = (tutor_this_week(p), p.tutor_sessions.len());
        let mut log = false;
        card(ui, |ui| {
            ui.label(RichText::new("🎓 원어민 화상 수업").size(18.0).strong());
            ui.label(RichText::new("AI 롤플레이만으로는 상대의 예상 못 한 반응에 대처하는 경험이 부족해요. 3개월 차부터 주 1~2회, 30분씩 원어민과 화상 수업(예: italki의 일본어 튜터)을 함께 하세요.").color(muted()));
            if day < TUTOR_FROM_DAY {
                ui.label(format!("Day {TUTOR_FROM_DAY}부터 권장 (지금 Day {day})"));
            } else {
                let color = if this_week >= 1 { GOOD } else { warn() };
                ui.label(RichText::new(format!("이번 주 수업 {this_week}회 / 목표 1~2회 · 지금까지 {total}회")).strong().color(color));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("🌐 italki 열기").clicked() {
                    let _ = open::that("https://www.italki.com");
                }
                log = ui.button("✅ 오늘 수업했어요").clicked();
            });
        });
        if log && !self.store.profile.tutor_sessions.contains(&today()) {
            self.store.profile.tutor_sessions.push(today());
            self.store.save_profile();
            self.toast("수업을 기록했어요. 수업에서 막혔던 표현은 피드백 센터나 단어장에 넣어 복습하세요.");
        }
    }

    fn mock_test_ui(&mut self, ui: &mut Ui) {
        const KINDS: [&str; 3] = ["JLPT", "JPT", "OPIc 일본어"];
        let due_in = mock_test_due_in(&self.store.profile);
        let mut add = false;
        card(ui, |ui| {
            ui.label(RichText::new("📝 외부 모의고사 (2주마다)").size(18.0).strong());
            ui.label(RichText::new("2주마다 JLPT 모의시험(청해), JPT, OPIc 일본어 같은 시험으로 실력을 확인하고 결과를 기록하세요.").color(muted()));
            if due_in <= 0 {
                ui.label(RichText::new("지금 볼 때예요").strong().color(warn()));
            } else {
                ui.label(format!("다음 모의고사까지 {due_in}일"));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("🌐 JLPT 문제 예시").clicked() {
                    let _ = open::that("https://www.jlpt.jp/samples/forlearners.html");
                }
                if ui.button("🌐 JPT").clicked() {
                    let _ = open::that("https://www.jpt.co.kr");
                }
                if ui.button("🌐 OPIc").clicked() {
                    let _ = open::that("https://www.opic.or.kr");
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (i, k) in KINDS.iter().enumerate() {
                    ui.radio_value(&mut self.coach.test_kind, i, *k);
                }
                ui.add(egui::TextEdit::singleline(&mut self.coach.test_result).hint_text("등급·점수 (예: N3 합격, 650, IM2)").desired_width(160.0));
                add = ui.add_enabled(!self.coach.test_result.trim().is_empty(), egui::Button::new("기록")).clicked();
            });
            for t in self.store.profile.external_tests.iter().rev() {
                ui.label(format!("{} · {} · {}", t.date.map(|d| d.to_string()).unwrap_or_default(), t.test, t.result));
            }
        });
        if add {
            let test = KINDS[self.coach.test_kind.min(KINDS.len() - 1)].to_string();
            let result = std::mem::take(&mut self.coach.test_result).trim().to_string();
            self.store.profile.external_tests.push(ExternalTest { date: Some(today()), test, result });
            self.store.save_profile();
        }
    }

    /// This week's to-dos outside the daily routine (home screen). Each opens the level page.
    pub fn coach_reminders_ui(&mut self, ui: &mut Ui) {
        let p = &self.store.profile;
        let mut todo: Vec<String> = Vec::new();
        if !weekly_recorded(p) {
            todo.push("📼 이번 주 1분 녹음을 아직 안 했어요".into());
        }
        if curriculum::day_number(p) >= TUTOR_FROM_DAY && tutor_this_week(p) == 0 {
            todo.push("🎓 이번 주 원어민 화상 수업 (주 1~2회 권장)".into());
        }
        if mock_test_due_in(p) <= 0 {
            todo.push("📝 모의고사 볼 때예요 (2주마다)".into());
        }
        if todo.is_empty() {
            return;
        }
        let mut open = false;
        card(ui, |ui| {
            ui.label(RichText::new("이번 주 할 일").size(17.0).strong());
            for t in &todo {
                open |= ui.link(t).clicked();
            }
        });
        ui.add_space(12.0);
        if open {
            self.go(Screen::Progress);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_limits_keep_the_4_3_2_ratio() {
        // 120 characters at 240 a minute: 30 s in the last round → 60 s, 45 s, 30 s.
        assert_eq!(round_limits(120, 240.0), [60.0, 45.0, 30.0]);
        // A very long script is capped at 4, 3, 2 minutes; a tiny one gets at least 10 s.
        assert_eq!(round_limits(4000, 240.0), [240.0, 180.0, 120.0]);
        assert_eq!(round_limits(6, 240.0), [20.0, 15.0, 10.0]);
    }

    #[test]
    fn weekly_tutor_and_mock_test_schedules() {
        let mut p = Profile { start_date: today() - chrono::Duration::days(20), ..Default::default() };
        // Day 21 = week 3, which started 6 days ago.
        assert!(!weekly_recorded(&p));
        p.weekly_recordings.push(WeeklyRecording { week: 3, ..Default::default() });
        assert!(weekly_recorded(&p));
        p.tutor_sessions = vec![today() - chrono::Duration::days(6), today() - chrono::Duration::days(7)];
        assert_eq!(tutor_this_week(&p), 1, "a lesson from last week doesn't count");
        assert!(mock_test_due_in(&p) <= 0, "no test in the first 14 days → due");
        p.external_tests.push(ExternalTest { date: Some(today() - chrono::Duration::days(4)), ..Default::default() });
        assert_eq!(mock_test_due_in(&p), 10);
        assert_eq!(word_count("はい、そうです。"), 6);
    }
}
