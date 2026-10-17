//! Homework: items the learner struggled with come back as tasks — read aloud N times
//! (checked by speech recognition) or use a word in an original sentence (checked by Claude).

use crate::app::App;
use crate::claude::WordUseCheck;
use crate::models::*;
use crate::widgets::*;
use egui::{RichText, Ui, Vec2};

#[derive(Default)]
pub struct HomeworkState {
    /// (homework index, task index) being worked on.
    pub active: Option<(usize, usize)>,
    pub drill: Option<Drill>,
    pub capture: Capture,
    pub check: Pending<WordUseCheck>,
    pub check_error: Option<String>,
}

impl App {
    pub fn homework_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "📝 숙제", "자주 틀린 단어·문장이 숙제로 나옵니다. 모두 통과하면 숙제가 확인 완료됩니다.");
        ui.add_space(8.0);

        if self.homework.active.is_some() {
            self.homework_task_ui(ui);
            return;
        }

        let open: Vec<usize> = (0..self.store.homework.len()).filter(|i| self.store.homework[*i].completed.is_none()).collect();
        if open.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("남은 숙제가 없어요 👍").size(18.0));
                ui.label(RichText::new("학습 중 통과 점수에 못 미친 항목이 생기면 '오늘 학습 마무리' 때 숙제로 나옵니다.").color(MUTED));
            });
            if !self.step_done("homework") && self.store.homework.iter().any(|h| h.completed == Some(today())) {
                self.complete_step("homework");
            }
        }
        let mut start: Option<(usize, usize)> = None;
        for hi in open {
            let hw = self.store.homework[hi].clone();
            let (done, total) = hw.progress();
            card(ui, |ui| {
                ui.label(RichText::new(format!("{} 숙제 · {done}/{total}", hw.assigned)).size(18.0).strong());
                for (ti, t) in hw.tasks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(if t.done { "✅" } else { "⬜" });
                        ui.label(RichText::new(&t.instruction_ko).color(if t.done { MUTED } else { egui::Color32::WHITE }));
                        if !t.done && ui.button("하기").clicked() {
                            start = Some((hi, ti));
                        }
                    });
                }
            });
            ui.add_space(6.0);
        }
        if let Some((hi, ti)) = start {
            self.start_task(hi, ti);
        }
    }

    fn start_task(&mut self, hi: usize, ti: usize) {
        let t = &self.store.homework[hi].tasks[ti];
        self.homework.drill = (t.kind == TaskKind::ReadAloud).then(|| Drill::new(&t.text, &t.meaning_ko, "", t.reps.saturating_sub(t.passed).max(1), false, false).with_chunks(self.store.gloss_for(&t.text)));
        self.homework.active = Some((hi, ti));
        self.homework.capture = Capture::default();
        self.homework.check_error = None;
    }

    fn homework_task_ui(&mut self, ui: &mut Ui) {
        let Some((hi, ti)) = self.homework.active else { return };
        let task = self.store.homework[hi].tasks[ti].clone();
        let pass = self.pass_score();
        let claude = self.claude();
        let mut finished = false;
        ui.label(RichText::new(&task.instruction_ko).size(18.0).strong());
        ui.add_space(6.0);
        match task.kind {
            TaskKind::ReadAloud => {
                let Some(drill) = self.homework.drill.as_mut() else { return };
                let ev = card(ui, |ui| drill.ui(ui, &mut self.voice, pass, claude.as_ref()));
                if let Some((text, chunks)) = drill.fresh_gloss.take() {
                    self.store.put_gloss(&text, &chunks);
                }
                match ev {
                    DrillEvent::Attempt(a) => {
                        self.record_attempt("homework", &task.text, &a);
                        if a.score >= pass {
                            self.store.homework[hi].tasks[ti].passed += 1;
                            self.store.save_homework();
                        }
                    }
                    DrillEvent::Completed => {
                        let first = self.homework.drill.as_ref().and_then(|d| d.first_score).unwrap_or(0.0);
                        self.apply_result(Some(task.item_id.clone()), None, first);
                        finished = true;
                    }
                    DrillEvent::None => {}
                }
            }
            TaskKind::UseWord => {
                let Some(c) = claude else {
                    error_label(ui, "이 숙제는 Claude API 키가 필요합니다.");
                    return;
                };
                if let Some(r) = self.homework.check.poll() {
                    match r {
                        Ok(res) => {
                            let t = &mut self.store.homework[hi].tasks[ti];
                            t.feedback = format!("{}\n👉 {}", res.feedback_ko, res.better);
                            if res.ok {
                                t.passed = 1;
                            }
                            self.store.save_homework();
                        }
                        Err(e) => self.homework.check_error = Some(format!("{e:#}")),
                    }
                }
                let task = self.store.homework[hi].tasks[ti].clone();
                card(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&task.text).size(30.0).strong());
                        crate::grammar::ko_label(ui, &task.meaning_ko, 16.0, MUTED);
                        ui.add_space(8.0);
                        if self.homework.check.running() {
                            spinner_label(ui, "Claude가 확인 중…");
                        } else if let Some(transcript) = self.homework.capture.ui(ui, &mut self.voice, 30.0) {
                            let (w, m) = (task.text.clone(), task.meaning_ko.clone());
                            self.store.log_attempt("homework", &w, &transcript, 0.0);
                            self.homework.check.spawn(move || c.check_word_use(&w, &m, &transcript));
                        }
                        if let Some(e) = &self.homework.check_error {
                            error_label(ui, e);
                        }
                        if !task.feedback.is_empty() {
                            ui.label(RichText::new(&task.feedback).color(if task.passed > 0 { GOOD } else { WARN }));
                        }
                        if task.passed > 0 && ui.add(egui::Button::new("다음 ▶").fill(GOOD).min_size(Vec2::new(140.0, 36.0))).clicked() {
                            finished = true;
                        }
                    });
                });
            }
        }
        if ui.small_button("◀ 숙제 목록").clicked() {
            self.homework.active = None;
        }
        if finished {
            let t = &mut self.store.homework[hi].tasks[ti];
            t.done = true;
            t.passed = t.passed.max(t.reps.min(1));
            let hw = &mut self.store.homework[hi];
            if hw.tasks.iter().all(|t| t.done) {
                hw.completed = Some(today());
                self.toast("🎉 숙제 확인 완료!");
                if !self.step_done("homework") {
                    self.complete_step("homework");
                }
            }
            self.store.save_homework();
            // Move straight on to the next unfinished task in the same homework.
            let next = self.store.homework[hi].tasks.iter().position(|t| !t.done);
            match next {
                Some(n) => self.start_task(hi, n),
                None => self.homework.active = None,
            }
        }
    }
}
