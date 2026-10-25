//! Study history: every day's record (history.json) as a 6-month calendar and a day list,
//! so the learner (or anyone) can check what was actually studied and when.

use crate::app::{App, STEPS};
use crate::curriculum::TOTAL_DAYS;
use crate::models::*;
use crate::widgets::*;
use chrono::Duration;
use egui::{Color32, RichText, Sense, Stroke, Ui, Vec2};

fn step_label(key: &str) -> &str {
    STEPS.iter().find(|s| s.0 == key).map(|s| s.1).unwrap_or(key)
}

pub fn stat_tile(ui: &mut Ui, value: &str, label: &str, color: Color32) {
    stat_card(ui, 130.0, value, label, color, |_| {});
}

/// A stat tile of a fixed width with extra content under the label (progress bar, button…).
pub fn stat_card(ui: &mut Ui, width: f32, value: &str, label: &str, color: Color32, extra: impl FnOnce(&mut Ui)) {
    stat_card_with(ui, width, value, label, color, false, extra);
}

/// A stat card whose value and label sit in the middle, across and down (no extras).
pub fn stat_card_centered(ui: &mut Ui, width: f32, value: &str, label: &str, color: Color32) {
    stat_card_with(ui, width, value, label, color, true, |_| {});
}

fn stat_card_with(ui: &mut Ui, width: f32, value: &str, label: &str, color: Color32, centered: bool, extra: impl FnOnce(&mut Ui)) {
    // In a wrapping row a frame doesn't wrap by itself: start the next row when this tile
    // (width + 32 margin + 2 border) won't fit (phones).
    if ui.layout().main_wrap && ui.available_size_before_wrap().x < width + 34.0 && ui.cursor().left() > ui.max_rect().left() + 1.0 {
        ui.end_row();
    }
    // Tiles of one row share a height; rows (on a phone, tiles one under another) don't.
    let row_id = ui.id().with(ui.cursor().top().round() as i32);
    let card_id = row_id.with(("stat_card", label));
    egui::Frame::new().fill(card_bg()).corner_radius(14.0).inner_margin(egui::Margin::symmetric(16, 12)).stroke(Stroke::new(1.0_f32, border())).show(ui, |ui| {
        // Stack value, label and extras even when the tiles sit in a horizontal row.
        ui.vertical(|ui| {
            ui.set_width(width);
            // Same height for every tile in a row: each frame the tiles grow to the tallest
            // content of the previous frame (a wrapped label or a button makes one taller).
            let id = row_id.with("stat_row_h");
            let pass = ui.ctx().cumulative_pass_nr();
            let (seen, tallest, last) = ui.data(|d| d.get_temp::<(u64, f32, f32)>(id)).unwrap_or((pass, 0.0, 0.0));
            let (tallest, last) = if seen == pass { (tallest, last) } else { (0.0, tallest) };
            ui.set_min_height(last);
            let top = ui.min_rect().top();
            // Centred: as much space above the content as below it (content height from the last frame).
            let own: f32 = ui.data(|d| d.get_temp(card_id)).unwrap_or(0.0);
            if centered && last > own {
                ui.add_space((last - own) / 2.0);
            }
            let content_top = ui.cursor().top();
            let labels = |ui: &mut Ui| {
                ui.label(RichText::new(value).size(24.0).strong().color(color));
                ui.label(RichText::new(label).color(muted()));
            };
            if centered {
                ui.vertical_centered(labels);
            } else {
                labels(ui);
            }
            extra(ui);
            let content = ui.cursor().top() - ui.spacing().item_spacing.y - content_top;
            let natural = if centered { content } else { ui.min_rect().bottom().min(ui.cursor().top()) - top };
            if natural > last + 0.5 || (centered && (content - own).abs() > 0.5) {
                ui.ctx().request_repaint();
            }
            ui.data_mut(|d| {
                d.insert_temp(id, (pass, tallest.max(natural), last));
                d.insert_temp(card_id, content);
            });
        });
    });
}

