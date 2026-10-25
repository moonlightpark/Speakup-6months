//! Speed training (Order.md: "빨리 읽고 빨리 말할 수 있는 훈련").
//! 1) Speed reading aloud — each program day (회차) has a 2-minute script on the week's theme:
//!    hear it, read along sentence by sentence, then read all of it (characters a minute over
//!    the correctly read words, against a level-based target).
//! 2) Quick response — a Korean prompt appears, the mic opens automatically, and the learner
//!    must say the Japanese within a time limit.

use crate::app::App;
use crate::audio;
use crate::claude::SpeedPassage;
use crate::content;
use crate::curriculum;
use crate::models::*;
use crate::scoring::{self, ScoreResult};
use crate::widgets::*;
use egui::{RichText, Ui};
use rand::seq::SliceRandom;
use std::time::Instant;

#[derive(Default)]
pub struct SpeedState {
    pub tab: usize,
    // speed reading
    pub passage: Option<SpeedPassage>,
    pub job: Pending<SpeedPassage>,
    pub error: Option<String>,
    pub capture: Capture,
    pub attempts: Vec<(f32, f32)>,
    pub last: Option<(String, ScoreResult, f32)>,
    pub show_ko: bool,
    pub show_gloss: bool,
    /// Books from the passages folder (loaded on first use) and the open one: (book, page).
    pub books: Option<Vec<crate::passages::Book>>,
    pub book: Option<(usize, usize)>,
    /// The program day (회차) whose 2-minute script is open, and the one Claude is writing.
    pub day: Option<u32>,
    pub job_day: u32,
    /// false = 문장별 따라 읽기, true = 통째로 읽기 (2분 도전).
    pub whole: bool,
    pub along: crate::screens::read_along::ReadAlong,
    // quick response
    pub queue: Vec<(String, String)>,
    pub idx: usize,
    pub shown_at: Option<Instant>,
    pub rcap: Capture,
    pub result: Option<(String, ScoreResult, f32, bool)>,
    pub stats: Vec<(f32, f32, bool)>,
}

/// Target reading-aloud speed for a level, in Japanese characters a minute (자/분): ~180 for
/// beginners up to ~360 (natural native conversation, about 7–8 morae a second).
pub fn target_wpm(level: f32) -> f32 {
    (150.0 + level * 22.0).min(360.0)
}

fn time_limit(target: &str) -> f32 {
    2.5 + 0.2 * crate::ja::char_count(target) as f32
}

impl App {
    pub fn speed_round_done(&mut self) {
        self.store.profile.today.speed_rounds += 1;
        self.store.save_profile();
        if self.store.profile.today.speed_rounds >= 3 && !self.step_done("speed") {
            self.complete_step("speed");
        }
    }

