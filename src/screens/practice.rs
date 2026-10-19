//! Drill queues: new words, reading aloud, spaced-repetition review and phrase practice.
//! Each item is a `Drill` the learner must say out loud N times; results feed the SRS.

use crate::app::App;
use crate::models::*;
use crate::srs;
use crate::widgets::*;
use egui::{Color32, RichText, Ui};

#[derive(Clone)]
pub struct Spec {
    /// Existing SRS item being reviewed.
    pub item_id: Option<String>,
    /// Item to add to the SRS once practiced.
    pub new_item: Option<StudyItem>,
    pub target: String,
    pub ko: String,
    pub tip: String,
    pub reps: u32,
    pub recall: bool,
    pub is_word: bool,
    pub chunks: Vec<Chunk>,
}

pub struct DrillQueue {
    /// Step completed when the queue is finished ("" = none).
    pub step: &'static str,
    pub category: &'static str,
    pub title: String,
    pub specs: Vec<Spec>,
    pub idx: usize,
    pub drill: Drill,
    pub results: Vec<(String, f32)>,
    /// When the celebration started (egui time), so it plays once from the beginning.
    pub finished_at: Option<f64>,
    /// The rest of the list after a clicked row, as (title, drills) per row: the celebration's
    /// "다음 단어 / 다음 문장" practises the first of these next, one row at a time.
    pub then: Vec<(String, Vec<Spec>)>,
}

impl DrillQueue {
    pub fn new(step: &'static str, category: &'static str, title: &str, specs: Vec<Spec>) -> Self {
        let drill = specs.first().map(make_drill).unwrap_or_default();
        Self { step, category, title: title.to_string(), specs, idx: 0, drill, results: Vec::new(), finished_at: None, then: Vec::new() }
    }

    pub fn finished(&self) -> bool {
        self.idx >= self.specs.len()
    }
}

/// What a 새 단어 entry is (decides how it is saved and drilled).
#[derive(Clone, Copy, PartialEq, Debug)]
enum WordKind {
    Chunk,
    Word,
    Expression,
}

/// Rows per page in today's lists.
const LIST_PAGE: usize = 25;

/// Colour for a 새 단어 entry by passing repetitions: red (not yet) → deep orange → deep
/// yellow-green while repeating → light green once memorized (10+).
pub fn word_color(passes: u32) -> Color32 {
    match passes {
        0 => Color32::from_rgb(240, 80, 80),
        p if p >= WORD_DONE_PASSES => Color32::from_rgb(150, 230, 90),
        p => {
            let t = (p - 1) as f32 / (WORD_DONE_PASSES - 2) as f32;
            let (a, b) = ([225.0, 105.0, 20.0], [110.0, 170.0, 30.0]);
            let c = |i: usize| (a[i] + (b[i] - a[i]) * t).round() as u8;
            Color32::from_rgb(c(0), c(1), c(2))
        }
    }
}

/// Colour and label ("미완료", "3/10회", "✓ 완료") for a 새 단어 entry.
pub fn word_mark(passes: u32) -> (Color32, String) {
    let label = match passes {
        0 => "미완료".to_string(),
        p if p >= WORD_DONE_PASSES => "✓ 완료".to_string(),
        p => format!("{p}/{WORD_DONE_PASSES}회"),
    };
    (word_color(passes), label)
}

/// Korean shown, English said from memory (once) — retrieval practice at the end of a set.
fn recall_spec(target: &str, ko: &str) -> Spec {
    Spec {
        item_id: None,
        new_item: None,
        target: target.to_string(),
        ko: ko.to_string(),
        tip: "💡 한국어만 보고 영어로 말해 보세요".into(),
        reps: 1,
        recall: true,
        is_word: false,
        chunks: Vec::new(),
    }
}

fn make_drill(s: &Spec) -> Drill {
    Drill::new(&s.target, &s.ko, &s.tip, s.reps, s.recall, s.is_word).with_chunks(s.chunks.clone())
}

