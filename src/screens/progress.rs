//! Level & history (level chart, stats, checkpoints) and the learner's word/sentence deck.

use crate::app::{App, Screen};
use crate::curriculum::{self, TOTAL_DAYS};
use crate::models::*;
use crate::screens::test::LevelTest;
use crate::srs;
use crate::widgets::*;
use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};

impl App {
    pub fn progress_ui(&mut self, ui: &mut Ui) {
        let p = self.store.profile.clone();
        page_title(ui, "📈 레벨 · 기록", "");
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("현재 레벨 {} ({:.2}/10)", cefr_of(p.level), p.level)).size(22.0).strong().color(ACCENT));
            let day = curriculum::day_number(&p);
            ui.label(format!("시작일 {} · Day {day}/{TOTAL_DAYS} · 학습한 날 {}일 · 연속 {}일 · 총 {}분", p.start_date, p.days_studied, p.streak, p.study_seconds / 60));
            ui.add_space(8.0);
            level_bands_table(ui, p.level);
            ui.add_space(8.0);
            level_chart(ui, &p.level_history, p.start_date);
        });
        ui.add_space(6.0);
        self.coach_progress_ui(ui);
        ui.add_space(6.0);
        let words = self.store.items.iter().filter(|i| i.is_vocab_word()).count();
        let sents = self.store.items.iter().filter(|i| i.kind == ItemKind::Sentence).count();
        let mastered = self.store.items.iter().filter(|i| i.interval_days >= 21).count();
        let weak = self.store.items.iter().filter(|i| i.weak).count();
        let today_scores = &p.today.scores;
        let avg = if today_scores.is_empty() { 0.0 } else { today_scores.iter().sum::<f32>() / today_scores.len() as f32 };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, v) in [
                    ("단어", words.to_string()),
                    ("문장", sents.to_string()),
                    ("장기 기억(21일+)", mastered.to_string()),
                    ("약점", weak.to_string()),
                    ("오늘 평균", format!("{avg:.0}점")),
                ] {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(v).size(24.0).strong());
                        ui.label(RichText::new(label).color(muted()));
                    });
                    ui.add_space(18.0);
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new("월말 레벨 평가").strong());
            ui.horizontal_wrapped(|ui| {
                for w in curriculum::CHECKPOINT_WEEKS {
                    let done = *w <= p.last_checkpoint_week;
                    ui.label(RichText::new(format!("{}{w}주차", if done { "✅ " } else { "⬜ " })).color(if done { GOOD } else { muted() }));
                }
            });
            ui.horizontal(|ui| {
                let due = curriculum::checkpoint_due(&p);
                let label = match due {
                    Some(w) => format!("📋 {w}주차 평가 보기"),
                    None => "📋 지금 레벨 평가 보기 (연습)".into(),
                };
                if ui.button(label).clicked() {
                    self.test = Some(LevelTest::new(false, due.unwrap_or(p.last_checkpoint_week)));
                    self.go(Screen::LevelTest);
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new("레벨 변경 기록 (level_log.csv)").strong());
            for r in p.level_history.iter().rev().take(15) {
                ui.label(format!("{}  {:.2} ({})  — {}", r.date, r.level, r.cefr, r.reason));
            }
            if p.level_history.is_empty() {
                ui.label(RichText::new("아직 기록이 없습니다.").color(muted()));
            }
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new("6개월 커리큘럼").strong());
            let cur = curriculum::week_number(&p);
            egui::Grid::new("curr").striped(true).show(ui, |ui| {
                for (i, w) in curriculum::WEEKS.iter().enumerate() {
                    let n = i as u32 + 1;
                    let txt = format!("{n}주 · {} — {}", w.theme_ko, w.business);
                    ui.label(if n == cur { RichText::new(format!("▶ {txt}")).color(ACCENT).strong() } else if n < cur { RichText::new(txt).color(muted()) } else { RichText::new(txt) });
                    ui.end_row();
                }
            });
        });
    }

    pub fn vocabulary_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "📒 내 단어장", "");
        ui.horizontal(|ui| {
            for (i, label) in ["전체", "약점", "오늘 복습", "단어", "문장"].iter().enumerate() {
                ui.selectable_value(&mut self.vocab_mode, i, *label);
            }
            ui.add(egui::TextEdit::singleline(&mut self.vocab_filter).hint_text("검색").desired_width(180.0));
        });
        let t = today();
        let filter = self.vocab_filter.to_lowercase();
        let mode = self.vocab_mode;
        let mut items: Vec<StudyItem> = self
            .store
            .items
            .iter()
            .filter(|i| match mode {
                1 => i.weak,
                2 => i.due <= t,
                3 => i.kind == ItemKind::Word,
                4 => i.kind == ItemKind::Sentence,
                _ => true,
            })
            .filter(|i| filter.is_empty() || i.text.to_lowercase().contains(&filter) || i.meaning_ko.contains(&filter))
            .cloned()
            .collect();
        items.sort_by_key(|i| (i.due, i.text.clone()));
        ui.label(RichText::new(format!("{}개 · 오늘 복습 {}개", items.len(), srs::due_count(&self.store.items))).color(muted()));
        ui.add_space(6.0);
        let mut delete: Option<String> = None;
        egui::Grid::new("vocab").striped(true).num_columns(5).show(ui, |ui| {
            ui.label(RichText::new("").strong());
            ui.label(RichText::new("표현").strong());
            ui.label(RichText::new("뜻").strong());
            ui.label(RichText::new("다음 복습 · 최근 점수").strong());
            ui.label("");
            ui.end_row();
            for i in &items {
                speaker_button(ui, &mut self.voice, &i.text);
                ui.label(RichText::new(&i.text).color(if i.weak { warn() } else { text_strong() }));
                crate::grammar::ko_label(ui, &i.meaning_ko, 16.0, muted());
                let due = if i.due <= t { "오늘".to_string() } else { format!("{} ({}일 뒤)", i.due, (i.due - t).num_days()) };
                ui.label(format!("{due} · {:.0}점{}", i.last_score, if i.weak { " · 약점" } else { "" }));
                if ui.small_button("🗑").on_hover_text("삭제").clicked() {
                    delete = Some(i.id.clone());
                }
                ui.end_row();
            }
        });
        if let Some(id) = delete {
            self.store.items.retain(|i| i.id != id);
            self.store.save_items();
        }
    }
}

