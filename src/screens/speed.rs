//! Speed training (Order.md: "빨리 읽고 빨리 말할 수 있는 훈련").
//! 1) Speed reading aloud — repeated reading of a short passage, measuring words per minute
//!    over correctly read words, against a level-based target.
//! 2) Quick response — a Korean prompt appears, the mic opens automatically, and the learner
//!    must say the English within a time limit.

use crate::app::App;
use crate::audio;
use crate::claude::SpeedPassage;
use crate::content;
use crate::curriculum;
use crate::models::*;
use crate::scoring::{self, ScoreResult};
use crate::widgets::*;
use egui::{RichText, Ui, Vec2};
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
    // quick response
    pub queue: Vec<(String, String)>,
    pub idx: usize,
    pub shown_at: Option<Instant>,
    pub rcap: Capture,
    pub result: Option<(String, ScoreResult, f32, bool)>,
    pub stats: Vec<(f32, f32, bool)>,
}

/// Target reading speed for a level: ~100 WPM for beginners up to ~160 (natural native pace).
pub fn target_wpm(level: f32) -> f32 {
    (80.0 + level * 9.0).min(165.0)
}

fn time_limit(target: &str) -> f32 {
    2.5 + 0.45 * target.split_whitespace().count() as f32
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
            ui.selectable_value(&mut self.speed.tab, 0, "📖 스피드 리딩 (WPM)");
            ui.selectable_value(&mut self.speed.tab, 1, "💨 순발력 말하기");
            ui.selectable_value(&mut self.speed.tab, 2, "⏱ 4/3/2 말하기");
            ui.label(RichText::new(format!("오늘 {}/3 라운드", self.store.profile.today.speed_rounds.min(3))).color(MUTED));
        });
        ui.add_space(6.0);
        match self.speed.tab {
            0 => self.speed_reading_ui(ui),
            1 => self.quick_response_ui(ui),
            _ => self.four_three_two_ui(ui),
        }
    }

    fn new_passage(&mut self) {
        self.speed.attempts.clear();
        self.speed.last = None;
        self.speed.error = None;
        let p = &self.store.profile;
        let theme = curriculum::week(curriculum::week_number(p)).theme.to_string();
        match self.claude() {
            Some(c) => {
                let level = p.level;
                let mut known: Vec<String> = self.store.items.iter().filter(|i| i.kind == ItemKind::Word).map(|i| i.text.clone()).collect();
                known.shuffle(&mut rand::rng());
                known.truncate(15);
                let mut avoid: Vec<String> = self.store.profile.speed_history.iter().filter(|r| !r.title.is_empty()).map(|r| r.title.clone()).collect();
                avoid.dedup();
                self.speed.job.spawn(move || c.speed_passage(&theme, level, &known, &avoid));
            }
            None => {
                let read: Vec<&str> = p.speed_history.iter().map(|r| r.title.as_str()).collect();
                let i = (0..content::SPEED_PASSAGES.len())
                    .find(|i| !read.contains(&content::SPEED_PASSAGES[*i].0))
                    .unwrap_or(curriculum::day_number(p) as usize % content::SPEED_PASSAGES.len());
                let (t, text) = content::SPEED_PASSAGES[i];
                self.speed.passage = Some(SpeedPassage { title: t.into(), passage: text.into(), passage_ko: String::new(), chunks: Vec::new() });
            }
        }
    }

    fn speed_reading_ui(&mut self, ui: &mut Ui) {
        if let Some(r) = self.speed.job.poll() {
            match r {
                Ok(mut p) => {
                    self.store.put_gloss(&p.passage, &p.chunks);
                    p.chunks = self.store.gloss_for(&p.passage);
                    self.speed.show_gloss = !p.chunks.is_empty() && support_stage(ui.ctx()) == 0;
                    self.speed.passage = Some(p);
                }
                Err(e) => self.speed.error = Some(format!("{e:#}")),
            }
        }
        let level = self.store.profile.level;
        let target = target_wpm(level);
        let best = self.store.profile.speed_history.iter().filter(|r| r.kind == "reading" && r.accuracy >= 85.0).map(|r| r.wpm).fold(0.0f32, f32::max);
        card(ui, |ui| {
            ui.label(format!("방법: ① 원어민 음성 듣기 → ② 정확하게 한 번 읽기 → ③ 같은 지문을 점점 더 빠르게 2~3번 더 읽기 (반복 읽기)"));
            ui.label(RichText::new(format!("목표: 정확도 85% 이상 + {target:.0} WPM 이상 · 내 최고 {best:.0} WPM · 원어민 대화 속도 약 150 WPM")).color(ACCENT));
        });
        ui.add_space(6.0);
        if self.speed.passage.is_none() {
            if self.speed.job.running() {
                spinner_label(ui, "Claude가 내 레벨에 맞는 지문을 만들고 있어요…");
            } else if ui.add(egui::Button::new(RichText::new("▶ 지문 받기").size(18.0)).fill(ACCENT).min_size(Vec2::new(180.0, 42.0))).clicked() {
                self.new_passage();
            }
            if let Some(e) = &self.speed.error {
                error_label(ui, e);
            }
            return;
        }
        let p = self.speed.passage.clone().unwrap();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&p.title).size(20.0).strong());
                if ui.button("🔊 원어민 속도").clicked() {
                    self.voice.say(&p.passage);
                }
                if ui.button("🐇 빠르게").clicked() {
                    self.voice.say_fast(&p.passage);
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
            // One sentence per line, so the eyes can run down the passage quickly.
            if self.speed.show_gloss && !p.chunks.is_empty() {
                chunk_view_lines(ui, &p.chunks, marks, 20.0);
            } else {
                match &self.speed.last {
                    Some((_, r, _)) => word_feedback_lines(ui, r, 20.0),
                    None => sentence_lines(ui, &p.passage, 20.0, egui::Color32::WHITE),
                }
            }
            if self.speed.show_ko {
                ui.add_space(4.0);
                sentence_lines(ui, &p.passage_ko, 16.0, MUTED);
            }
        });
        ui.add_space(6.0);
        let mut new_result = None;
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                if let Some(t) = self.speed.capture.ui(ui, &mut self.voice, 120.0) {
                    new_result = Some(t);
                }
            });
        });
        if let Some(t) = new_result {
            let r = scoring::score(&p.passage, &t);
            let correct = r.marks.iter().filter(|m| m.ok).count();
            let wpm = audio::wpm(correct, self.speed.capture.last_secs);
            let missed: Vec<String> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect();
            self.record_attempt("speed-reading", &p.passage, &Attempt { score: r.score, transcript: t.clone(), missed });
            self.store.profile.speed_history.push(SpeedRecord { date: today(), kind: "reading".into(), wpm, accuracy: r.score, seconds: self.speed.capture.last_secs, title: p.title.clone() });
            self.speed.attempts.push((wpm, r.score));
            self.speed.last = Some((t, r, wpm));
            self.speed_round_done();
        }
        if let Some((_, r, wpm)) = &self.speed.last {
            let ok = r.score >= 85.0 && *wpm >= target;
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{wpm:.0} WPM")).size(34.0).strong().color(if *wpm >= target { GOOD } else { WARN }));
                    ui.add_space(16.0);
                    ui.label(RichText::new(format!("정확도 {:.0}%", r.score)).size(24.0).color(if r.score >= 85.0 { GOOD } else { BAD }));
                });
                ui.label(if ok {
                    RichText::new("🎯 목표 달성! 한 번 더 더 빠르게 읽거나 새 지문에 도전하세요.").color(GOOD)
                } else if r.score < 85.0 {
                    RichText::new("속도보다 정확도가 먼저예요. 빨간 단어를 듣고 다시 읽어 보세요.").color(WARN)
                } else {
                    RichText::new(format!("정확해요! 이제 {:.0} WPM까지 속도를 올려 봅시다 — 문장 사이 쉬는 시간을 줄이세요.", target)).color(WARN)
                });
                if self.speed.attempts.len() > 1 {
                    let trail: Vec<String> = self.speed.attempts.iter().enumerate().map(|(i, (w, a))| format!("{}회 {w:.0}({a:.0}%)", i + 1)).collect();
                    ui.label(RichText::new(format!("반복 읽기: {}", trail.join(" → "))).color(MUTED));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("↻ 같은 지문 더 빠르게").clicked() {
                    self.speed.last = None;
                }
                if ui.button("새 지문").clicked() {
                    self.speed.passage = None;
                    self.new_passage();
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
            pool.extend(content::PLACEMENT.iter().take(4).map(|(e, k)| (e.to_string(), k.to_string())));
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
        card(ui, |ui| {
            ui.label("한국어가 나오면 마이크가 자동으로 켜집니다. 생각나는 즉시 영어로 말하세요 — 말이 끝나면 자동으로 채점돼요.");
            ui.label(RichText::new("배운 단어·문장에서 출제됩니다. 제한 시간 안에 정확히 말해야 통과.").color(MUTED));
        });
        ui.add_space(6.0);
        if self.speed.queue.is_empty() {
            if ui.add(egui::Button::new(RichText::new("▶ 시작 (10문제)").size(18.0)).fill(ACCENT).min_size(Vec2::new(180.0, 42.0))).clicked() {
                self.start_quick_response();
            }
            return;
        }
        if self.speed.idx >= self.speed.queue.len() {
            let n = self.speed.stats.len().max(1) as f32;
            let avg_t = self.speed.stats.iter().map(|s| s.0).sum::<f32>() / n;
            let avg_a = self.speed.stats.iter().map(|s| s.1).sum::<f32>() / n;
            let ok = self.speed.stats.iter().filter(|s| s.2).count();
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
        ui.label(RichText::new(format!("{} / {}", self.speed.idx + 1, self.speed.queue.len())).color(MUTED));
        let mut next = false;
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                crate::grammar::ko_label(ui, &ko, 28.0, egui::Color32::WHITE);
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
                    ui.label(RichText::new(format!("들린 말: \"{heard}\"")).color(MUTED));
                    ui.label(
                        RichText::new(format!("{} · {:.1}초 (제한 {:.1}초) · 정확도 {:.0}%", if *ok { "✅ 통과" } else { "❌ 다시 연습" }, secs, limit, r.score))
                            .size(18.0)
                            .color(if *ok { GOOD } else { WARN }),
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
        assert!(target_wpm(10.0) <= 165.0);
        assert!(time_limit("I'm fine") < time_limit("Could you send me the report by Friday afternoon?"));
    }
}