impl App {
    /// Runs the active queue. Returns true while a queue is being shown.
    pub fn run_queue_ui(&mut self, ui: &mut Ui) -> bool {
        let pass = self.pass_score();
        let claude = self.claude();
        let Some(q) = self.queue.as_mut() else { return false };

        if q.finished() {
            let step = q.step;
            let avg = if q.results.is_empty() { 0.0 } else { q.results.iter().map(|r| r.1).sum::<f32>() / q.results.len() as f32 };
            let weak: Vec<String> = q.results.iter().filter(|r| r.1 < pass).map(|r| r.0.clone()).collect();
            let title = q.title.clone();
            let has_next = !q.then.is_empty();
            let q_category = q.category;
            let now = ui.input(|i| i.time);
            let t = (now - *q.finished_at.get_or_insert(now)) as f32;
            // Animated celebration, with the title and score as real labels on top of it.
            let headline = if avg >= 90.0 { "🎉 완벽해요! 잘했어요!" } else { "🎉 잘했어요!" };
            let shown = avg * (t / 1.2).min(1.0);
            crate::art::cheer(ui, t, avg, headline, &format!("{title} 완료 · 첫 시도 평균 {shown:.0}점"));
            ui.add_space(8.0);
            if !weak.is_empty() {
                card(ui, |ui| {
                    ui.label(RichText::new("어려웠던 항목 (복습·숙제로 다시 나옵니다)").color(warn()));
                    for w in &weak {
                        ui.label(format!("• {w}"));
                    }
                });
            }
            ui.add_space(8.0);
            let (mut close, mut go_next) = (false, false);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("🏠 홈으로").size(16.0)).fill(ACCENT).min_size(crate::widgets::ACTION_BUTTON)).clicked() {
                    close = true;
                    self.screen = crate::app::Screen::Home;
                }
                if ui.button("닫기").clicked() {
                    close = true;
                }
                let next_label = if q_category == "vocab" { "다음 단어 ▶" } else { "다음 문장 ▶" };
                if has_next && ui.add(egui::Button::new(RichText::new(next_label).size(16.0)).min_size(crate::widgets::ACTION_BUTTON)).clicked() {
                    go_next = true;
                }
            });
            if close {
                self.queue = None;
            } else if go_next {
                if let Some(q) = self.queue.take() {
                    let mut rest = q.then;
                    let (title, specs) = rest.remove(0);
                    crate::applog!("다음 항목: '{title}'");
                    let mut next = DrillQueue::new("", q.category, &title, specs);
                    next.then = rest;
                    self.queue = Some(next);
                }
            }
            if !step.is_empty() && !self.step_done(step) {
                self.complete_step(step);
            }
            return true;
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(&q.title).size(20.0).strong());
            ui.label(RichText::new(format!("{} / {}", q.idx + 1, q.specs.len())).color(muted()));
        });
        ui.add(egui::ProgressBar::new(q.idx as f32 / q.specs.len() as f32).desired_height(6.0).fill(ACCENT));
        ui.add_space(10.0);
        let event = card(ui, |ui| q.drill.ui(ui, &mut self.voice, pass, claude.as_ref()));
        if let Some((text, chunks)) = q.drill.fresh_gloss.take() {
            q.specs[q.idx].chunks = chunks.clone();
            self.store.put_gloss(&text, &chunks);
        }
        let Some(q) = self.queue.as_mut() else { return true };
        let mut stop = false;
        ui.horizontal(|ui| {
            if ui.button("건너뛰기").clicked() {
                q.idx += 1;
                if let Some(s) = q.specs.get(q.idx) {
                    q.drill = make_drill(s);
                }
            }
            stop = ui.button("그만하기").clicked();
        });
        if stop {
            self.queue = None;
            return true;
        }
        match event {
            DrillEvent::Attempt(a) => {
                let (cat, target) = (q.category, q.drill.target.clone());
                self.record_attempt(cat, &target, &a);
            }
            DrillEvent::Completed => {
                let first = q.drill.first_score.unwrap_or(0.0);
                q.results.push((q.drill.target.clone(), first));
                let spec_item = q.specs[q.idx].item_id.clone();
                let new_item = q.specs[q.idx].new_item.take();
                if q.step == "words_review" {
                    if let Some(id) = &spec_item {
                        self.store.profile.today.words_reviewed.push(id.clone());
                    }
                }
                q.idx += 1;
                if let Some(s) = q.specs.get(q.idx) {
                    q.drill = make_drill(s);
                }
                self.apply_result(spec_item, new_item, first);
            }
            DrillEvent::None => {}
        }
        true
    }

    /// Feeds a finished drill into spaced repetition.
    pub fn apply_result(&mut self, item_id: Option<String>, new_item: Option<StudyItem>, first_score: f32) {
        let pass = self.pass_score();
        let items = &mut self.store.items;
        let idx = match (new_item, item_id) {
            (Some(item), _) => Some(srs::add_item(items, item)),
            (None, Some(id)) => items.iter().position(|i| i.id == id),
            _ => None,
        };
        if let Some(i) = idx {
            srs::review(&mut items[i], first_score, pass);
            self.store.save_items();
        }
    }

    /// Today's list (25 per page) and the start button. `marks` (same order as `rows`, may be
    /// empty) colours each row by how far it has been practised.
    /// Returns (start pressed, row clicked, row marked 학습완료); rows are clickable when `marks`
    /// are given, and unfinished rows get a 학습완료 button when `can_finish`.
    fn intro_card(&mut self, ui: &mut Ui, title: &str, desc: &str, rows: &[(String, String)], step: &str, marks: &[(Color32, String)], can_finish: bool) -> (bool, Option<usize>, Option<usize>) {
        let mut start = false;
        let mut picked = None;
        let mut finished = None;
        page_title(ui, title, desc);
        ui.add_space(8.0);
        let done_today = self.step_done(step);
        let label = if done_today { "↻ 다시 연습하기" } else { "▶ 시작하기" };
        let start_button = |ui: &mut Ui| ui.add(egui::Button::new(RichText::new(label).size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked();
        // The start button above the list too, so a long list need not be scrolled through first.
        ui.horizontal(|ui| {
            start |= start_button(ui);
            if done_today {
                ui.label(RichText::new("✅ 오늘 완료했어요").color(GOOD));
            }
        });
        ui.add_space(8.0);
        let pages = rows.len().div_ceil(LIST_PAGE).max(1);
        let page_id = egui::Id::new(("intro_page", step));
        let mut page = ui.data(|d| d.get_temp::<usize>(page_id)).unwrap_or(0).min(pages - 1);
        card(ui, |ui| {
            if !marks.is_empty() {
                let done = marks.iter().filter(|m| m.1.starts_with('✓')).count();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("기억 완료 {done}/{}", rows.len())).strong());
                    ui.label(RichText::new("■ 미완료").color(word_color(0)));
                    ui.label(RichText::new("■ 반복 중").color(word_color(1)));
                    ui.label(RichText::new("■").color(word_color(5)));
                    ui.label(RichText::new("■").color(word_color(9)));
                    ui.label(RichText::new(format!("■ 완료({WORD_DONE_PASSES}회 이상)")).color(word_color(WORD_DONE_PASSES)));
                });
                ui.add_space(6.0);
            }
            for (i, (en, ko)) in rows.iter().enumerate().skip(page * LIST_PAGE).take(LIST_PAGE) {
                if i > page * LIST_PAGE {
                    ui.add_space(8.0);
                }
                let mark = marks.get(i);
                // Wrapped so a long sentence + translation stays inside the card.
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, en);
                    let color = mark.map(|m| m.0).unwrap_or(text_strong());
                    let text = RichText::new(en).size(self.store.settings.list_size()).strong().color(color);
                    if mark.is_some() {
                        // Click a word to practise just that one right away.
                        let r = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text("클릭하면 이것만 바로 연습해요");
                        if r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            picked = Some(i);
                        }
                    } else {
                        ui.label(text);
                    }
                    if let Some((c, text)) = mark {
                        ui.label(RichText::new(text).small().color(*c));
                        if can_finish && !text.starts_with('✓') && finish_button(ui) {
                            finished = Some(i);
                        }
                    }
                    // Months 5-6: English only; the Korean is a hover away.
                    if support_stage(ui.ctx()) < 2 {
                        crate::grammar::ko_label(ui, ko, 16.0, muted());
                    } else {
                        ui.label(RichText::new("뜻").small().color(muted())).on_hover_text(ko);
                    }
                });
            }
            if pages > 1 {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(page > 0, egui::Button::new("◀ 이전")).clicked() {
                        page -= 1;
                    }
                    ui.label(format!("{} / {pages} 쪽 ({}개 중 {}~{})", page + 1, rows.len(), page * LIST_PAGE + 1, ((page + 1) * LIST_PAGE).min(rows.len())));
                    if ui.add_enabled(page + 1 < pages, egui::Button::new("다음 ▶")).clicked() {
                        page += 1;
                    }
                });
            }
        });
        ui.data_mut(|d| d.insert_temp(page_id, page));
        ui.add_space(8.0);
        if done_today {
            ui.label(RichText::new("✅ 오늘 완료했어요. 더 연습해도 좋아요!").color(GOOD));
        }
        start |= start_button(ui);
        (start, picked, finished)
    }

    pub fn words_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) || self.word_mission_ui(ui) {
            return;
        }
        self.ensure_tenses();
        // Built-in word database + chunk list: no waiting for Claude. The rest of today's
        // lesson (sentences, speaking…) is still prepared in the background meanwhile.
        self.ensure_lesson();
        let lesson = self.word_set();
        // Chunks first: whole spoken phrases pay off in conversation sooner than single words.
        let rows: Vec<(String, String)> = lesson
            .phrases
            .iter()
            .map(|w| (format!("🔗 {}", w.word), format!("{} — {}", w.meaning_ko, w.example)))
            .chain(lesson.words.iter().map(|w| (w.word.clone(), format!("{} — {}", w.meaning_ko, w.example))))
            .chain(lesson.expressions.iter().map(|w| (format!("💬 {}", w.word), format!("{} — {}", w.meaning_ko, w.example))))
            .collect();
        // Practice so far per entry (word_stats.json): red → orange → green as passes add up.
        let marks: Vec<(Color32, String)> = lesson
            .phrases
            .iter()
            .chain(&lesson.words)
            .chain(&lesson.expressions)
            .map(|w| word_mark(self.store.word_stat(&w.word).passes))
            .collect();
        let reps = self.store.settings.reps_new;
        let (start, picked, finished) = self.intro_card(
            ui,
            "📚 오늘의 덩어리 표현 · 새 단어",
            &format!("🔗 덩어리 표현을 먼저, 통째로 익혀요. 듣고 → 소리 내어 {reps}번 → 시제 3문장(현재 ~할까 생각 중이에요 · 과거 ~했어요 · 미래 ~할 거예요)을 3번씩 말하면 저장되고 복습 일정이 잡힙니다. 단어를 클릭하면 그 단어만 바로 연습해요."),
            &rows,
            "words",
            &marks,
            true,
        );
        // Same order as `rows`.
        let entries: Vec<(&LessonWord, WordKind)> = lesson
            .phrases
            .iter()
            .map(|w| (w, WordKind::Chunk))
            .chain(lesson.words.iter().map(|w| (w, WordKind::Word)))
            .chain(lesson.expressions.iter().map(|w| (w, WordKind::Expression)))
            .collect();
        if let Some((w, kind)) = finished.and_then(|i| entries.get(i).copied()) {
            self.mark_learned(w, kind);
            return;
        }
        if let Some((w, kind)) = picked.and_then(|i| entries.get(i).copied()) {
            // One entry on demand: full repetitions + example, even if it was passed before
            // (the passes still count toward 기억 완료; the review schedule isn't touched twice).
            let specs = self.entry_specs(w, kind, reps, true);
            crate::applog!("words: 리스트에서 '{}' 바로 연습", w.word);
            let mut q = DrillQueue::new("", "vocab", &format!("새 단어 · {}", w.word), specs);
            // "다음 단어" then goes on down the list.
            let i = picked.unwrap_or(0);
            q.then = entries[i + 1..].iter().map(|(w, kind)| (format!("새 단어 · {}", w.word), self.entry_specs(w, *kind, reps, true))).collect();
            self.queue = Some(q);
            return;
        }
        if start {
            let mut specs = Vec::new();
            for (w, kind) in &entries {
                // Names that slip into a lesson ("Seoul") are not drilled as vocabulary.
                if *kind == WordKind::Word && is_proper_noun(&w.word, &w.meaning_ko) || self.is_known(&w.word, *kind) {
                    continue;
                }
                specs.extend(self.entry_specs(w, *kind, reps, false));
            }
            // Then each chunk once more from the Korean alone.
            specs.extend(
                lesson.phrases.iter().filter(|w| !self.entry_passed(&w.word, WordKind::Chunk)).map(|w| recall_spec(&w.word, &w.meaning_ko)),
            );
            self.queue = Some(DrillQueue::new("words", "vocab", "새 단어", specs));
            return;
        }
        self.sentence_making_card(ui);
        self.past_unfinished_ui(ui, reps);
        let days: Vec<(chrono::NaiveDate, Vec<DayRow>)> = self
            .word_days_list(true)
            .into_iter()
            .map(|(d, v)| {
                let rows = v
                    .iter()
                    .map(|(w, k)| DayRow {
                        label: match k {
                            WordKind::Chunk => format!("🔗 {}", w.word),
                            WordKind::Expression => format!("💬 {}", w.word),
                            WordKind::Word => w.word.clone(),
                        },
                        ko: w.meaning_ko.clone(),
                        specs: self.entry_specs(w, *k, reps, true),
                    })
                    .collect();
                (d, rows)
            })
            .collect();
        self.day_lists_ui(ui, "words", "📚 날짜별 전체 단어", "새 단어", &days);
        // Then up to 50 learned words, said once each (Order.md: 하루 새 단어 35 + 배운 단어 50).
        ui.add_space(14.0);
        let ids = self.word_review_ids();
        let done = self.store.profile.today.words_reviewed.len();
        let goal = crate::wordbank::REVIEW_PER_DAY;
        let mut start_review = false;
        card(ui, |ui| {
            ui.label(RichText::new(format!("🔁 배운 단어 복습  {}/{goal}", done.min(goal))).size(18.0).strong());
            if ids.is_empty() {
                let msg = if done >= goal { "오늘 복습을 다 했어요!" } else { "복습할 배운 단어가 아직 없어요. 새 단어를 배우면 내일부터 쌓입니다." };
                ui.label(RichText::new(msg).color(muted()));
            } else {
                ui.label(RichText::new("이미 배운 단어를 한 번씩 소리 내어 말해요. 첫 시도 점수로 다음 복습일이 정해집니다.").color(muted()));
                ui.add_space(6.0);
                start_review = ui.add(egui::Button::new(RichText::new(format!("▶ 배운 단어 {}개 복습", ids.len())).size(16.0)).min_size(crate::widgets::ACTION_BUTTON)).clicked();
            }
        });
        if start_review {
            let specs: Vec<Spec> = ids
                .iter()
                .filter_map(|id| self.store.items.iter().find(|i| &i.id == id))
                .map(|i| Spec {
                    item_id: Some(i.id.clone()),
                    new_item: None,
                    target: i.text.clone(),
                    ko: i.meaning_ko.clone(),
                    tip: "🔁 배운 단어 복습".into(),
                    reps: 1,
                    recall: false,
                    is_word: true,
                    chunks: Vec::new(),
                })
                .collect();
            self.queue = Some(DrillQueue::new("words_review", "vocab", "배운 단어 복습", specs));
        }
    }

    /// The week's curriculum line, theme and grammar for 오늘의 문장 만들기.
    fn week_focus(&self) -> (u32, &'static crate::curriculum::Week) {
        let n = crate::curriculum::week_number(&self.store.profile);
        (n, crate::curriculum::week(n))
    }

    /// ✏ 오늘의 문장 만들기 card: the day's 5 focus words to use in sentences of my own, on the
    /// week's theme and grammar.
    fn sentence_making_card(&mut self, ui: &mut Ui) {
        let words = self.focus_words();
        if words.is_empty() {
            return;
        }
        let (n, wk) = self.week_focus();
        let made = self.store.profile.today.sentences_made.clone();
        let done = words.iter().filter(|w| made.contains(&w.word)).count();
        let mut start = false;
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("✏ 오늘의 문장 만들기  {done}/{}", words.len())).size(18.0).strong());
                chip(ui, &format!("{n}주차 · {}", wk.theme_ko), crate::art::VIOLET);
                chip(ui, &format!("문법: {}", wk.grammar), ACCENT);
            });
            ui.label(RichText::new(format!("오늘 단어를 넣어 '{}' 주제로 내 문장을 만들어 말해요. 이번 주 문법({})을 써 보면 더 좋아요.", wk.theme_ko, wk.grammar)).color(muted()));
            ui.horizontal_wrapped(|ui| {
                for w in &words {
                    let on = made.contains(&w.word);
                    chip(ui, &format!("{} {}", if on { "✓" } else { "✏" }, w.word), if on { GOOD } else { warn() });
                }
            });
            ui.add_space(4.0);
            let label = if done >= words.len() { "↻ 문장 더 만들기" } else { "▶ 문장 만들기 시작" };
            start = ui.add(egui::Button::new(RichText::new(label).size(17.0)).fill(if done >= words.len() { subtle_fill() } else { ACCENT }).min_size(crate::widgets::BIG_BUTTON)).clicked();
            if done >= words.len() {
                ui.label(RichText::new("✅ 오늘 단어로 문장 5개를 모두 만들었어요!").color(GOOD));
            }
        });
        if start {
            let next = words.iter().find(|w| !made.contains(&w.word)).or(words.first()).map(|w| w.word.clone());
            if let Some(w) = next {
                crate::applog!("words: 문장 만들기 시작 · {w}");
                self.word_mission = Some((w, Default::default()));
            }
        }
    }

    /// Runs 오늘의 문장 만들기 when it is open. Returns true while it is shown.
    fn word_mission_ui(&mut self, ui: &mut Ui) -> bool {
        let Some((word, mut m)) = self.word_mission.take() else { return false };
        let words = self.focus_words();
        let Some(w) = words.iter().find(|x| x.word == word).cloned() else { return false };
        let (n, wk) = self.week_focus();
        let made = &self.store.profile.today.sentences_made;
        let pos = words.iter().position(|x| x.word == word).unwrap_or(0);
        ui.label(RichText::new(format!("단어 {}/{} · 완료 {}", pos + 1, words.len(), words.iter().filter(|x| made.contains(&x.word)).count())).color(muted()));
        let spec = crate::screens::mission::MissionSpec {
            word: w.word.clone(),
            meaning_ko: w.meaning_ko.clone(),
            week: Some((format!("{n}주차 · {} ({}) · 문법: {}", wk.theme_ko, wk.theme, wk.grammar), wk.theme.to_string(), wk.grammar.to_string())),
            starters: crate::curriculum::STARTERS[(n.clamp(1, 26) - 1) as usize].iter().map(|s| s.to_string()).collect(),
            already_passed: false,
        };
        let ev = self.mission_ui(ui, &mut m, &spec);
        let mut close = ui.button("◀ 새 단어 목록").clicked();
        match ev {
            Some(crate::screens::mission::MissionEvent::Checked(res)) if res.ok => {
                if !self.store.profile.today.sentences_made.contains(&w.word) {
                    self.store.profile.today.sentences_made.push(w.word.clone());
                    self.store.record_word_practice(&w.word, true);
                    self.store.save_profile();
                }
            }
            Some(crate::screens::mission::MissionEvent::Next) => {
                let made = &self.store.profile.today.sentences_made;
                match words.iter().skip(pos + 1).chain(words.iter()).find(|x| !made.contains(&x.word)) {
                    Some(x) => {
                        self.word_mission = Some((x.word.clone(), Default::default()));
                        return true;
                    }
                    None => {
                        self.toast("🎉 오늘의 문장 만들기 완료! 자기 표현·회화에서도 오늘 단어를 써 보세요");
                        close = true;
                    }
                }
            }
            _ => {}
        }
        if !close {
            self.word_mission = Some((word, m));
        }
        true
    }

    /// Earlier days' 새 단어 entries not finished yet (not saved to the deck), by day, oldest first.
    fn past_unfinished(&self) -> Vec<(chrono::NaiveDate, Vec<(LessonWord, WordKind)>)> {
        self.word_days_list(false)
    }

    /// Each day's 새 단어 list up to today, all entries (`all`) or just the unfinished ones of
    /// earlier days.
    fn word_days_list(&self, all: bool) -> Vec<(chrono::NaiveDate, Vec<(LessonWord, WordKind)>)> {
        let until = if all { today() + chrono::Duration::days(1) } else { today() };
        self.store
            .word_days
            .range(..until)
            .map(|(d, s)| {
                let left = s
                    .phrases
                    .iter()
                    .map(|w| (w, WordKind::Chunk))
                    .chain(s.words.iter().map(|w| (w, WordKind::Word)))
                    .chain(s.expressions.iter().map(|w| (w, WordKind::Expression)))
                    .filter(|(w, k)| all || !self.entry_passed(&w.word, *k))
                    .map(|(w, k)| (w.clone(), k))
                    .collect();
                (*d, left)
            })
            .filter(|(_, v): &(_, Vec<_>)| !v.is_empty())
            .collect()
    }

    /// 📅 지난 날 못 끝낸 새 단어: earlier days' unfinished words, coloured and clickable like
    /// today's list, with one button to go through all of them.
    fn past_unfinished_ui(&mut self, ui: &mut Ui, reps: u32) {
        let days = self.past_unfinished();
        if days.is_empty() {
            return;
        }
        let total: usize = days.iter().map(|d| d.1.len()).sum();
        let start = self.store.profile.start_date;
        let mut picked: Option<(usize, usize)> = None;
        let mut finished: Option<(usize, usize)> = None;
        let mut all = false;
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("📅 지난 날 못 끝낸 새 단어 {total}개")).size(18.0).strong());
            ui.label(RichText::new("전에 나왔지만 아직 통과하지 못한 단어예요. 클릭하면 그 단어를 바로 연습해요.").color(muted()));
            for (di, (date, left)) in days.iter().enumerate() {
                let day_no = (*date - start).num_days() + 1;
                egui::CollapsingHeader::new(RichText::new(format!("{day_no}일차 · {} — {}개", date.format("%m월 %d일"), left.len())).strong())
                    .id_salt(("past_words", *date))
                    .default_open(di + 1 == days.len())
                    .show(ui, |ui| {
                        for (wi, (w, kind)) in left.iter().enumerate() {
                            let (color, mark) = word_mark(self.store.word_stat(&w.word).passes);
                            ui.horizontal_wrapped(|ui| {
                                speaker_button(ui, &mut self.voice, &w.word);
                                let label = match kind {
                                    WordKind::Chunk => format!("🔗 {}", w.word),
                                    WordKind::Expression => format!("💬 {}", w.word),
                                    WordKind::Word => w.word.clone(),
                                };
                                let r = ui.add(egui::Label::new(RichText::new(label).size(self.store.settings.list_size()).strong().color(color)).sense(egui::Sense::click())).on_hover_text("클릭하면 이것만 바로 연습해요");
                                if r.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if r.clicked() {
                                    picked = Some((di, wi));
                                }
                                ui.label(RichText::new(mark).small().color(color));
                                if finish_button(ui) {
                                    finished = Some((di, wi));
                                }
                                crate::grammar::ko_label(ui, &w.meaning_ko, 15.0, muted());
                            });
                        }
                    });
            }
            ui.add_space(6.0);
            all = ui.add(egui::Button::new(RichText::new(format!("▶ 지난 단어 {total}개 이어서 연습")).size(16.0)).min_size(crate::widgets::ACTION_BUTTON)).clicked();
        });
        let flat: Vec<(LessonWord, WordKind)> = days.iter().flat_map(|d| d.1.iter().cloned()).collect();
        if let Some((di, wi)) = finished {
            let (w, kind) = days[di].1[wi].clone();
            self.mark_learned(&w, kind);
        } else if let Some((di, wi)) = picked {
            let skip: usize = days[..di].iter().map(|d| d.1.len()).sum::<usize>() + wi;
            let (w, kind) = &flat[skip];
            let mut q = DrillQueue::new("", "vocab", &format!("새 단어 · {}", w.word), self.entry_specs(w, *kind, reps, true));
            q.then = flat[skip + 1..].iter().map(|(w, k)| (format!("새 단어 · {}", w.word), self.entry_specs(w, *k, reps, true))).collect();
            crate::applog!("words: 지난 날 단어 '{}' 바로 연습", w.word);
            self.queue = Some(q);
        } else if all {
            let specs = flat.iter().flat_map(|(w, k)| self.entry_specs(w, *k, reps, false)).collect();
            self.queue = Some(DrillQueue::new("", "vocab", "지난 날 새 단어", specs));
        }
    }

    /// 학습완료: a word too easy to drill counts as learned (deck + 기억 완료) and is never
    /// reviewed.
    fn mark_learned(&mut self, w: &LessonWord, kind: WordKind) {
        let item = self.entry_specs(w, kind, 1, true).into_iter().find_map(|s| s.new_item);
        self.mark_done(item, &w.word);
    }

    /// Saves `item` (if not in the deck yet) as learned without review, and counts `text` as
    /// fully practised.
    fn mark_done(&mut self, item: Option<StudyItem>, text: &str) {
        if let Some(mut item) = item {
            item.known = true;
            let i = srs::add_item(&mut self.store.items, item);
            let far = today() + chrono::Duration::days(3650);
            let it = &mut self.store.items[i];
            it.known = true;
            it.due = far;
            it.interval_days = 3650;
            self.store.save_items();
        }
        self.store.mark_word_done(text);
        crate::applog!("'{text}' 학습완료 (쉬운 항목)");
        self.toast(format!("✓ '{}' 학습완료", text.chars().take(40).collect::<String>()));
    }

    fn is_known(&self, text: &str, kind: WordKind) -> bool {
        let id = srs::make_id(if kind == WordKind::Chunk { ItemKind::Sentence } else { ItemKind::Word }, text);
        self.store.items.iter().any(|i| i.id == id && i.known)
    }

    /// Earlier days' 문장 읽기 sentences (idioms, then the lesson's sentences) not finished yet,
    /// by day, oldest first, as (row label, Korean, drill).
    fn past_reading_unfinished(&self, reps: u32) -> Vec<(chrono::NaiveDate, Vec<(String, String, Spec)>)> {
        self.reading_days_list(reps, false)
    }

    /// Each day's 문장 읽기 list (idioms + the lesson's sentences) up to today, all of them
    /// (`all`) or just the unfinished ones of earlier days.
    fn reading_days_list(&self, reps: u32, all: bool) -> Vec<(chrono::NaiveDate, Vec<(String, String, Spec)>)> {
        let in_deck = |t: &str| {
            let id = srs::make_id(ItemKind::Sentence, t);
            !all && self.store.items.iter().any(|i| i.id == id)
        };
        let mut lessons = self.store.past_lessons(400);
        if all {
            lessons.extend(self.lesson.clone().filter(|l| l.date == Some(today())));
        }
        let until = if all { today() + chrono::Duration::days(1) } else { today() };
        let mut dates: Vec<chrono::NaiveDate> = self.store.idiom_days.range(..until).map(|(d, _)| *d).chain(lessons.iter().filter_map(|l| l.date)).filter(|d| *d < until).collect();
        dates.sort();
        dates.dedup();
        dates
            .into_iter()
            .map(|d| {
                let idioms = self.store.idiom_days.get(&d).cloned().unwrap_or_default();
                let mut left: Vec<(String, String, Spec)> = idioms
                    .iter()
                    .filter_map(|n| crate::idioms::by_no(*n))
                    .filter(|i| !in_deck(&i.en))
                    .map(|i| {
                        let spec = Spec {
                            item_id: None,
                            new_item: Some(StudyItem {
                                kind: ItemKind::Sentence,
                                text: i.en.clone(),
                                meaning_ko: i.ko.clone(),
                                example: i.also.first().cloned().unwrap_or_default(),
                                category: "idiom".into(),
                                ..Default::default()
                            }),
                            target: i.en.clone(),
                            ko: i.ko.clone(),
                            tip: i.also.first().map(|a| format!("💬 이렇게도 말해요: {a}")).unwrap_or_else(|| "💬 숙어 문장".into()),
                            reps,
                            recall: false,
                            is_word: false,
                            chunks: Vec::new(),
                        };
                        (format!("💬 {}", i.en), i.ko.clone(), spec)
                    })
                    .collect();
                if let Some(l) = lessons.iter().find(|l| l.date == Some(d)) {
                    left.extend(l.sentences.iter().filter(|s| !in_deck(&s.text)).map(|s| {
                        let spec = Spec {
                            item_id: None,
                            new_item: Some(StudyItem { kind: ItemKind::Sentence, text: s.text.clone(), meaning_ko: s.ko.clone(), category: "reading".into(), ..Default::default() }),
                            target: s.text.clone(),
                            ko: s.ko.clone(),
                            tip: s.tip_ko.clone(),
                            reps,
                            recall: false,
                            is_word: false,
                            chunks: self.store.gloss_for(&s.text),
                        };
                        (s.text.clone(), s.ko.clone(), spec)
                    }));
                }
                (d, left)
            })
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// 📅 지난 날 못 끝낸 문장: like the 새 단어 one — by day, clickable, 학습완료, practise all.
    fn past_reading_ui(&mut self, ui: &mut Ui, reps: u32) {
        let days = self.past_reading_unfinished(reps);
        if days.is_empty() {
            return;
        }
        let total: usize = days.iter().map(|d| d.1.len()).sum();
        let start = self.store.profile.start_date;
        let (mut picked, mut finished, mut all) = (None, None, false);
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("📅 지난 날 못 끝낸 문장 {total}개")).size(18.0).strong());
            ui.label(RichText::new("전에 나왔지만 아직 통과하지 못한 문장이에요. 클릭하면 그 문장을 바로 연습해요.").color(muted()));
            for (di, (date, left)) in days.iter().enumerate() {
                let day_no = (*date - start).num_days() + 1;
                egui::CollapsingHeader::new(RichText::new(format!("{day_no}일차 · {} — {}개", date.format("%m월 %d일"), left.len())).strong())
                    .id_salt(("past_reading", *date))
                    .default_open(di + 1 == days.len())
                    .show(ui, |ui| {
                        for (ri, (label, ko, spec)) in left.iter().enumerate() {
                            let (color, mark) = word_mark(self.store.word_stat(&spec.target).passes);
                            ui.horizontal_wrapped(|ui| {
                                speaker_button(ui, &mut self.voice, &spec.target);
                                let r = ui.add(egui::Label::new(RichText::new(label).size(self.store.settings.list_size()).strong().color(color)).sense(egui::Sense::click())).on_hover_text("클릭하면 이것만 바로 연습해요");
                                if r.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if r.clicked() {
                                    picked = Some((di, ri));
                                }
                                ui.label(RichText::new(mark).small().color(color));
                                if finish_button(ui) {
                                    finished = Some((di, ri));
                                }
                                crate::grammar::ko_label(ui, ko, 15.0, muted());
                            });
                        }
                    });
            }
            ui.add_space(6.0);
            all = ui.add(egui::Button::new(RichText::new(format!("▶ 지난 문장 {total}개 이어서 연습")).size(16.0)).min_size(crate::widgets::ACTION_BUTTON)).clicked();
        });
        let title = |t: &str| format!("문장 읽기 · {}", t.chars().take(30).collect::<String>());
        let flat: Vec<Spec> = days.iter().flat_map(|d| d.1.iter().map(|r| r.2.clone())).collect();
        if let Some((di, ri)) = finished {
            let spec = days[di].1[ri].2.clone();
            self.mark_done(spec.new_item, &spec.target);
        } else if let Some((di, ri)) = picked {
            let skip: usize = days[..di].iter().map(|d| d.1.len()).sum::<usize>() + ri;
            let spec = flat[skip].clone();
            crate::applog!("reading: 지난 날 문장 '{}' 바로 연습", spec.target);
            let mut q = DrillQueue::new("", "reading", &title(&spec.target), vec![spec]);
            q.then = flat[skip + 1..].iter().map(|s| (title(&s.target), vec![s.clone()])).collect();
            self.queue = Some(q);
        } else if all {
            self.queue = Some(DrillQueue::new("", "reading", "지난 날 문장 읽기", flat.into_iter().map(|s| Spec { reps: 1, ..s }).collect()));
        }
    }

    /// 📚 날짜별 전체 목록: a button per day (1일차, 2일차 …); the chosen day's whole list, done
    /// ones included, coloured, clickable to practise, with 학습완료 for the rest.
    fn day_lists_ui(&mut self, ui: &mut Ui, key: &str, heading: &str, title: &str, days: &[(chrono::NaiveDate, Vec<DayRow>)]) {
        if days.is_empty() {
            return;
        }
        let start = self.store.profile.start_date;
        let sel_id = egui::Id::new(("day_list", key));
        let mut sel: Option<chrono::NaiveDate> = ui.data(|d| d.get_temp(sel_id)).flatten();
        let (mut picked, mut finished) = (None, None);
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new(heading).size(18.0).strong());
            ui.label(RichText::new("날짜를 누르면 그날 목록 전체(학습완료한 것 포함)를 보여 줘요.").color(muted()));
            ui.horizontal_wrapped(|ui| {
                for (d, rows) in days {
                    let done = rows.iter().filter(|r| word_mark(self.store.word_stat(&r.specs[0].target).passes).1.starts_with('✓')).count();
                    let label = format!("{}일차 ({done}/{})", (*d - start).num_days() + 1, rows.len());
                    if ui.selectable_label(sel == Some(*d), label).on_hover_text(d.format("%Y-%m-%d").to_string()).clicked() {
                        sel = if sel == Some(*d) { None } else { Some(*d) };
                    }
                }
            });
            let Some((d, rows)) = days.iter().find(|(d, _)| Some(*d) == sel) else { return };
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{}일차 · {} — {}개", (*d - start).num_days() + 1, d.format("%m월 %d일"), rows.len())).strong());
            for (i, r) in rows.iter().enumerate() {
                let target = &r.specs[0].target;
                let (color, mark) = word_mark(self.store.word_stat(target).passes);
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, target);
                    let resp = ui.add(egui::Label::new(RichText::new(&r.label).size(self.store.settings.list_size()).strong().color(color)).sense(egui::Sense::click())).on_hover_text("클릭하면 이것만 바로 연습해요");
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        picked = Some(i);
                    }
                    ui.label(RichText::new(&mark).small().color(color));
                    if !mark.starts_with('✓') && finish_button(ui) {
                        finished = Some(i);
                    }
                    crate::grammar::ko_label(ui, &r.ko, 15.0, muted());
                });
            }
        });
        ui.data_mut(|d| d.insert_temp(sel_id, sel));
        let Some(rows) = days.iter().find(|(d, _)| Some(*d) == sel).map(|x| &x.1) else { return };
        let category = if key == "words" { "vocab" } else { "reading" };
        let name = |r: &DayRow| format!("{title} · {}", r.specs[0].target.chars().take(30).collect::<String>());
        if let Some(i) = finished {
            let item = rows[i].specs.iter().find_map(|s| s.new_item.clone());
            self.mark_done(item, &rows[i].specs[0].target.clone());
        } else if let Some(i) = picked {
            let mut q = DrillQueue::new("", category, &name(&rows[i]), rows[i].specs.clone());
            q.then = rows[i + 1..].iter().map(|r| (name(r), r.specs.clone())).collect();
            self.queue = Some(q);
        }
    }

    /// Words and sentences not finished yet — today's first, then earlier days' — for the
    /// notifications.
    pub fn unfinished_items(&mut self) -> (Vec<String>, Vec<String>) {
        let ws = self.word_set();
        let mut words: Vec<String> = ws
            .phrases
            .iter()
            .map(|w| (w, WordKind::Chunk))
            .chain(ws.words.iter().map(|w| (w, WordKind::Word)))
            .chain(ws.expressions.iter().map(|w| (w, WordKind::Expression)))
            .filter(|(w, k)| !self.entry_passed(&w.word, *k) && !self.is_known(&w.word, *k))
            .map(|(w, _)| w.word.clone())
            .collect();
        words.extend(self.past_unfinished().into_iter().flat_map(|(_, v)| v.into_iter().map(|(w, _)| w.word)));
        let lesson = self.lesson.clone().filter(|l| l.date == Some(today())).unwrap_or_default();
        let idioms = self.idiom_set();
        let in_deck = |t: &str| {
            let id = srs::make_id(ItemKind::Sentence, t);
            self.store.items.iter().any(|i| i.id == id)
        };
        let mut sentences: Vec<String> = idioms.iter().map(|i| i.en.clone()).chain(lesson.sentences.iter().map(|s| s.text.clone())).filter(|t| !in_deck(t)).collect();
        let reps = self.store.settings.reps_new;
        sentences.extend(self.past_reading_unfinished(reps).into_iter().flat_map(|(_, v)| v.into_iter().map(|(_, _, spec)| spec.target)));
        (words, sentences)
    }

    /// Already saved in the deck (passed on an earlier day or before a restart).
    fn entry_passed(&self, text: &str, kind: WordKind) -> bool {
        let id = srs::make_id(if kind == WordKind::Chunk { ItemKind::Sentence } else { ItemKind::Word }, text);
        self.store.items.iter().any(|i| i.id == id)
    }

    /// Drills for one 새 단어 entry: the word/chunk `reps` times, then its example once.
    /// A passed entry in the day's run is just said once (no example) unless `full` (picked
    /// from the list); either way it isn't added to the review schedule a second time.
    fn entry_specs(&self, w: &LessonWord, kind: WordKind, reps: u32, full: bool) -> Vec<Spec> {
        let passed = self.entry_passed(&w.word, kind);
        let is_word = kind != WordKind::Chunk;
        if passed && !full {
            let tip = if is_word { "✓ 통과한 단어 — 한 번만 확인" } else { "✓ 통과한 표현 — 한 번만 확인" };
            return vec![Spec { reps: 1, tip: tip.into(), is_word, recall: false, ..recall_spec(&w.word, &w.meaning_ko) }];
        }
        let (item_kind, category, tip) = match kind {
            WordKind::Chunk => (ItemKind::Sentence, "phrase", "🔗 덩어리 표현 — 한 단어처럼 이어서 말하세요"),
            WordKind::Word => (ItemKind::Word, "vocab", ""),
            WordKind::Expression => (ItemKind::Word, "expression", "💬 원어민 표현·유행어"),
        };
        let mut specs = vec![
            Spec {
                item_id: None,
                new_item: (!passed).then(|| StudyItem {
                    kind: item_kind,
                    text: w.word.clone(),
                    meaning_ko: w.meaning_ko.clone(),
                    example: w.example.clone(),
                    example_ko: w.example_ko.clone(),
                    category: category.into(),
                    ..Default::default()
                }),
                target: w.word.clone(),
                ko: w.meaning_ko.clone(),
                tip: tip.into(),
                reps,
                recall: false,
                is_word,
                chunks: Vec::new(),
            },
        ];
        // Every new entry — word, chunk or expression — then said in three tenses, each sentence
        // 3 times (시제 3문장).
        let t = self.tense_for(w);
        specs.extend(t.rows().iter().map(|(en, ko, label, frame)| Spec {
            item_id: None,
            new_item: None,
            target: en.to_string(),
            ko: ko.to_string(),
            tip: format!("🕐 {label} · {frame} — '{}'을(를) 넣어 3번 말해요", w.word),
            reps: 3,
            recall: false,
            is_word: false,
            chunks: Vec::new(),
        }));
        specs
    }

    pub fn reading_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) {
            return;
        }
        // 30 idiom sentences from the built-in list come first (no waiting); the day's easy
        // longer sentences follow once the lesson is ready.
        self.ensure_lesson();
        let idioms = self.idiom_set();
        let lesson = self.lesson.clone().filter(|l| l.date == Some(today())).unwrap_or_default();
        let is_known = |t: &str| {
            let id = srs::make_id(ItemKind::Sentence, t);
            self.store.items.iter().any(|i| i.id == id)
        };
        let known_texts: Vec<String> = idioms.iter().map(|i| i.en.clone()).chain(lesson.sentences.iter().map(|s| s.text.clone())).filter(|t| is_known(t)).collect();
        let known = |t: &str| known_texts.iter().any(|k| k == t);
        let rows: Vec<(String, String)> = idioms
            .iter()
            .map(|i| (format!("💬 {}", i.en), i.ko.clone()))
            .chain(lesson.sentences.iter().map(|s| (s.text.clone(), s.ko.clone())))
            .collect();
        // Practice so far, like the 새 단어 list: red → orange → green.
        let marks: Vec<(Color32, String)> = idioms
            .iter()
            .map(|i| i.en.as_str())
            .chain(lesson.sentences.iter().map(|s| s.text.as_str()))
            .map(|t| word_mark(self.store.word_stat(t).passes))
            .collect();
        let reps = self.store.settings.reps_new;
        // Claude rewrites today's sentences in three tenses once a day (where they can change).
        let today_sentences: Vec<(String, String)> = idioms.iter().map(|i| (i.en.clone(), i.ko.clone())).chain(lesson.sentences.iter().map(|s| (s.text.clone(), s.ko.clone()))).collect();
        self.ensure_sentence_tenses(&today_sentences);
        // One drill per row (same order as `rows`). Idioms once each in the day's run; a sentence
        // passed before (e.g. before a restart) once, without counting as a new review.
        let row_specs: Vec<Spec> = idioms
            .iter()
            .map(|i| Spec {
                item_id: None,
                // Saved once; afterwards spaced repetition brings it back.
                new_item: (!known(&i.en)).then(|| StudyItem {
                    kind: ItemKind::Sentence,
                    text: i.en.clone(),
                    meaning_ko: i.ko.clone(),
                    example: i.also.first().cloned().unwrap_or_default(),
                    category: "idiom".into(),
                    ..Default::default()
                }),
                target: i.en.clone(),
                ko: i.ko.clone(),
                tip: i.also.first().map(|a| format!("💬 이렇게도 말해요: {a}")).unwrap_or_else(|| "💬 오늘의 숙어 문장".into()),
                reps: 1,
                recall: false,
                is_word: false,
                chunks: Vec::new(),
            })
            .chain(lesson.sentences.iter().map(|s| {
                let passed = known(&s.text);
                Spec {
                    item_id: None,
                    new_item: (!passed).then(|| StudyItem {
                        kind: ItemKind::Sentence,
                        text: s.text.clone(),
                        meaning_ko: s.ko.clone(),
                        category: "reading".into(),
                        ..Default::default()
                    }),
                    target: s.text.clone(),
                    ko: s.ko.clone(),
                    tip: s.tip_ko.clone(),
                    reps: if passed { 1 } else { reps },
                    recall: false,
                    is_word: false,
                    chunks: self.store.gloss_for(&s.text),
                }
            }))
            .collect();
        let (start, picked, finished) = self.intro_card(
            ui,
            "📖 문장 소리 내어 읽기 (쉐도잉)",
            &format!(
                "💬 오늘의 숙어 문장 {}개를 한 번씩 소리 내어 읽고, 이어서 읽기 쉬운 긴 문장을 원어민처럼 따라 읽어요(각 {reps}번 통과). 시제를 바꿀 수 있는 문장은 이어서 🕐 현재·과거·미래로 2번씩 말해요. 문장을 클릭하면 그 문장만 바로 연습해요.",
                idioms.len()
            ),
            &rows,
            "reading",
            &marks,
            true,
        );
        if let Some(spec) = finished.and_then(|i| row_specs.get(i).cloned()) {
            self.mark_done(spec.new_item, &spec.target);
            return;
        }
        // Each sentence, then (where it can change tense) present · past · future, twice each.
        let with_tenses = |s: Spec| -> Vec<Spec> {
            let tenses = self.sentence_tense_specs(&s.target);
            std::iter::once(s).chain(tenses).collect()
        };
        if let Some((i, spec)) = picked.and_then(|i| Some((i, row_specs.get(i).cloned()?))) {
            // One sentence on demand, repeated in full so the count moves toward 기억 완료;
            // "다음 문장" then goes on down the list.
            crate::applog!("reading: 리스트에서 '{}' 바로 연습", spec.target);
            let title = format!("문장 읽기 · {}", spec.target.chars().take(30).collect::<String>());
            let mut q = DrillQueue::new("", "reading", &title, with_tenses(Spec { reps, ..spec }));
            q.then = row_specs[i + 1..].iter().map(|s| (format!("문장 읽기 · {}", s.target.chars().take(30).collect::<String>()), with_tenses(Spec { reps, ..s.clone() }))).collect();
            self.queue = Some(q);
            return;
        }
        if start {
            let mut specs: Vec<Spec> = row_specs.into_iter().flat_map(with_tenses).collect();
            // Finally say each sentence again from the Korean only (기억해서 말하기).
            specs.extend(lesson.sentences.iter().filter(|s| !s.ko.is_empty() && !known(&s.text)).map(|s| recall_spec(&s.text, &s.ko)));
            self.queue = Some(DrillQueue::new("reading", "reading", "문장 읽기", specs));
        }
        self.past_reading_ui(ui, reps);
        let days: Vec<(chrono::NaiveDate, Vec<DayRow>)> = self
            .reading_days_list(reps, true)
            .into_iter()
            .map(|(d, v)| {
                let rows = v
                    .into_iter()
                    .map(|(label, ko, spec)| {
                        let tenses = self.sentence_tense_specs(&spec.target);
                        DayRow { label, ko, specs: std::iter::once(spec).chain(tenses).collect() }
                    })
                    .collect();
                (d, rows)
            })
            .collect();
        self.day_lists_ui(ui, "reading", "📚 날짜별 전체 문장", "문장 읽기", &days);
        if lesson.sentences.is_empty() {
            ui.add_space(8.0);
            self.lesson_ready_ui(ui);
        }
    }

    /// A 문장 읽기 sentence in the present, past and future (Claude's, kept in
    /// sentence_tenses.json), each said twice. None for a sentence that can't change tense or
    /// hasn't been rewritten yet; a tense the same as the sentence itself is left out.
    pub fn sentence_tense_specs(&self, text: &str) -> Vec<Spec> {
        let Some(t) = self.store.sentence_tenses.get(&text.trim().to_lowercase()).filter(|t| t.is_complete()) else {
            return Vec::new();
        };
        let same = |a: &str| a.trim().trim_end_matches(['.', '!', '?']).eq_ignore_ascii_case(text.trim().trim_end_matches(['.', '!', '?']));
        [(&t.present, &t.present_ko), (&t.past, &t.past_ko), (&t.future, &t.future_ko)]
            .into_iter()
            .zip(crate::tenses::SENTENCE_TENSES)
            .filter(|((en, _), _)| !same(en))
            .map(|((en, ko), (label, frame))| Spec {
                item_id: None,
                new_item: None,
                target: en.clone(),
                ko: ko.clone(),
                tip: format!("🕐 {label} · {frame} — 시제를 바꿔 2번 말해요"),
                reps: SENTENCE_TENSE_REPS,
                recall: false,
                is_word: false,
                chunks: Vec::new(),
            })
            .collect()
    }

    pub fn review_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) {
            return;
        }
        page_title(ui, "🔁 복습", "한국어 뜻을 보고 영어로 먼저 말해 본 뒤(기억해 내기), 정답을 여러 번 소리 내어 따라 합니다. 첫 시도 점수로 다음 복습일이 정해져요.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.wordle.tab, 0, RichText::new("🔁 복습 카드").size(16.0));
            ui.selectable_value(&mut self.wordle.tab, 1, RichText::new("🔠 낱말 맞추기").size(16.0));
        });
        ui.add_space(8.0);
        if self.wordle.tab == 1 {
            self.wordle_ui(ui);
            return;
        }
        let ids = srs::review_queue(&self.store.items, 40);
        let due = srs::due_count(&self.store.items);
        let weak = self.store.items.iter().filter(|i| i.weak).count();
        card(ui, |ui| {
            ui.label(RichText::new(format!("오늘 복습할 항목 {due}개 · 약점 항목 {weak}개")).size(18.0));
            if ids.is_empty() {
                ui.label(RichText::new("지금은 복습할 항목이 없어요. 새 단어와 문장을 배우면 여기에 쌓입니다.").color(muted()));
            }
        });
        if ids.is_empty() {
            return;
        }
        ui.add_space(8.0);
        if ui.add(egui::Button::new(RichText::new(format!("▶ 복습 시작 ({}개)", ids.len())).size(17.0)).fill(ACCENT).min_size(crate::widgets::BIG_BUTTON)).clicked() {
            let s = self.store.settings.clone();
            let specs = ids
                .iter()
                .filter_map(|id| self.store.items.iter().find(|i| &i.id == id))
                .map(|i| {
                    let blanked = if i.kind == ItemKind::Word && !i.example.is_empty() {
                        i.example.replace(&i.text, "____")
                    } else {
                        String::new()
                    };
                    Spec {
                        item_id: Some(i.id.clone()),
                        new_item: None,
                        target: i.text.clone(),
                        ko: i.meaning_ko.clone(),
                        tip: blanked,
                        reps: srs::required_reps(Some(i), &s, false),
                        recall: true,
                        is_word: i.kind == ItemKind::Word,
                        chunks: self.store.gloss_for(&i.text),
                    }
                })
                .collect();
            self.queue = Some(DrillQueue::new("review", "review", "복습", specs));
        }
    }

    /// Starts a queue that drills phrases and saves them as sentences (used by speaking screens).
    pub fn start_phrase_queue(&mut self, title: &str, category: &'static str, phrases: &[Phrase]) {
        let reps = self.store.settings.reps_new;
        let specs = phrases
            .iter()
            .filter(|p| !p.text.trim().is_empty())
            .map(|p| Spec {
                item_id: None,
                new_item: Some(StudyItem {
                    kind: ItemKind::Sentence,
                    text: p.text.clone(),
                    meaning_ko: p.ko.clone(),
                    category: category.into(),
                    ..Default::default()
                }),
                target: p.text.clone(),
                ko: p.ko.clone(),
                tip: String::new(),
                reps,
                recall: false,
                is_word: false,
                chunks: self.store.gloss_for(&p.text),
            })
            .collect();
        self.queue = Some(DrillQueue::new("", category, title, specs));
    }
}

/// Times each tense of a 문장 읽기 sentence is said (3 would make the day too long).
pub const SENTENCE_TENSE_REPS: u32 = 2;

/// The small "학습완료" button for a word too easy to drill.
fn finish_button(ui: &mut Ui) -> bool {
    ui.add(egui::Button::new(RichText::new("✓ 학습완료").small()).small()).on_hover_text("아주 쉬운 단어: 연습 없이 완료로 표시해요 (복습에도 나오지 않아요)").clicked()
}

/// One row of a day's full list: what is shown and the drills for it.
pub struct DayRow {
    pub label: String,
    pub ko: String,
    pub specs: Vec<Spec>,
}
