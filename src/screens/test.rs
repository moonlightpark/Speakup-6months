//! Level test: the placement test on first run and the monthly checkpoints. Six graded
//! sentences read aloud (A1→C1) plus one free-speech answer, assessed by Claude.

use crate::app::{App, Screen};
use crate::claude::LevelAssessment;
use crate::content;
use crate::models::*;
use crate::widgets::*;
use egui::{RichText, Ui, Vec2};

pub struct LevelTest {
    pub placement: bool,
    /// Taken from the home screen to move to the level that fits now (replaces the level).
    pub retest: bool,
    pub week: u32,
    pub idx: usize,
    pub drill: Drill,
    pub readings: Vec<(String, String, f32)>,
    pub capture: Capture,
    pub answer: Option<String>,
    pub job: Pending<LevelAssessment>,
    pub result: Option<LevelAssessment>,
    pub error: Option<String>,
}

impl LevelTest {
    pub fn new(placement: bool, week: u32) -> Self {
        let (t, ko) = content::PLACEMENT[0];
        Self {
            placement,
            retest: false,
            week,
            idx: 0,
            drill: Drill::new(t, ko, "", 1, false, false).without_autoplay(),
            readings: Vec::new(),
            capture: Capture::default(),
            answer: None,
            job: Pending::default(),
            result: None,
            error: None,
        }
    }
}

impl LevelTest {
    pub fn retest() -> Self {
        Self { retest: true, ..Self::new(false, 0) }
    }
}

/// Offline estimate: how far up the difficulty ladder the learner read accurately.
pub fn local_estimate(readings: &[(String, String, f32)]) -> f32 {
    let mut level = 1.0;
    for (i, (_, _, s)) in readings.iter().enumerate() {
        let weight = 1.0 + i as f32 * 0.35;
        level += weight * (s / 100.0).powf(1.5);
    }
    (level * 0.9).clamp(1.0, 9.0)
}

