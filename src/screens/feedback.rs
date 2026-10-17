//! Feedback center (modelled on Learna): words the learner keeps mispronouncing, and every
//! correction Claude has made, each with a way to practice it out loud.

use crate::app::App;
use crate::models::Phrase;
use crate::widgets::*;
use egui::{RichText, Ui, Vec2};

#[derive(Default)]
pub struct FeedbackState {
    pub tab: usize,
    pub queue: Vec<String>,
    pub drill: Option<Drill>,
}

impl App {
    pub fn feedback_ui(&mut self, ui: &mut Ui) {
        if self.run_queue_ui(ui) {
            return;
        }
        page_title(ui, "🔬 피드백 센터", "");
        ui.horizontal(|ui| {
            let open = self.store.mistakes.iter().filter(|m| m.fixed_streak < 3).count();
            let corr = self.store.corrections.iter().filter(|c| !c.practiced).count();
            ui.selectable_value(&mut self.feedback.tab, 0, format!("💬 내 발음 실수 ({open})"));
            ui.selectable_value(&mut self.feedback.tab, 1, format!("✏ 교정된 문장 ({corr})"));
        });
        ui.add_space(6.0);
        if self.feedback.tab == 0 { self.mistakes_ui(ui) } else { self.corrections_ui(ui) }
    }

    fn mistakes_ui(&mut self, ui: &mut Ui) {
        let pass = self.pass_score();
        let claude = self.claude();
        if let Some(drill) = self.feedback.drill.as_mut() {
            let word = drill.target.clone();
            ui.label(RichText::new(format!("남은 단어 {}개 · 3번 연속 정확히 말하면 해결!", self.feedback.queue.len() + 1)).color(MUTED));
            let ev = card(ui, |ui| drill.ui(ui, &mut self.voice, pass, claude.as_ref()));
            match ev {
                DrillEvent::Attempt(a) => {
                    self.record_score("pronunciation", &word, &a.transcript, a.score);
                    if let Some(m) = self.store.mistakes.iter_mut().find(|m| m.word == word) {
                        if a.score >= pass {
                            m.fixed_streak += 1;
                        } else {
                            m.fixed_streak = 0;
                        }
                    }
                    self.store.save_mistakes();
                }
                DrillEvent::Completed => {
                    self.feedback.drill = None;
                    if !self.feedback.queue.is_empty() {
                        let w = self.feedback.queue.remove(0);
                        self.feedback.drill = Some(Drill::new(&w, "", "", 3, false, true));
                    } else {
                        self.toast("🎉 발음 연습 완료!");
                    }
                }
                DrillEvent::None => {}
            }
            if ui.small_button("그만하기").clicked() {
                self.feedback.drill = None;
                self.feedback.queue.clear();
            }
            return;
        }
        ui.label(RichText::new("연습 중 음성 인식이 알아듣지 못한 단어들이에요. 발음이 부정확했을 가능성이 높습니다.").color(MUTED));
        let mut list: Vec<_> = self.store.mistakes.iter().filter(|m| m.fixed_streak < 3).cloned().collect();
        list.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
        if list.is_empty() {
            card(ui, |ui| {
                ui.label("아직 발음 실수가 없어요 👍");
            });
            return;
        }
        if ui.add(egui::Button::new(RichText::new(format!("▶ 상위 {}개 발음 연습", list.len().min(10))).size(17.0)).fill(ACCENT).min_size(Vec2::new(0.0, 40.0))).clicked() {
            let mut words: Vec<String> = list.iter().take(10).map(|m| m.word.clone()).collect();
            let first = words.remove(0);
            self.feedback.queue = words;
            self.feedback.drill = Some(Drill::new(&first, "", "", 3, false, true));
        }
        ui.add_space(6.0);
        card(ui, |ui| {
            egui::Grid::new("mistakes").striped(true).show(ui, |ui| {
                for m in list.iter().take(60) {
                    speaker_button(ui, &mut self.voice, &m.word);
                    ui.label(RichText::new(&m.word).size(17.0).strong());
                    ui.label(RichText::new(format!("{}회", m.count)).color(WARN));
                    ui.label(RichText::new(format!("최근 {}", m.last)).color(MUTED));
                    ui.end_row();
                }
            });
        });
    }

    fn corrections_ui(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("자기 표현·회화에서 Claude가 고쳐 준 문장들이에요. 소리 내어 따라 하면 단어장에 들어가 반복 학습됩니다.").color(MUTED));
        let pending: Vec<Phrase> = self
            .store
            .corrections
            .iter()
            .filter(|c| !c.practiced)
            .map(|c| Phrase { text: c.corrected.clone(), ko: c.explanation_ko.clone() })
            .collect();
        if !pending.is_empty() && ui.add(egui::Button::new(RichText::new(format!("▶ 교정 문장 {}개 따라 말하기", pending.len().min(10))).size(17.0)).fill(ACCENT).min_size(Vec2::new(0.0, 40.0))).clicked() {
            let batch: Vec<Phrase> = pending.into_iter().take(10).collect();
            for c in self.store.corrections.iter_mut().filter(|c| batch.iter().any(|p| p.text == c.corrected)) {
                c.practiced = true;
            }
            self.store.save_corrections();
            self.start_phrase_queue("교정 문장 연습", "correction", &batch);
            return;
        }
        if self.store.corrections.is_empty() {
            card(ui, |ui| {
                ui.label("아직 교정 기록이 없어요. 자기 표현이나 회화를 해 보세요.");
            });
            return;
        }
        ui.add_space(6.0);
        let list: Vec<_> = self.store.corrections.iter().rev().take(80).cloned().collect();
        for c in list {
            card(ui, |ui| {
                ui.label(RichText::new(format!("{} · {}{}", c.date, c.source, if c.practiced { " · ✅ 연습함" } else { "" })).color(MUTED).small());
                ui.label(RichText::new(format!("✖ {}", c.original)).color(BAD));
                ui.horizontal_wrapped(|ui| {
                    speaker_button(ui, &mut self.voice, &c.corrected);
                    ui.label(RichText::new(format!("✓ {}", c.corrected)).color(GOOD).strong());
                });
                if !c.explanation_ko.is_empty() {
                    ui.label(RichText::new(&c.explanation_ko).color(MUTED));
                }
            });
            ui.add_space(4.0);
        }
    }
}
