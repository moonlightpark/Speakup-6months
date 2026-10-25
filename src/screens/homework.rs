//! Homework: items the learner struggled with come back as tasks — read aloud N times
//! (checked by speech recognition) or use a word in an original sentence (checked by Claude).

use crate::app::App;
use crate::screens::mission::{Mission, MissionEvent, MissionSpec};
use crate::models::*;
use crate::widgets::*;
use egui::{RichText, Ui};

#[derive(Default)]
pub struct HomeworkState {
    /// (homework index, task index) being worked on.
    pub active: Option<(usize, usize)>,
    pub drill: Option<Drill>,
    /// 나만의 문장 (use the word in a sentence).
    pub mission: Mission,
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
                ui.label(RichText::new("학습 중 통과 점수에 못 미친 항목이 생기면 '오늘 학습 마무리' 때 숙제로 나옵니다.").color(muted()));
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
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if t.done { "✅" } else { "⬜" });
                        let icon = if t.kind == TaskKind::UseWord { "✏ 문장 만들기" } else { "🔊 읽기" };
                        ui.label(RichText::new(icon).small().color(if t.kind == TaskKind::UseWord { warn() } else { ACCENT }));
                        ui.label(RichText::new(&t.instruction_ko).color(if t.done { muted() } else { text_strong() }));
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
        self.homework.mission = Mission::default();
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
                let spec = MissionSpec {
                    word: task.text.clone(),
                    meaning_ko: task.meaning_ko.clone(),
                    week: None,
                    starters: vec![format!("I … {} …", task.text), format!("Yesterday, my … {} …", task.text), format!("I think {} is …", task.text), format!("At work, I … {} …", task.text)],
                    already_passed: task.passed > 0,
                };
                let mut m = std::mem::take(&mut self.homework.mission);
                let ev = self.mission_ui(ui, &mut m, &spec);
                self.homework.mission = m;
                match ev {
                    Some(MissionEvent::Checked(res)) => {
                        let t = &mut self.store.homework[hi].tasks[ti];
                        t.feedback = format!("{}\n👉 {}", res.feedback_ko, res.better);
                        if res.ok {
                            t.passed = 1;
                        }
                        self.store.save_homework();
                    }
                    Some(MissionEvent::Next) => finished = true,
                    None => {}
                }
            }
        }
        if ui.button("◀ 숙제 목록").clicked() {
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