impl App {
    pub fn history_ui(&mut self, ui: &mut Ui) {
        self.update_history();
        let hist = self.store.history.clone();
        let p = self.store.profile.clone();
        page_title(ui, "📅 학습 기록", "매일의 학습이 history.json 에 기록됩니다. 무엇을 언제 배웠는지 확인할 수 있어요.");

        let total_min: u32 = hist.iter().map(|r| r.minutes).sum();
        let total_attempts: u32 = hist.iter().map(|r| r.attempts).sum();
        let words: usize = hist.iter().map(|r| r.new_words.len()).sum();
        let week_ago = today() - Duration::days(7);
        let recent: Vec<&DayRecord> = hist.iter().filter(|r| r.date.is_some_and(|d| d > week_ago) && r.attempts > 0).collect();
        let recent_avg = if recent.is_empty() { 0.0 } else { recent.iter().map(|r| r.avg_score).sum::<f32>() / recent.len() as f32 };
        ui.horizontal_wrapped(|ui| {
            stat_tile(ui, &format!("{} / {TOTAL_DAYS}", hist.len()), "학습한 날", ACCENT);
            stat_tile(ui, &format!("{}시간 {}분", total_min / 60, total_min % 60), "총 학습 시간", GOOD);
            stat_tile(ui, &words.to_string(), "새로 배운 단어", warn());
            stat_tile(ui, &total_attempts.to_string(), "소리 내어 말한 횟수", Color32::from_rgb(190, 140, 255));
            stat_tile(ui, &format!("{recent_avg:.0}점"), "최근 7일 평균", GOOD);
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            ui.label(RichText::new("6개월 학습 달력").size(18.0).strong());
            ui.label(RichText::new("칸 색이 진할수록 많이 학습한 날입니다. 마우스를 올리면 그날 기록이 보여요.").color(muted()).small());
            ui.add_space(6.0);
            calendar(ui, p.start_date, &hist);
        });
        ui.add_space(10.0);

        ui.label(RichText::new("일별 기록").size(18.0).strong());
        if hist.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("아직 기록이 없습니다. 오늘 학습을 시작해 보세요!").color(muted()));
            });
        }
        let mut open_diary = None;
        for r in hist.iter().rev() {
            let Some(date) = r.date else { continue };
            let title = format!(
                "{date} · Day {} · {}  —  {}분 · {}회 발화 · 평균 {:.0}점 · 단계 {}/{}",
                r.program_day,
                r.theme,
                r.minutes,
                r.attempts,
                r.avg_score,
                r.steps.len(),
                STEPS.len()
            );
            egui::Frame::new().fill(card_bg()).corner_radius(12.0).inner_margin(12.0).stroke(Stroke::new(1.0_f32, border())).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::CollapsingHeader::new(RichText::new(title).size(15.0)).id_salt(date).default_open(date == today()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for s in &r.steps {
                            chip(ui, step_label(s), GOOD);
                        }
                    });
                    if !r.new_words.is_empty() {
                        ui.label(RichText::new(format!("📚 새 단어 {}개: {}", r.new_words.len(), r.new_words.join(", "))));
                    }
                    if r.new_sentences > 0 {
                        ui.label(format!("📖 새 문장 {}개", r.new_sentences));
                    }
                    if !r.question.is_empty() {
                        ui.label(format!("💬 자기 표현 질문: {}", r.question));
                    }
                    if !r.scenario.is_empty() {
                        ui.label(format!("💼 회화 상황: {}", r.scenario));
                    }
                    if r.speaking_minutes + r.listening_minutes + r.reading_minutes + r.writing_minutes > 0 {
                        ui.label(
                            RichText::new(format!(
                                "⏱ 말하기 {}분 · 듣기 {}분 · 읽기 {}분 · 쓰기 {}분",
                                r.speaking_minutes, r.listening_minutes, r.reading_minutes, r.writing_minutes
                            ))
                            .color(muted()),
                        );
                    }
                    if !r.reading.is_empty() {
                        ui.label(format!("📘 읽은 책: {}", r.reading.join(", ")));
                    }
                    if r.writings > 0 {
                        ui.label(format!("✏ 쓰기 {}개", r.writings));
                    }
                    if let Some(d) = self.diary_on(date) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("📝 영어 일기: {}", d.text.chars().take(60).collect::<String>()));
                            if ui.button("📅 일기장에서 보기").clicked() {
                                open_diary = Some(date);
                            }
                        });
                    }
                    if r.best_wpm > 0.0 {
                        ui.label(format!("⚡ 스피드 리딩 최고 {:.0} WPM", r.best_wpm));
                    }
                    ui.label(RichText::new(format!("레벨 {:.2} ({})", r.level, cefr_of(r.level))).color(muted()));
                });
            });
            ui.add_space(4.0);
        }
        if let Some(d) = open_diary {
            self.open_journal(d);
        }
        ui.add_space(6.0);
        if !cfg!(target_os = "android") && ui.button("📂 기록 파일 폴더 열기 (history.json · attempts.jsonl · level_log.csv)").clicked() {
            let _ = open::that(&self.store.dir);
        }
    }
}