/// 영어 실력 레벨 7단계: the whole ladder, with the learner's step and the 6-month goal marked.
fn level_bands_table(ui: &mut Ui, level: f32) {
    ui.label(RichText::new(format!("📶 영어 실력 레벨 {}단계", LEVEL_BANDS.len())).size(17.0).strong());
    ui.label(RichText::new("레벨 점수(1~10)로 단계가 정해지고, 레벨 테스트·주차 평가 결과로 올라가요. 단계에 맞춰 단어·지문·대화 난이도가 바뀝니다.").color(muted()).small());
    ui.add_space(4.0);
    let current = LEVEL_BANDS.iter().position(|b| level < b.0).unwrap_or(LEVEL_BANDS.len() - 1);
    egui::Grid::new("level_bands").num_columns(6).spacing([22.0, 6.0]).striped(true).show(ui, |ui| {
        for h in ["단계", "이름", "CEFR", "레벨 점수", "목표 어휘", ""] {
            ui.label(RichText::new(h).strong().color(muted()));
        }
        ui.end_row();
        for (i, (upper, name, cefr, words)) in LEVEL_BANDS.iter().enumerate() {
            let lower = if i == 0 { None } else { Some(LEVEL_BANDS[i - 1].0) };
            let range = match (lower, *upper < f32::MAX) {
                (None, _) => format!("{upper:.1} 미만"),
                (Some(l), true) => format!("{l:.1} ~ {upper:.1} 미만"),
                (Some(l), false) => format!("{l:.1} 이상"),
            };
            let here = i == current;
            let color = if here { ACCENT } else { text_strong() };
            let cell = |ui: &mut Ui, t: String| {
                let rt = RichText::new(t).color(color);
                ui.label(if here { rt.strong() } else { rt });
            };
            cell(ui, (i + 1).to_string());
            cell(ui, name.to_string());
            cell(ui, cefr.to_string());
            cell(ui, range);
            cell(ui, format!("{}단어", crate::app::thousands(*words as u64)));
            ui.horizontal(|ui| {
                if here {
                    chip(ui, "◀ 지금 내 레벨", ACCENT);
                }
                if *cefr == "B1" {
                    chip(ui, "🎯 6개월 목표", GOOD);
                }
            });
            ui.end_row();
        }
    });
}

fn level_chart(ui: &mut Ui, history: &[LevelRecord], start: chrono::NaiveDate) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 180.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, themed(Color32::from_rgb(16, 21, 34), Color32::from_rgb(246, 248, 252)));
    let x_of = |d: chrono::NaiveDate| rect.left() + 30.0 + (rect.width() - 40.0) * ((d - start).num_days() as f32 / TOTAL_DAYS as f32).clamp(0.0, 1.0);
    let y_of = |l: f32| rect.bottom() - 16.0 - (rect.height() - 30.0) * ((l - 1.0) / 9.0);
    for (l, name) in [(2.0, "A2"), (3.5, "B1"), (6.5, "B2"), (9.0, "C1")] {
        let y = y_of(l);
        p.line_segment([Pos2::new(rect.left() + 28.0, y), Pos2::new(rect.right() - 8.0, y)], Stroke::new(1.0_f32, themed(Color32::from_rgb(40, 48, 70), Color32::from_rgb(222, 227, 238))));
        p.text(Pos2::new(rect.left() + 4.0, y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(11.0), muted());
    }
    // Pace line toward B2 by the end of the program.
    if let Some(first) = history.first() {
        let goal = curriculum::target_level_for_day(TOTAL_DAYS, first.level);
        p.line_segment(
            [Pos2::new(x_of(start), y_of(first.level)), Pos2::new(x_of(start + chrono::Duration::days(TOTAL_DAYS)), y_of(goal))],
            Stroke::new(1.5_f32, warn().gamma_multiply(0.5)),
        );
    }
    let pts: Vec<Pos2> = history.iter().map(|r| Pos2::new(x_of(r.date), y_of(r.level))).collect();
    if pts.len() > 1 {
        p.add(egui::Shape::line(pts.clone(), Stroke::new(2.5_f32, ACCENT)));
    }
    for pt in pts {
        p.circle_filled(pt, 3.5, ACCENT);
    }
}
