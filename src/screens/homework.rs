//! Homework: items the learner struggled with come back as tasks — read aloud N times
//! (checked by speech recognition) or use a word in an original sentence (checked by Claude).

use crate::app::App;
use crate::claude::WordUseCheck;
use crate::models::*;
use crate::widgets::*;
use egui::{Color32, RichText, Ui, Vec2};

#[derive(Default)]
pub struct HomeworkState {
    /// (homework index, task index) being worked on.
    pub active: Option<(usize, usize)>,
    pub drill: Option<Drill>,
    pub capture: Capture,
    pub check: Pending<WordUseCheck>,
    pub check_error: Option<String>,
    /// 나만의 문장: what was said last, and whether it used the word.
    pub said: Option<(String, bool)>,
    pub show_hint: bool,
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
                    ui.horizontal(|ui| {
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
        self.homework.capture = Capture::default();
        self.homework.check_error = None;
        self.homework.said = None;
        self.homework.show_hint = false;
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
                finished = self.make_sentence_ui(ui, &task, c);
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

    /// ✏ 나만의 문장 만들기: a mission screen that looks nothing like read-aloud — the word sits
    /// in a sentence template, and what was said is checked for the word before Claude judges it.
    /// Returns true when the learner moves on.
    fn make_sentence_ui(&mut self, ui: &mut Ui, task: &HomeworkTask, c: crate::claude::Claude) -> bool {
        let word = task.text.clone();
        let mut next = false;
        // Mission banner.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(760.0), 78.0), egui::Sense::hover());
        crate::art::gradient_rect(ui.painter(), rect, 16.0, Color32::from_rgb(236, 120, 40), crate::art::PINK);
        ui.painter().text(rect.left_top() + Vec2::new(20.0, 14.0), egui::Align2::LEFT_TOP, "✏ 나만의 문장 만들기 미션", egui::FontId::proportional(22.0), Color32::WHITE);
        ui.painter().text(
            rect.left_top() + Vec2::new(20.0, 46.0),
            egui::Align2::LEFT_TOP,
            "따라 읽기가 아니에요! 아래 단어를 꼭 넣어 내 문장을 직접 만들어 말하세요",
            egui::FontId::proportional(15.0),
            Color32::from_rgb(255, 240, 230),
        );
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                // The sentence template: ____ ( word ) ____ .
                ui.horizontal(|ui| {
                    let blank = RichText::new("＿＿＿＿").size(28.0).color(muted());
                    let w = ui.fonts(|f| f.layout_no_wrap(word.clone(), egui::FontId::proportional(32.0), Color32::WHITE).size().x) + 230.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    ui.label(blank.clone());
                    egui::Frame::new().fill(warn()).corner_radius(10.0).inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| {
                        ui.label(RichText::new(format!("( {word} )")).size(32.0).strong().color(Color32::WHITE));
                    });
                    ui.label(blank);
                    ui.label(RichText::new(".").size(28.0).color(muted()));
                });
                crate::grammar::ko_label(ui, &format!("뜻: {}", task.meaning_ko), 17.0, muted());
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(((ui.available_width() - 520.0) / 2.0).max(0.0));
                    for step in ["① 뜻 확인", "② 내 이야기로 문장 떠올리기", "③ 🎤 눌러서 말하기"] {
                        chip(ui, step, crate::art::VIOLET);
                    }
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.homework.show_hint, "💡 막히면 힌트 보기");
                if self.homework.show_hint {
                    egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                        ui.label(RichText::new("이렇게 시작해 보세요 (… 자리에 내 이야기를)").color(ACCENT));
                        for starter in [format!("I … {word} …"), format!("Yesterday, my … {word} …"), format!("I think {word} is …"), format!("At work, I … {word} …")] {
                            ui.label(RichText::new(starter).size(16.0));
                        }
                        if let Some((b, _)) = crate::dictionary::lookup(&word).filter(|(b, _)| !b.example.is_empty()) {
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("참고 예문: {}", b.example)).color(muted()));
                            ui.label(RichText::new("그대로 따라 하지 말고, 바꿔서 내 문장으로 만들어요!").color(warn()).small());
                        }
                    });
                }
                ui.add_space(8.0);
                if self.homework.check.running() {
                    spinner_label(ui, "Claude가 내 문장을 확인 중…");
                } else if let Some(said) = self.homework.capture.ui(ui, &mut self.voice, 30.0) {
                    self.store.log_attempt("homework", &word, &said, 0.0);
                    let used = crate::dictionary::sentence_uses(&said, &word);
                    let long_enough = said.split_whitespace().count() >= 4;
                    self.homework.said = Some((said.clone(), used && long_enough));
                    self.homework.check_error = None;
                    // Only a real sentence with the word goes to Claude (saves credit too).
                    if used && long_enough {
                        let (w, m) = (word.clone(), task.meaning_ko.clone());
                        self.homework.check.spawn(move || c.check_word_use(&w, &m, &said));
                    }
                }
                ui.label(RichText::new(format!("🎤 '{word}'을(를) 넣어서 말해요")).color(muted()).small());
                // What was said, with the word highlighted (or what's missing).
                if let Some((said, ok)) = self.homework.said.clone() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("내 문장:").color(muted()));
                        for t in said.split_whitespace() {
                            if crate::dictionary::sentence_uses(t, &word) || word.split_whitespace().any(|w| crate::dictionary::same_word(t, w)) {
                                egui::Frame::new().fill(GOOD.gamma_multiply(0.3)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(4, 0)).show(ui, |ui| {
                                    ui.label(RichText::new(t).size(18.0).strong().color(GOOD));
                                });
                            } else {
                                ui.label(RichText::new(t).size(18.0));
                            }
                        }
                    });
                    if !crate::dictionary::sentence_uses(&said, &word) {
                        error_label(ui, &format!("✖ '{word}'이(가) 빠졌어요 — 녹음만 하지 말고, '{word}'을(를) 넣은 내 문장을 만들어 다시 말해 보세요."));
                    } else if !ok {
                        error_label(ui, "✖ 단어만 말했어요 — 4단어 이상의 문장으로 말해 보세요. (예: 주어 + 동사 + 단어 + 이야기)");
                    } else {
                        ui.label(RichText::new(format!("✓ '{word}'을(를) 넣어 문장을 만들었어요")).color(GOOD).strong());
                    }
                }
                if let Some(e) = &self.homework.check_error {
                    error_label(ui, e);
                }
                if !task.feedback.is_empty() {
                    ui.label(RichText::new(&task.feedback).color(if task.passed > 0 { GOOD } else { warn() }));
                }
                if task.passed > 0 && ui.add(egui::Button::new("다음 ▶").fill(GOOD).min_size(Vec2::new(140.0, 36.0))).clicked() {
                    next = true;
                }
            });
        });
        next
    }
}