    pub fn speed_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "⚡ 스피드 트레이닝", "빨리 읽고 빨리 말하기. 정확도를 지키면서 속도를 올립니다. 오늘 목표: 3라운드.");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.speed.tab, 0, "📖 스피드 리딩 (자/분)");
            ui.selectable_value(&mut self.speed.tab, 1, "💨 순발력 말하기");
            ui.selectable_value(&mut self.speed.tab, 2, "⏱ 4/3/2 말하기");
            ui.label(RichText::new(format!("오늘 {}/3 라운드", self.store.profile.today.speed_rounds.min(3))).color(muted()));
        });
        ui.add_space(6.0);
        match self.speed.tab {
            0 => self.speed_reading_ui(ui),
            1 => self.quick_response_ui(ui),
            _ => self.four_three_two_ui(ui),
        }
    }

    fn books(&mut self) -> &[crate::passages::Book] {
        let dir = crate::passages::dir(&self.store.dir);
        self.speed.books.get_or_insert_with(|| crate::passages::load(&dir))
    }

    /// A fresh start on `p`: no attempts yet, read-along from the first sentence.
    fn set_passage(&mut self, p: SpeedPassage) {
        self.speed.attempts.clear();
        self.speed.last = None;
        self.speed.capture = Capture::default();
        self.speed.along = crate::screens::read_along::ReadAlong::new(crate::screens::speaking::split_sentences(&p.passage).len());
        self.speed.whole = false;
        self.speed.passage = Some(p);
    }

    /// Shows one page of a book from the passages folder as the passage to read.
    fn open_book_page(&mut self, book: usize, page: usize) {
        let Some(b) = self.books().get(book).cloned() else { return };
        let page = page.min(b.pages.len().saturating_sub(1));
        self.speed.error = None;
        self.speed.show_gloss = false;
        self.set_passage(SpeedPassage {
            title: format!("{} · {}/{}쪽", b.title, page + 1, b.pages.len()),
            passage: b.pages[page].clone(),
            passage_ko: String::new(),
            chunks: Vec::new(),
        });
        self.speed.book = Some((book, page));
        self.speed.day = None;
        self.store.profile.speed_book_pages.insert(b.title.clone(), page);
        self.store.save_profile();
    }

    /// Words in a 2-minute script at the learner's target pace.
    fn two_minute_words(&self) -> u32 {
        (target_wpm(self.store.profile.level) * 2.0 / 10.0).round() as u32 * 10
    }

    /// Opens 회차 `day`'s 2-minute script: the kept one, else Claude writes it (kept for next
    /// time), else the built-in one for its week.
    fn open_day(&mut self, day: u32) {
        self.speed.book = None;
        self.speed.day = Some(day);
        self.speed.error = None;
        self.speed.passage = None;
        if let Some(p) = self.store.speed_scripts.get(&day).cloned() {
            self.show_day_script(p);
            return;
        }
        let week = crate::speed_scripts::week_of(day);
        let w = curriculum::week(week);
        match self.claude().filter(|_| !self.talk_offline()) {
            Some(c) => {
                let (theme, grammar, level, words) = (w.theme.to_string(), w.grammar.to_string(), self.store.profile.level, self.two_minute_words());
                let mut known: Vec<String> = self.store.items.iter().filter(|i| i.kind == ItemKind::Word).map(|i| i.text.clone()).collect();
                known.shuffle(&mut rand::rng());
                known.truncate(15);
                let avoid: Vec<String> = self.store.speed_scripts.values().map(|p| p.title.clone()).collect();
                crate::applog!("speed: {day}일차 2분 대본 요청 ({words}자, {})", w.theme);
                self.speed.job_day = day;
                self.speed.job.spawn(move || c.speed_passage(day, &theme, &grammar, level, words, &known, &avoid));
            }
            None => self.builtin_day(day),
        }
    }

    fn builtin_day(&mut self, day: u32) {
        let p = crate::speed_scripts::for_day(day).unwrap_or_else(|| {
            let (t, text) = content::speed_passages()[day as usize % content::speed_passages().len()];
            SpeedPassage { title: t.into(), passage: text.into(), passage_ko: String::new(), chunks: Vec::new() }
        });
        self.show_day_script(p);
    }

    fn show_day_script(&mut self, mut p: SpeedPassage) {
        if p.chunks.is_empty() {
            p.chunks = self.store.gloss_for(&p.passage);
        }
        self.speed.show_gloss = false;
        self.set_passage(p);
    }

    fn speed_reading_ui(&mut self, ui: &mut Ui) {
        if let Some(r) = self.speed.job.poll() {
            let day = self.speed.job_day;
            match r {
                Ok(mut p) => {
                    self.store.put_gloss(&p.passage, &p.chunks);
                    p.chunks = self.store.gloss_for(&p.passage);
                    self.store.put_speed_script(day, &p);
                    if self.speed.day == Some(day) {
                        self.show_day_script(p);
                    }
                }
                Err(e) => {
                    crate::applog!("speed: 대본 생성 실패 · {e:#}");
                    if self.speed.day == Some(day) {
                        self.speed.error = Some(format!("Claude가 대본을 만들지 못해 내장 대본으로 연습해요 ({e:#})"));
                        self.builtin_day(day);
                    }
                }
            }
        }
        let today_day = curriculum::day_number(&self.store.profile) as u32;
        // Today's 회차 opens by itself.
        if self.speed.passage.is_none() && !self.speed.job.running() && self.speed.book.is_none() {
            let d = self.speed.day.unwrap_or(today_day);
            self.open_day(d);
        }
        let level = self.store.profile.level;
        let target = target_wpm(level);
        let best = self.store.profile.speed_history.iter().filter(|r| r.kind == "reading" && r.accuracy >= 85.0).map(|r| r.wpm).fold(0.0f32, f32::max);
        card(ui, |ui| {
            ui.label("방법: 회차마다 2분 분량의 일본어 대본 — ① 원어민 음성 듣기 → ② 문장별 따라 읽기 → ③ 대본 전체를 2분 안에 통째로 읽기 (반복할수록 빨라져요)");
            ui.label(RichText::new(format!("목표: 정확도 85% 이상 + 분당 {target:.0}자 이상 · 내 최고 {best:.0}자/분 · 원어민 대화 속도 약 350자/분")).color(ACCENT));
        });
        ui.add_space(6.0);
        // Which script: the 회차 (◀ ▶ to earlier days), or a book from the passages folder.
        let n_books = self.books().len();
        let mut open: Option<u32> = None;
        let mut book_mode = false;
        ui.horizontal_wrapped(|ui| {
            match self.speed.day {
                Some(d) => {
                    let w = crate::speed_scripts::week_of(d);
                    if ui.add_enabled(d > 1, egui::Button::new("◀ 이전 회차")).clicked() {
                        open = Some(d - 1);
                    }
                    ui.label(RichText::new(format!("📅 {d}일차 · {w}주차 {}", curriculum::week(w).theme_ko)).size(17.0).strong());
                    if ui.add_enabled(d < today_day, egui::Button::new("다음 회차 ▶")).clicked() {
                        open = Some(d + 1);
                    }
                    if d != today_day && ui.button("오늘 회차").clicked() {
                        open = Some(today_day);
                    }
                }
                None => {
                    if ui.button(format!("📅 오늘 회차({today_day}일차) 대본으로")).clicked() {
                        open = Some(today_day);
                    }
                }
            }
            if n_books > 0 && self.speed.book.is_none() && ui.button(format!("📚 내 책 {n_books}권에서 읽기")).clicked() {
                book_mode = true;
            }
        });
        if let Some(d) = open {
            self.voice.speaker.stop();
            self.open_day(d);
            return;
        }
        if book_mode {
            let level = self.store.profile.level;
            if let Some(bi) = crate::passages::pick(self.books(), level) {
                let title = self.books()[bi].title.clone();
                let page = self.store.profile.speed_book_pages.get(&title).copied().unwrap_or(0);
                crate::applog!("speed: 지문 {title} {}쪽 (레벨 {level:.1})", page + 1);
                self.open_book_page(bi, page);
            }
            return;
        }
        if let Some(e) = &self.speed.error {
            ui.label(RichText::new(format!("⚠ {e}")).color(warn()).small());
        }
        let Some(p) = self.speed.passage.clone() else {
            spinner_label(ui, "Claude가 오늘 회차의 2분 대본을 만들고 있어요…");
            return;
        };
        if let Some((bi, page)) = self.speed.book {
            let books: Vec<(u32, String, usize)> = self.books().iter().map(|b| (b.stage, b.title.clone(), b.pages.len())).collect();
            let pages = books.get(bi).map(|b| b.2).unwrap_or(1);
            let mut go: Option<(usize, usize)> = None;
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("speed_book").selected_text(books.get(bi).map(|b| format!("{}단계 · {}", b.0, b.1)).unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, (stage, title, n)) in books.iter().enumerate() {
                        if ui.selectable_label(i == bi, format!("{stage}단계 · {title} ({n}쪽)")).clicked() && i != bi {
                            go = Some((i, self.store.profile.speed_book_pages.get(title).copied().unwrap_or(0)));
                        }
                    }
                });
                if ui.add_enabled(page > 0, egui::Button::new("◀ 이전 쪽")).clicked() {
                    go = Some((bi, page - 1));
                }
                ui.label(RichText::new(format!("{} / {pages}쪽", page + 1)).strong());
                if ui.add_enabled(page + 1 < pages, egui::Button::new("다음 쪽 ▶")).clicked() {
                    go = Some((bi, page + 1));
                }
            });
            if let Some((b, pg)) = go {
                self.open_book_page(b, pg);
                return;
            }
            ui.add_space(4.0);
        }
        let sentences = crate::screens::speaking::split_sentences(&p.passage);
        let words = crate::ja::char_count(&p.passage);
        let size = self.store.settings.list_size().max(18.0);
        let mut jump: Option<usize> = None;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&p.title).size(20.0).strong());
                ui.label(RichText::new(format!("{words}자 · 약 {:.1}분", words as f32 / target)).color(muted()));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add(egui::Button::new("🔊 원어민 속도").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.say(&p.passage);
                }
                if ui.add(egui::Button::new("🐢 천천히").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.say_slow(&p.passage);
                }
                if ui.add(egui::Button::new("🐇 빠르게").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.say_fast(&p.passage);
                }
                if ui.add(egui::Button::new("⏹ 멈춤").min_size(ACTION_BUTTON)).clicked() {
                    self.voice.speaker.stop();
                }
                if !p.chunks.is_empty() {
                    ui.toggle_value(&mut self.speed.show_gloss, "📖 직독직해");
                }
                if !p.passage_ko.is_empty() {
                    ui.checkbox(&mut self.speed.show_ko, "해석");
                }
            });
            ui.add_space(4.0);
            let marks = self.speed.last.as_ref().map(|(_, r, _)| r.marks.as_slice());
            if self.speed.show_gloss && !p.chunks.is_empty() {
                chunk_view_lines(ui, &p.chunks, marks, size);
            } else if let (true, Some((_, r, _))) = (self.speed.whole, &self.speed.last) {
                // After reading it all: right words green, missed ones red.
                word_feedback_lines(ui, r, size);
            } else {
                let along = std::mem::take(&mut self.speed.along);
                jump = self.script_view(ui, &sentences, &along, !self.speed.whole, size);
                self.speed.along = along;
            }
            if self.speed.show_ko {
                ui.add_space(4.0);
                sentence_lines(ui, &p.passage_ko, 16.0, muted());
            }
        });
        if let Some(k) = jump {
            self.speed.whole = false;
            self.speed.along.go(k);
            self.voice.say(&sentences[k]);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let mut whole = self.speed.whole;
            ui.selectable_value(&mut whole, false, RichText::new("② 문장별 따라 읽기").size(16.0));
            ui.label(RichText::new("→").color(muted()));
            ui.selectable_value(&mut whole, true, RichText::new("③ 통째로 읽기 (2분 도전)").size(16.0));
            if whole != self.speed.whole {
                self.voice.speaker.stop();
                self.speed.whole = whole;
            }
        });
        ui.add_space(4.0);
        if !self.speed.whole {
            let mut along = std::mem::take(&mut self.speed.along);
            if self.read_along_ui(ui, &mut along, &sentences, size, "speed-follow", "③ 통째로 읽기로 ▶") {
                self.speed.whole = true;
            }
            self.speed.along = along;
            return;
        }
        let limit = (words as f32 / target * 60.0 * 1.5).max(120.0);
        let mut new_result = None;
        card(ui, |ui| {
            ui.label(RichText::new("대본 전체를 소리 내어 읽어요. 목표는 2분 안에, 정확하게!").strong());
            if self.speed.capture.is_recording() && self.voice.recorder.is_recording() {
                let e = self.voice.recorder.elapsed();
                ui.label(RichText::new(format!("⏱ {}:{:02} / 목표 2:00", e as u32 / 60, e as u32 % 60)).size(22.0).strong().color(if e <= 120.0 { ACCENT } else { warn() }));
            }
            ui.vertical_centered(|ui| {
                if let Some(t) = self.speed.capture.ui(ui, &mut self.voice, limit) {
                    new_result = Some(t);
                }
            });
        });
        if let Some(t) = new_result {
            let r = scoring::score(&p.passage, &t);
            let correct = r.heard_chars();
            let wpm = audio::wpm(correct, self.speed.capture.last_secs);
            let missed: Vec<String> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect();
            self.record_attempt("speed-reading", &p.passage, &Attempt { score: r.score, transcript: t.clone(), missed });
            self.store.profile.speed_history.push(SpeedRecord { date: today(), kind: "reading".into(), wpm, accuracy: r.score, seconds: self.speed.capture.last_secs, title: p.title.clone() });
            self.speed.attempts.push((wpm, r.score));
            self.speed.last = Some((t, r, wpm));
            self.speed_round_done();
        }
        let t_read = crate::art::since_event(ui, egui::Id::new("cheer_speed_reading"), self.store.profile.speed_history.iter().filter(|s| s.kind == "reading").count() as u64);
        if let Some((_, r, wpm)) = &self.speed.last {
            let ok = r.score >= 85.0 && *wpm >= target;
            if ok {
                crate::art::cheer_strip(ui, t_read, &format!("🎯 목표 달성! 분당 {wpm:.0}자"));
            } else if r.score >= 85.0 {
                crate::art::cheer_strip(ui, t_read, &format!("👍 정확해요! {:.0}% — 이제 속도를 올려 봐요", r.score));
            }
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{wpm:.0}자/분")).size(34.0).strong().color(if *wpm >= target { GOOD } else { warn() }));
                    ui.add_space(16.0);
                    ui.label(RichText::new(format!("정확도 {:.0}%", r.score)).size(24.0).color(if r.score >= 85.0 { GOOD } else { BAD }));
                });
                ui.label(if ok {
                    RichText::new("🎯 목표 달성! 한 번 더 더 빠르게 읽거나 새 지문에 도전하세요.").color(GOOD)
                } else if r.score < 85.0 {
                    RichText::new("속도보다 정확도가 먼저예요. 빨간 단어를 듣고 다시 읽어 보세요.").color(warn())
                } else {
                    RichText::new(format!("정확해요! 이제 분당 {:.0}자까지 속도를 올려 봅시다 — 문장 사이 쉬는 시간을 줄이세요.", target)).color(warn())
                });
                if self.speed.attempts.len() > 1 {
                    let trail: Vec<String> = self.speed.attempts.iter().enumerate().map(|(i, (w, a))| format!("{}회 {w:.0}({a:.0}%)", i + 1)).collect();
                    ui.label(RichText::new(format!("반복 읽기: {}", trail.join(" → "))).color(muted()));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("↻ 같은 지문 더 빠르게").clicked() {
                    self.speed.last = None;
                }
                match self.speed.book {
                    // A book: the next page is the next passage.
                    Some((bi, page)) => {
                        let pages = self.books().get(bi).map(|b| b.pages.len()).unwrap_or(0);
                        if page + 1 < pages && ui.button("다음 쪽 ▶").clicked() {
                            self.open_book_page(bi, page + 1);
                        }
                    }
                    None => {}
                }
            });
        }
    }

    fn start_quick_response(&mut self) {
        let mut pool: Vec<(String, String)> = self
            .store
            .items
            .iter()
            .filter(|i| !i.meaning_ko.is_empty())
            .map(|i| (i.text.clone(), i.meaning_ko.clone()))
            .collect();
        if pool.len() < 5 {
            if let Some(l) = &self.lesson {
                pool.extend(l.sentences.iter().map(|s| (s.text.clone(), s.ko.clone())));
            }
            pool.extend(content::placement().iter().take(4).map(|(e, k)| (e.to_string(), k.to_string())));
        }
        pool.shuffle(&mut rand::rng());
        pool.truncate(10);
        self.speed.queue = pool;
        self.speed.idx = 0;
        self.speed.stats.clear();
        self.speed.result = None;
        self.speed.shown_at = None;
    }

    fn quick_response_ui(&mut self, ui: &mut Ui) {
        let pass = self.pass_score();
        let t_done = crate::art::since_event(ui, egui::Id::new("cheer_quick_response"), self.store.profile.speed_history.iter().filter(|s| s.kind == "response").count() as u64);
        card(ui, |ui| {
            ui.label("한국어가 나오면 마이크가 자동으로 켜집니다. 생각나는 즉시 일본어로 말하세요 — 말이 끝나면 자동으로 채점돼요.");
            ui.label(RichText::new("배운 단어·문장에서 출제됩니다. 제한 시간 안에 정확히 말해야 통과.").color(muted()));
        });
        ui.add_space(6.0);
        if self.speed.queue.is_empty() {
            if ui.add(egui::Button::new(RichText::new("▶ 시작 (10문제)").size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
                self.start_quick_response();
            }
            return;
        }
        if self.speed.idx >= self.speed.queue.len() {
            let n = self.speed.stats.len().max(1) as f32;
            let avg_t = self.speed.stats.iter().map(|s| s.0).sum::<f32>() / n;
            let avg_a = self.speed.stats.iter().map(|s| s.1).sum::<f32>() / n;
            let ok = self.speed.stats.iter().filter(|s| s.2).count();
            // The round's end: the same celebration as a finished lesson.
            let total = self.speed.stats.len().max(1);
            let score = 100.0 * ok as f32 / total as f32;
            crate::art::cheer(ui, t_done, score, if score >= 90.0 { "🎉 완벽해요! 번개 같아요!" } else { "🎉 잘했어요!" }, &format!("빠르게 말하기 {ok}/{total} 통과 · 평균 {avg_t:.1}초"));
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.label(RichText::new(format!("💨 {ok}/{} 통과", self.speed.stats.len())).size(28.0).strong());
                ui.label(format!("평균 반응 시간 {avg_t:.1}초 · 평균 정확도 {avg_a:.0}%"));
            });
            if ui.button("↻ 한 번 더").clicked() {
                self.start_quick_response();
            }
            if ui.button("닫기").clicked() {
                self.speed.queue.clear();
            }
            return;
        }
        let (target, ko) = self.speed.queue[self.speed.idx].clone();
        let limit = time_limit(&target);
        if self.speed.shown_at.is_none() {
            self.speed.shown_at = Some(Instant::now());
            self.speed.rcap = Capture::auto_stopping();
            self.speed.rcap.start_now(&mut self.voice);
        }
        let shown = self.speed.shown_at.unwrap();
        ui.label(RichText::new(format!("{} / {}", self.speed.idx + 1, self.speed.queue.len())).color(muted()));
        let mut next = false;
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                crate::grammar::ko_label(ui, &ko, 28.0, text_strong());
                ui.add_space(6.0);
                if self.speed.result.is_none() {
                    let el = shown.elapsed().as_secs_f32();
                    let frac = (1.0 - el / limit).max(0.0);
                    ui.add(egui::ProgressBar::new(frac).desired_width(320.0).fill(if frac > 0.3 { ACCENT } else { BAD }).text(format!("{:.1}초", (limit - el).max(0.0))));
                    ui.ctx().request_repaint();
                }
                if let Some(t) = self.speed.rcap.ui(ui, &mut self.voice, limit + 6.0) {
                    let secs = self.speed.rcap.stopped_at.map(|s| (s - shown).as_secs_f32()).unwrap_or(limit);
                    let r = scoring::score(&target, &t);
                    let ok = r.score >= pass - 10.0 && secs <= limit;
                    self.store.profile.today.scores.push(r.score);
                    self.store.log_attempt("quick-response", &target, &t, r.score);
                    self.speed.stats.push((secs, r.score, ok));
                    self.speed.result = Some((t, r, secs, ok));
                }
                if let Some((heard, r, secs, ok)) = &self.speed.result {
                    word_feedback(ui, r, 24.0);
                    ui.label(RichText::new(format!("들린 말: \"{heard}\"")).color(muted()));
                    ui.label(
                        RichText::new(format!("{} · {:.1}초 (제한 {:.1}초) · 정확도 {:.0}%", if *ok { "✅ 통과" } else { "❌ 다시 연습" }, secs, limit, r.score))
                            .size(18.0)
                            .color(if *ok { GOOD } else { warn() }),
                    );
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 200.0).max(0.0) / 2.0);
                        if ui.button("🔊 정답 듣기").clicked() {
                            self.voice.say(&target);
                        }
                        if ui.add(egui::Button::new("다음 ▶").fill(ACCENT)).clicked() {
                            next = true;
                        }
                    });
                }
            });
        });
        if self.speed.result.is_some() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            next = true;
        }
        if next {
            self.speed.idx += 1;
            self.speed.result = None;
            self.speed.shown_at = None;
            if self.speed.idx >= self.speed.queue.len() {
                let n = self.speed.stats.len().max(1) as f32;
                let avg_t = self.speed.stats.iter().map(|s| s.0).sum::<f32>() / n;
                let avg_a = self.speed.stats.iter().map(|s| s.1).sum::<f32>() / n;
                self.store.profile.speed_history.push(SpeedRecord { date: today(), kind: "response".into(), wpm: 0.0, accuracy: avg_a, seconds: avg_t, title: String::new() });
                self.speed_round_done();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_scale_with_level() {
        assert!(target_wpm(2.0) < target_wpm(8.0));
        assert!(target_wpm(10.0) <= 360.0);
        assert!(time_limit("I'm fine") < time_limit("Could you send me the report by Friday afternoon?"));
    }
}
