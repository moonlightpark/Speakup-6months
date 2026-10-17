//! Drill queues: new words, reading aloud, spaced-repetition review and phrase practice.
//! Each item is a `Drill` the learner must say out loud N times; results feed the SRS.

use crate::app::App;
use crate::models::*;
use crate::srs;
use crate::widgets::*;
use egui::{RichText, Ui, Vec2};

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

    fn intro_card(&mut self, ui: &mut Ui, title: &str, desc: &str, rows: &[(String, String)], step: &str) -> bool {
        let mut start = false;
        page_title(ui, title, desc);
        ui.add_space(8.0);
        card(ui, |ui| {
            for (en, ko) in rows {
                ui.horizontal(|ui| {
                    speaker_button(ui, &mut self.voice, en);
                    ui.label(RichText::new(en).size(17.0).strong());
                    ui.label(RichText::new(ko).color(MUTED));
                });
            }
        });
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
        if self.run_queue_ui(ui) || !self.lesson_ready_ui(ui) {
            return;
        }
        let lesson = self.lesson.clone().unwrap_or_default();
        let rows: Vec<(String, String)> = lesson.words.iter().map(|w| (w.word.clone(), format!("{} — {}", w.meaning_ko, w.example))).collect();
        let reps = self.store.settings.reps_new;
        if self.intro_card(ui, "📚 오늘의 새 단어", &format!("듣고 → 소리 내어 {reps}번 → 예문까지 말하면 단어장에 저장되고 복습 일정이 잡힙니다."), &rows, "words") {
            let mut specs = Vec::new();
            for w in &lesson.words {
                specs.push(Spec {
                    item_id: None,
                    new_item: Some(StudyItem {
                        kind: ItemKind::Word,
                        text: w.word.clone(),
                        meaning_ko: w.meaning_ko.clone(),
                        example: w.example.clone(),
                        example_ko: w.example_ko.clone(),
                        category: "vocab".into(),
                        ..Default::default()
                    }),
                    target: w.word.clone(),
                    ko: w.meaning_ko.clone(),
                    tip: String::new(),
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
            self.queue = Some(DrillQueue::new("words", "vocab", "새 단어", specs));
        }
    }

    pub fn reading_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) || !self.lesson_ready_ui(ui) {
            return;
        }
        let lesson = self.lesson.clone().unwrap_or_default();
        let rows: Vec<(String, String)> = lesson.sentences.iter().map(|s| (s.text.clone(), s.ko.clone())).collect();
        let reps = self.store.settings.reps_new;
        if self.intro_card(
            ui,
            "📖 문장 소리 내어 읽기 (쉐도잉)",
            &format!("원어민 음성을 듣고 리듬·연음까지 따라 하세요. 각 문장을 {reps}번 통과해야 다음으로 넘어갑니다."),
            &rows,
            "reading",
        ) {
            let specs = lesson
                .sentences
                .iter()
                .map(|s| Spec {
                    item_id: None,
                    new_item: Some(StudyItem {
                        kind: ItemKind::Sentence,
                        text: s.text.clone(),
                        meaning_ko: s.ko.clone(),
                        category: "reading".into(),
                        ..Default::default()
                    }),
                    target: s.text.clone(),
                    ko: s.ko.clone(),
                    tip: s.tip_ko.clone(),
                    reps,
                    recall: false,
                    is_word: false,
                    chunks: self.store.gloss_for(&s.text),
                })
                .collect();
            self.queue = Some(DrillQueue::new("reading", "reading", "문장 읽기", specs));
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