impl App {
    pub fn level_test_ui(&mut self, ui: &mut Ui) {
        let Some(mut t) = self.test.take() else {
            self.screen = Screen::Home;
            return;
        };
        let pass = self.pass_score();
        let claude = self.claude();
        let title = if t.placement || t.retest { "레벨 테스트".to_string() } else { format!("{}주차 레벨 평가", t.week) };
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            ui.heading(title);
        });
        let total = content::PLACEMENT.len() + 1;
        ui.add(egui::ProgressBar::new((t.idx as f32 / total as f32).min(1.0)).desired_height(6.0).fill(ACCENT));
        ui.add_space(10.0);

        if t.idx < content::PLACEMENT.len() {
            ui.label(RichText::new("문장을 듣지 말고 바로 소리 내어 읽어 보세요. 어려우면 '모르겠어요'를 눌러도 됩니다.").color(MUTED));
            // Reading test: no audio model first — the learner reads cold.
            let ev = card(ui, |ui| t.drill.ui(ui, &mut self.voice, pass, None));
            let mut next = false;
            match ev {
                DrillEvent::Attempt(a) => {
                    t.readings.push((t.drill.target.clone(), a.transcript.clone(), a.score));
                    self.store.log_attempt("level-test", &t.drill.target, &a.transcript, a.score);
                    next = true;
                }
                DrillEvent::Completed => next = true,
                DrillEvent::None => {}
            }
            if ui.button("모르겠어요 (건너뛰기)").clicked() {
                t.readings.push((t.drill.target.clone(), String::new(), 0.0));
                next = true;
            }
            if next {
                t.idx += 1;
                if let Some((s, ko)) = content::PLACEMENT.get(t.idx) {
                    t.drill = Drill::new(s, ko, "", 1, false, false).without_autoplay();
                }
            }
        } else if t.result.is_none() && t.answer.is_none() {
            let (q, qko) = content::PLACEMENT_QUESTION;
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("마지막: 자유롭게 말하기 (30초~1분)").color(MUTED));
                    ui.horizontal(|ui| {
                        speaker_button(ui, &mut self.voice, q);
                        ui.label(RichText::new(q).size(20.0).strong());
                    });
                    ui.label(RichText::new(qko).color(MUTED));
                    ui.add_space(8.0);
                    if let Some(a) = t.capture.ui(ui, &mut self.voice, 75.0) {
                        t.answer = Some(a);
                    }
                    if ui.small_button("영어로 말하기 어려워요 (건너뛰기)").clicked() {
                        t.answer = Some(String::new());
                    }
                });
            });
            if let Some(a) = t.answer.clone() {
                self.store.log_attempt("level-test", q, &a, 0.0);
                match &claude {
                    Some(c) => {
                        let (c, r, cur) = (c.clone(), t.readings.clone(), self.store.profile.level);
                        t.job.spawn(move || c.assess_level(&r, q, &a, cur));
                    }
                    None => {
                        t.result = Some(LevelAssessment {
                            level: local_estimate(&t.readings),
                            summary_ko: "Claude API 키가 없어 읽기 정확도로만 추정한 레벨입니다. 키를 입력하면 말하기까지 평가해요.".into(),
                            ..Default::default()
                        })
                    }
                }
            }
        } else {
            if let Some(r) = t.job.poll() {
                match r {
                    Ok(a) => t.result = Some(a),
                    Err(e) => {
                        t.error = Some(format!("{e:#}"));
                        t.result = Some(LevelAssessment {
                            level: local_estimate(&t.readings),
                            summary_ko: "AI 평가에 실패해 읽기 정확도로 추정했습니다.".into(),
                            ..Default::default()
                        });
                    }
                }
            }
            if t.job.running() {
                spinner_label(ui, "Claude가 레벨을 평가하고 있어요…");
            }
            if let Some(e) = &t.error {
                error_label(ui, e);
            }
            if let Some(a) = t.result.clone() {
                card(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("당신의 레벨").color(MUTED));
                        ui.label(RichText::new(format!("{} {}  ({:.1}/10)", level_name_of(a.level), cefr_of(a.level), a.level)).size(40.0).strong().color(ACCENT));
                        if t.retest {
                            let cur = self.store.profile.level;
                            ui.label(RichText::new(format!("현재 {} {} ({cur:.1}) → 새 레벨 {} {} ({:.1})", level_name_of(cur), cefr_of(cur), level_name_of(a.level), cefr_of(a.level), a.level)).color(MUTED));
                        }
                        ui.label(RichText::new(&a.summary_ko).size(16.0));
                    });
                    for s in &a.strengths_ko {
                        ui.label(RichText::new(format!("👍 {s}")).color(GOOD));
                    }
                    for w in &a.weaknesses_ko {
                        ui.label(RichText::new(format!("🎯 {w}")).color(WARN));
                    }
                    if !a.focus_next_ko.is_empty() {
                        ui.label(RichText::new(format!("다음 목표: {}", a.focus_next_ko)).color(ACCENT));
                    }
                });
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    let go = if t.retest { "이 레벨로 이동 ▶" } else { "학습 시작하기 ▶" };
                    if ui.add(egui::Button::new(RichText::new(go).size(18.0)).fill(ACCENT).min_size(Vec2::new(220.0, 44.0))).clicked() {
                        if t.retest {
                            // The learner asked to be placed by this result: use it as is.
                            self.store.record_level(a.level, "레벨 테스트 (홈에서 재측정)");
                        } else if t.placement {
                            self.store.profile.placement_done = true;
                            self.store.profile.start_date = today();
                            self.store.record_level(a.level, "레벨 테스트 (시작)");
                        } else {
                            // Blend with the running level so one bad day doesn't swing it too far.
                            let lvl = self.store.profile.level * 0.3 + a.level * 0.7;
                            self.store.profile.last_checkpoint_week = t.week;
                            self.store.record_level(lvl, &format!("{}주차 레벨 평가", t.week));
                        }
                        self.screen = Screen::Home;
                        return;
                    }
                });
            }
        }
        if self.screen == Screen::LevelTest {
            if t.retest && t.result.is_some() && ui.small_button("지금 레벨 유지하기").clicked() {
                self.screen = Screen::Home;
                return;
            }
            if !t.placement && t.result.is_none() && ui.small_button("◀ 나중에 하기").clicked() {
                self.screen = Screen::Home;
                return;
            }
            self.test = Some(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_monotonic() {
        let low: Vec<_> = (0..6).map(|i| (String::new(), String::new(), if i < 1 { 90.0 } else { 10.0 })).collect();
        let high: Vec<_> = (0..6).map(|_| (String::new(), String::new(), 95.0)).collect();
        assert!(local_estimate(&low) < 3.0);
        assert!(local_estimate(&high) > 7.0);
    }
}
