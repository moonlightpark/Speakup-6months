//! 📐 문법: the grammar points of each level (built-in, `grammar_book`) — explanation and
//! pattern, examples to listen to and say aloud, and a short quiz that marks the point learned.

use crate::app::App;
use crate::grammar_book::{self, Point};
use crate::models::*;
use crate::widgets::*;
use egui::{Color32, RichText, Ui};

#[derive(Default)]
pub struct GrammarState {
    /// Level band shown (index into `LEVEL_BANDS`); None = the learner's own level.
    level: Option<usize>,
    /// Grammar point opened (index into the level's points).
    open: Option<usize>,
    /// Picked choice for each quiz question of the open point.
    picks: Vec<Option<usize>>,
    drill: Option<Drill>,
}

impl GrammarState {
    fn open(&mut self, i: Option<usize>) {
        self.open = i;
        self.picks.clear();
        self.drill = None;
    }
}

impl App {
    pub fn grammar_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "📐 문법", "레벨(N5~N1)에 맞는 핵심 문법을 설명 · 예문 · 퀴즈로 익혀요. 예문은 듣고 소리 내어 따라 말해 보세요.");
        let mine = band_index(self.store.profile.level);
        let band = self.grammar.level.unwrap_or(mine);

        // Level tabs, with how many points of each are done.
        ui.horizontal_wrapped(|ui| {
            for (i, b) in LEVEL_BANDS.iter().enumerate() {
                let Some(l) = grammar_book::level(b.2) else { continue };
                let done = l.points.iter().filter(|p| self.grammar_done(b.2, p)).count();
                let star = if i == mine { "★ " } else { "" };
                let text = format!("{star}{} {} {done}/{}", b.2, b.1, l.points.len());
                let selected = i == band;
                let btn = egui::Button::new(RichText::new(text).color(if selected { Color32::WHITE } else { text_strong() })).fill(if selected { ACCENT } else { subtle_fill() });
                if ui.add(btn).clicked() && !selected {
                    self.grammar.level = Some(i);
                    self.grammar.open(None);
                }
            }
        });
        ui.label(RichText::new("★ 내 레벨").small().color(muted()));
        ui.add_space(8.0);

        let cefr = LEVEL_BANDS[band].2;
        let Some(level) = grammar_book::level(cefr) else { return };
        match self.grammar.open.filter(|&i| i < level.points.len()) {
            None => self.grammar_list_ui(ui, cefr, &level.points),
            Some(i) => self.grammar_point_ui(ui, cefr, &level.points, i),
        }
    }

    fn grammar_done(&self, cefr: &str, p: &Point) -> bool {
        self.store.profile.grammar_done.contains(&p.key(cefr))
    }

    fn grammar_list_ui(&mut self, ui: &mut Ui, cefr: &str, points: &[Point]) {
        let mut open = None;
        for (i, p) in points.iter().enumerate() {
            let done = self.grammar_done(cefr, p);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(if done { "✅" } else { "⭕" }).size(18.0));
                    ui.label(RichText::new(format!("{}. {}", i + 1, p.title)).size(19.0).strong().color(text_strong()));
                });
                ui.add(egui::Label::new(RichText::new(&p.form).color(muted())).wrap());
                ui.add_space(4.0);
                if ui.button(if done { "다시 보기 ›" } else { "학습하기 ›" }).clicked() {
                    open = Some(i);
                }
            });
        }
        if open.is_some() {
            self.grammar.open(open);
        }
    }

    fn grammar_point_ui(&mut self, ui: &mut Ui, cefr: &str, points: &[Point], i: usize) {
        let p = &points[i];
        if ui.button("← 문법 목록").clicked() {
            self.grammar.open(None);
            return;
        }
        ui.add_space(6.0);

        // Explanation and pattern.
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                chip(ui, cefr, ACCENT);
                ui.add(egui::Label::new(RichText::new(&p.title).size(22.0).strong().color(text_strong())).wrap());
            });
            ui.add_space(6.0);
            egui::Frame::new().fill(subtle_fill()).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(format!("📌 {}", p.form)).size(16.0).strong().color(gloss())).wrap());
            });
            ui.add_space(6.0);
            for line in &p.explain {
                ui.add(egui::Label::new(RichText::new(line).size(16.0)).wrap());
            }
        });

        // Examples: listen, slow, say it.
        let mut practice: Option<(String, String)> = None;
        card(ui, |ui| {
            ui.label(RichText::new("🔊 예문 — 듣고 따라 말해 보세요").strong());
            ui.add_space(4.0);
            for ex in &p.examples {
                ja_line(ui, &ex.ja, 20.0);
                crate::grammar::ko_label(ui, &ex.ko, 15.0, muted());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("🔊 듣기").clicked() {
                        self.voice.say(&ex.ja);
                    }
                    if ui.button("🐢 천천히").clicked() {
                        self.voice.say_slow(&ex.ja);
                    }
                    if ui.button("🎤 따라 말하기").clicked() {
                        practice = Some((ex.ja.clone(), ex.ko.clone()));
                    }
                });
                ui.add_space(8.0);
            }
        });
        if let Some((ja, ko)) = practice {
            self.grammar.drill = Some(Drill::new(&ja, &ko, &p.form, self.store.settings.reps_new, false, false));
        }
        let claude = self.claude();
        let pass = self.pass_score();
        let mut ev = DrillEvent::None;
        if let Some(d) = self.grammar.drill.as_mut() {
            ev = card(ui, |ui| d.ui(ui, &mut self.voice, pass, claude.as_ref()));
        }
        match ev {
            DrillEvent::Attempt(a) => {
                let t = self.grammar.drill.as_ref().map(|d| d.target.clone()).unwrap_or_default();
                self.record_attempt("grammar", &t, &a);
            }
            DrillEvent::Completed => {
                self.grammar.drill = None;
                self.toast("👏 잘했어요! 다른 예문도 말해 보세요.");
            }
            DrillEvent::None => {}
        }

        // Quiz: all right = this point is learned.
        self.grammar.picks.resize(p.quiz.len(), None);
        let mut pick: Option<(usize, usize)> = None;
        card(ui, |ui| {
            ui.label(RichText::new("✏ 확인 퀴즈 — 빈칸에 알맞은 것을 고르세요").strong());
            ui.add_space(4.0);
            for (qi, q) in p.quiz.iter().enumerate() {
                let picked = self.grammar.picks[qi];
                ja_line(ui, &format!("{}. {}", qi + 1, q.sentence), 19.0);
                ui.horizontal_wrapped(|ui| {
                    for c in grammar_book::choice_order(q) {
                        let mut b = egui::Button::new(RichText::new(&q.choices[c]).size(17.0)).min_size(egui::vec2(80.0, 36.0));
                        if let Some(pk) = picked {
                            if c == 0 {
                                b = b.fill(GOOD);
                            } else if c == pk {
                                b = b.fill(BAD);
                            }
                        }
                        if ui.add_enabled(picked.is_none() || c == 0 || Some(c) == picked, b).clicked() && picked.is_none() {
                            pick = Some((qi, c));
                        }
                    }
                });
                if let Some(pk) = picked {
                    let (mark, color) = if pk == 0 { ("⭕ 정답", GOOD) } else { ("❌ 오답", BAD) };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(mark).strong().color(color));
                        ui.label(RichText::new(&q.ko).color(muted()));
                    });
                }
                ui.add_space(8.0);
            }
        });
        if let Some((qi, c)) = pick {
            self.grammar.picks[qi] = Some(c);
            let q = &p.quiz[qi];
            let full = q.sentence.replacen("___", &q.choices[0], 1);
            self.voice.say(&full);
            let all = self.grammar.picks.iter().all(Option::is_some);
            let right = self.grammar.picks.iter().all(|x| *x == Some(0));
            if all && right {
                // One grammar point with its quiz all right (new or again) is today's 문법 step.
                if !self.grammar_done(cefr, p) {
                    self.store.profile.grammar_done.push(p.key(cefr));
                    self.store.save_profile();
                    self.toast("🎉 이 문법을 익혔어요!");
                }
                self.complete_step("grammar");
            }
        }

        let answered = self.grammar.picks.iter().all(Option::is_some);
        let wrong = self.grammar.picks.iter().any(|x| x.is_some_and(|c| c != 0));
        ui.horizontal_wrapped(|ui| {
            if answered && wrong && ui.button("🔄 퀴즈 다시 풀기").clicked() {
                self.grammar.picks.clear();
            }
            if i + 1 < points.len() && ui.add(egui::Button::new(RichText::new("다음 문법 ›").strong().color(Color32::WHITE)).fill(ACCENT)).clicked() {
                self.grammar.open(Some(i + 1));
            }
        });
    }
}

/// Japanese with furigana, wrapped, named by its text (for screen readers and tests).
fn ja_line(ui: &mut Ui, text: &str, size: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.y = size * 0.3;
        ja_entry(ui, text, "", size, text_strong());
    });
}

#[cfg(test)]
impl App {
    /// The sentence being said in the 문법 drill (functional tests).
    pub fn grammar_drill_target(&self) -> Option<String> {
        self.grammar.drill.as_ref().map(|d| d.target.clone())
    }
}