/// A GitHub-style heatmap: 26 week columns × 7 day rows from the program start date.
fn calendar(ui: &mut Ui, start: chrono::NaiveDate, hist: &[DayRecord]) {
    // Smaller cells when the 26 weeks don't fit (phones).
    let gap = 4.0_f32.min(ui.available_width() / 26.0 * 0.2);
    let cell = 16.0_f32.min(ui.available_width() / 26.0 - gap);
    let size = Vec2::new(26.0 * (cell + gap), 7.0 * (cell + gap));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let mut hovered: Option<(chrono::NaiveDate, Option<&DayRecord>)> = None;
    for day in 0..TOTAL_DAYS {
        let date = start + Duration::days(day);
        let (col, row) = ((day / 7) as f32, (day % 7) as f32);
        let r = egui::Rect::from_min_size(rect.min + Vec2::new(col * (cell + gap), row * (cell + gap)), Vec2::splat(cell));
        let rec = hist.iter().find(|h| h.date == Some(date));
        let intensity = rec.map(|h| (h.minutes as f32 / 30.0).max(h.steps.len() as f32 / STEPS.len() as f32).min(1.0)).unwrap_or(0.0);
        let fill = if rec.is_some() {
            themed(Color32::from_rgb(30, 50, 90), Color32::from_rgb(205, 218, 248)).lerp_to_gamma(ACCENT, 0.25 + 0.75 * intensity)
        } else if date > today() {
            themed(Color32::from_rgb(20, 25, 38), Color32::from_rgb(236, 239, 245))
        } else {
            themed(Color32::from_rgb(32, 39, 58), Color32::from_rgb(224, 229, 240))
        };
        p.rect_filled(r, 4.0, fill);
        if date == today() {
            p.rect_stroke(r.expand(1.5), 5.0, Stroke::new(2.0_f32, warn()), egui::StrokeKind::Outside);
        }
        if resp.hover_pos().is_some_and(|h| r.contains(h)) {
            hovered = Some((date, rec));
        }
    }
    if let Some((date, rec)) = hovered {
        resp.on_hover_ui_at_pointer(|ui| {
            ui.label(RichText::new(date.to_string()).strong());
            match rec {
                Some(r) => {
                    ui.label(format!("{}분 · {}회 발화 · 평균 {:.0}점", r.minutes, r.attempts, r.avg_score));
                    ui.label(format!("완료 단계 {}/{}", r.steps.len(), STEPS.len()));
                }
                None => {
                    ui.label("기록 없음");
                }
            }
        });
    }
}
