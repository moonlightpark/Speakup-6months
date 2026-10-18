//! Drill queues: new words, reading aloud, spaced-repetition review and phrase practice.
//! Each item is a `Drill` the learner must say out loud N times; results feed the SRS.

use crate::app::App;
use crate::models::*;
use crate::srs;
use crate::widgets::*;
use egui::{Color32, RichText, Ui, Vec2};

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
}

impl DrillQueue {
    pub fn new(step: &'static str, category: &'static str, title: &str, specs: Vec<Spec>) -> Self {
        let drill = specs.first().map(make_drill).unwrap_or_default();
        Self { step, category, title: title.to_string(), specs, idx: 0, drill, results: Vec::new() }
    }

    pub fn finished(&self) -> bool {
        self.idx >= self.specs.len()
    }
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
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("🎉 잘했어요!").size(30.0).strong());
                    ui.label(RichText::new(format!("{title} 완료 · 첫 시도 평균 {avg:.0}점")).size(18.0));
                });
                if !weak.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("어려웠던 항목 (복습·숙제로 다시 나옵니다)").color(WARN));
                    for w in &weak {
                        ui.label(format!("• {w}"));
                    }
                }
            });
            ui.add_space(8.0);
            let mut close = false;
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("🏠 홈으로").size(17.0)).fill(ACCENT).min_size(Vec2::new(160.0, 40.0))).clicked() {
                    close = true;
                    self.screen = crate::app::Screen::Home;
                }
                if ui.button("닫기").clicked() {
                    close = true;
                }
            });
            if close {
                self.queue = None;
            }
            if !step.is_empty() && !self.step_done(step) {
                self.complete_step(step);
            }
            return true;
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(&q.title).size(20.0).strong());
            ui.label(RichText::new(format!("{} / {}", q.idx + 1, q.specs.len())).color(MUTED));
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
            if ui.small_button("건너뛰기").clicked() {
                q.idx += 1;
                if let Some(s) = q.specs.get(q.idx) {
                    q.drill = make_drill(s);
                }
            }
            stop = ui.small_button("그만하기").clicked();
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
    fn intro_card(&mut self, ui: &mut Ui, title: &str, desc: &str, rows: &[(String, String)], step: &str, marks: &[(Color32, String)]) -> bool {
        let mut start = false;
        page_title(ui, title, desc);
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
                    let color = mark.map(|m| m.0).unwrap_or(Color32::WHITE);
                    ui.label(RichText::new(en).size(17.0).strong().color(color));
                    if let Some((c, text)) = mark {
                        ui.label(RichText::new(text).small().color(*c));
                    }
                    // Months 5-6: English only; the Korean is a hover away.
                    if support_stage(ui.ctx()) < 2 {
                        crate::grammar::ko_label(ui, ko, 16.0, MUTED);
                    } else {
                        ui.label(RichText::new("뜻").small().color(MUTED)).on_hover_text(ko);
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
        let label = if self.step_done(step) { "↻ 다시 연습하기" } else { "▶ 시작하기" };
        if self.step_done(step) {
            ui.label(RichText::new("✅ 오늘 완료했어요. 더 연습해도 좋아요!").color(GOOD));
        }
        if ui.add(egui::Button::new(RichText::new(label).size(18.0)).fill(ACCENT).min_size(Vec2::new(200.0, 44.0))).clicked() {
            start = true;
        }
        start
    }

    pub fn words_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) {
            return;
        }
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
        if self.intro_card(
            ui,
            "📚 오늘의 덩어리 표현 · 새 단어",
            &format!("🔗 덩어리 표현을 먼저, 통째로 익혀요. 듣고 → 소리 내어 {reps}번 → 예문까지 말하면 저장되고 복습 일정이 잡힙니다."),
            &rows,
            "words",
            &marks,
        ) {
            // Words and chunks already passed (saved in the deck) are said once, without the
            // example, so restarting the app doesn't make the learner redo them 3 times.
            let passed = |kind: ItemKind, text: &str| {
                let id = srs::make_id(kind, text);
                self.store.items.iter().any(|i| i.id == id)
            };
            let mut specs = Vec::new();
            for w in &lesson.phrases {
                if passed(ItemKind::Sentence, &w.word) {
                    specs.push(Spec { reps: 1, tip: "✓ 통과한 표현 — 한 번만 확인".into(), recall: false, ..recall_spec(&w.word, &w.meaning_ko) });
                    continue;
                }
                specs.push(Spec {
                    item_id: None,
                    new_item: Some(StudyItem {
                        kind: ItemKind::Sentence,
                        text: w.word.clone(),
                        meaning_ko: w.meaning_ko.clone(),
                        example: w.example.clone(),
                        example_ko: w.example_ko.clone(),
                        category: "phrase".into(),
                        ..Default::default()
                    }),
                    target: w.word.clone(),
                    ko: w.meaning_ko.clone(),
                    tip: "🔗 덩어리 표현 — 한 단어처럼 이어서 말하세요".into(),
                    reps,
                    recall: false,
                    is_word: false,
                    chunks: Vec::new(),
                });
                specs.push(Spec {
                    item_id: None,
                    new_item: None,
                    target: w.example.clone(),
                    ko: w.example_ko.clone(),
                    tip: format!("예문으로 '{}' 익히기", w.word),
                    reps: 1,
                    recall: false,
                    is_word: false,
                    chunks: self.store.gloss_for(&w.example),
                });
            }
            let slang: Vec<&LessonWord> = lesson.expressions.iter().collect();
            // Names that slip into a lesson ("Seoul") are not drilled as vocabulary.
            for (w, is_expr) in lesson.words.iter().filter(|w| !is_proper_noun(&w.word, &w.meaning_ko)).map(|w| (w, false)).chain(slang.into_iter().map(|w| (w, true))) {
                if passed(ItemKind::Word, &w.word) {
                    specs.push(Spec { reps: 1, tip: "✓ 통과한 단어 — 한 번만 확인".into(), is_word: true, recall: false, ..recall_spec(&w.word, &w.meaning_ko) });
                    continue;
                }
                specs.push(Spec {
                    item_id: None,
                    new_item: Some(StudyItem {
                        kind: ItemKind::Word,
                        text: w.word.clone(),
                        meaning_ko: w.meaning_ko.clone(),
                        example: w.example.clone(),
                        example_ko: w.example_ko.clone(),
                        category: if is_expr { "expression".into() } else { "vocab".into() },
                        ..Default::default()
                    }),
                    target: w.word.clone(),
                    ko: w.meaning_ko.clone(),
                    tip: if is_expr { "💬 원어민 표현·유행어".into() } else { String::new() },
                    reps,
                    recall: false,
                    is_word: true,
                    chunks: Vec::new(),
                });
                specs.push(Spec {
                    item_id: None,
                    new_item: None,
                    target: w.example.clone(),
                    ko: w.example_ko.clone(),
                    tip: format!("예문으로 '{}' 익히기", w.word),
                    reps: 1,
                    recall: false,
                    is_word: false,
                    chunks: self.store.gloss_for(&w.example),
                });
            }
            // Then each chunk once more from the Korean alone.
            specs.extend(lesson.phrases.iter().filter(|w| !passed(ItemKind::Sentence, &w.word)).map(|w| recall_spec(&w.word, &w.meaning_ko)));
            self.queue = Some(DrillQueue::new("words", "vocab", "새 단어", specs));
            return;
        }
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
                ui.label(RichText::new(msg).color(MUTED));
            } else {
                ui.label(RichText::new("이미 배운 단어를 한 번씩 소리 내어 말해요. 첫 시도 점수로 다음 복습일이 정해집니다.").color(MUTED));
                ui.add_space(6.0);
                start_review = ui.add(egui::Button::new(RichText::new(format!("▶ 배운 단어 {}개 복습", ids.len())).size(17.0)).min_size(Vec2::new(200.0, 40.0))).clicked();
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
        let reps = self.store.settings.reps_new;
        if self.intro_card(
            ui,
            "📖 문장 소리 내어 읽기 (쉐도잉)",
            &format!(
                "💬 오늘의 숙어 문장 {}개를 한 번씩 소리 내어 읽고, 이어서 읽기 쉬운 긴 문장을 원어민처럼 따라 읽어요(각 {reps}번 통과).",
                idioms.len()
            ),
            &rows,
            "reading",
            &[],
        ) {
            let mut specs: Vec<Spec> = idioms
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
                .collect();
            for s in &lesson.sentences {
                // Passed before (e.g. before a restart): once, without counting as a new review.
                let passed = known(&s.text);
                specs.push(Spec {
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
                });
            }
            // Finally say each sentence again from the Korean only (기억해서 말하기).
            specs.extend(lesson.sentences.iter().filter(|s| !s.ko.is_empty() && !known(&s.text)).map(|s| recall_spec(&s.text, &s.ko)));
            self.queue = Some(DrillQueue::new("reading", "reading", "문장 읽기", specs));
        }
        if lesson.sentences.is_empty() {
            ui.add_space(8.0);
            self.lesson_ready_ui(ui);
        }
    }

    pub fn review_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) {
            return;
        }
        page_title(ui, "🔁 복습", "한국어 뜻을 보고 영어로 먼저 말해 본 뒤(기억해 내기), 정답을 여러 번 소리 내어 따라 합니다. 첫 시도 점수로 다음 복습일이 정해져요.");
        ui.add_space(8.0);
        let ids = srs::review_queue(&self.store.items, 40);
        let due = srs::due_count(&self.store.items);
        let weak = self.store.items.iter().filter(|i| i.weak).count();
        card(ui, |ui| {
            ui.label(RichText::new(format!("오늘 복습할 항목 {due}개 · 약점 항목 {weak}개")).size(18.0));
            if ids.is_empty() {
                ui.label(RichText::new("지금은 복습할 항목이 없어요. 새 단어와 문장을 배우면 여기에 쌓입니다.").color(MUTED));
            }
        });
        if ids.is_empty() {
            return;
        }
        ui.add_space(8.0);
        if ui.add(egui::Button::new(RichText::new(format!("▶ 복습 시작 ({}개)", ids.len())).size(18.0)).fill(ACCENT).min_size(Vec2::new(220.0, 44.0))).clicked() {
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
