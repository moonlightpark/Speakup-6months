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
            let goal = band_index_of_cefr(&self.store.settings.goal_cefr).unwrap_or(2);
            level_bands_table(ui, p.level, goal);
            ui.add_space(10.0);
            if let Some(g) = goal_consulting(ui, &p, goal, self.store.items.iter().filter(|i| i.is_vocab_word()).count()) {
                self.store.settings.goal_cefr = LEVEL_BANDS[g].2.to_string();
                self.store.save_settings();
            }
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
            ui.horizontal_wrapped(|ui| {
                for (label, v) in [
                    ("단어", words.to_string()),
                    ("문장", sents.to_string()),
                    ("장기 기억(21일+)", mastered.to_string()),
                    ("약점", weak.to_string()),
                    ("오늘 평균", format!("{avg:.0}점")),
                ] {
                    // A group doesn't wrap by itself: next line when it won't fit (phones).
                    if ui.available_size_before_wrap().x < 110.0 {
                        ui.end_row();
                    }
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
            ui.horizontal_wrapped(|ui| {
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
            wide(ui, "curr", |ui| {
egui::Grid::new("curr").striped(true).show(ui, |ui| {
                for (i, w) in curriculum::WEEKS.iter().enumerate() {
                    let n = i as u32 + 1;
                    let txt = format!("{n}주 · {} — {}", w.theme_ko, w.business);
                    ui.label(if n == cur { RichText::new(format!("▶ {txt}")).color(ACCENT).strong() } else if n < cur { RichText::new(txt).color(muted()) } else { RichText::new(txt) });
                    ui.end_row();
                }
            })
});
        });
    }

    pub fn vocabulary_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "📒 내 단어장", "");
        ui.horizontal_wrapped(|ui| {
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
        wide(ui, "vocab", |ui| {
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
        })
});
        if let Some(id) = delete {
            self.store.items.retain(|i| i.id != id);
            self.store.save_items();
        }
    }
}

/// 영어 실력 레벨 7단계: the whole ladder, with the learner's step and the 6-month goal marked.
fn level_bands_table(ui: &mut Ui, level: f32, goal: usize) {
    ui.label(RichText::new(format!("📶 영어 실력 레벨 {}단계", LEVEL_BANDS.len())).size(17.0).strong());
    ui.label(RichText::new("레벨 점수(1~10)로 단계가 정해지고, 레벨 테스트·주차 평가 결과로 올라가요. 단계에 맞춰 단어·지문·대화 난이도가 바뀝니다.").color(muted()).small());
    ui.add_space(4.0);
    let current = LEVEL_BANDS.iter().position(|b| level < b.0).unwrap_or(LEVEL_BANDS.len() - 1);
    wide(ui, "level_bands", |ui| {
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
            ui.horizontal_wrapped(|ui| {
                if here {
                    chip(ui, "◀ 지금 내 레벨", ACCENT);
                }
                if i == goal {
                    chip(ui, "🎯 내 목표", GOOD);
                }
            });
            ui.end_row();
        }
    })
});
}

/// 🎯 목표 레벨 컨설팅: study hours still needed from the learner's level to their goal, what
/// their own pace and a few daily amounts add up to by the end, and advice — all worked out
/// from the profile. Returns a newly chosen goal (index in `LEVEL_BANDS`).
fn goal_consulting(ui: &mut Ui, p: &Profile, goal: usize, words: usize) -> Option<usize> {
    let cur = band_index(p.level);
    let mut chosen = None;
    let day = curriculum::day_number(p);
    let left_days = (TOTAL_DAYS - day + 1).max(1) as f32;
    let studied_h = p.study_seconds as f32 / 3600.0;
    let pace_min = p.study_seconds as f32 / 60.0 / day.max(1) as f32;
    // Hours behind the learner when they started at this band (its range's middle).
    let (clo, chi) = CUM_HOURS[cur];
    let base_h = (clo + chi) as f32 / 2.0;
    let (glo, ghi) = CUM_HOURS[goal];
    let at_end = |daily_min: f32| base_h + studied_h + daily_min * left_days / 60.0;
    let reached = |h: f32| CUM_HOURS.iter().rposition(|(lo, _)| h >= *lo as f32).map(|i| LEVEL_BANDS[i].2).unwrap_or("A1");
    let verdict = |h: f32| {
        if goal <= cur {
            ("이미 도달", GOOD)
        } else if h >= ghi as f32 {
            ("목표 확실", GOOD)
        } else if h >= glo as f32 {
            ("목표 가능", GOOD)
        } else if h >= glo as f32 * 0.85 {
            ("목표 근접", warn())
        } else {
            ("목표 어려움", BAD)
        }
    };
    egui::Frame::new().fill(subtle_fill()).corner_radius(12.0).inner_margin(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("🎯 목표 레벨 컨설팅").size(17.0).strong());
            ui.label(RichText::new("내 목표:").color(muted()));
            for (i, b) in LEVEL_BANDS.iter().enumerate().skip(1) {
                let text = RichText::new(format!("🎯 {}", b.2)).strong();
                if ui.selectable_label(i == goal, if i == goal { text.color(GOOD) } else { text }).on_hover_text(format!("{} — {}", b.1, GOAL_DESC[i])).clicked() && i != goal {
                    chosen = Some(i);
                }
            }
        });
        ui.label(RichText::new(format!("{} 목표: {}", LEVEL_BANDS[goal].2, GOAL_DESC[goal])).color(ACCENT));
        ui.add_space(6.0);

        // 1. Hours to each level (reference).
        ui.label(RichText::new("레벨별 누적 학습 시간 (케임브리지 영어 등의 추정치 · 수업+연습 합계, 개인차가 큰 참고값)").strong());
        wide(ui, "cum_hours", |ui| {
egui::Grid::new("cum_hours").num_columns(3).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
            for (i, (lo, hi)) in CUM_HOURS.iter().enumerate().skip(1) {
                let mark = if i == cur { "◀ 지금" } else if i == goal { "🎯 목표" } else { "" };
                let color = if i == cur { ACCENT } else if i == goal { GOOD } else { text_strong() };
                ui.label(RichText::new(LEVEL_BANDS[i].2).strong().color(color));
                ui.label(RichText::new(format!("{lo}~{hi}시간")).color(color));
                ui.label(RichText::new(mark).color(color).small());
                ui.end_row();
            }
        })
});
        ui.add_space(6.0);

        // 2. What is still needed.
        if goal <= cur {
            ui.label(RichText::new(format!("👏 이미 목표({})에 도달했어요. 목표를 한 단계 올려 보세요!", LEVEL_BANDS[goal].2)).color(GOOD).strong());
        } else {
            let need_lo = (glo as f32 - chi as f32 - studied_h).max(0.0);
            let need_hi = (ghi as f32 - clo as f32 - studied_h).max(0.0);
            ui.label(RichText::new(format!(
                "지금은 {}({} · 레벨 {:.1})이고 앱에서 {:.0}시간 공부했어요. {}까지 약 {:.0}~{:.0}시간 더 필요해요.",
                LEVEL_BANDS[cur].2, LEVEL_BANDS[cur].1, p.level, studied_h, LEVEL_BANDS[goal].2, need_lo, need_hi
            )));
            for (i, (lo, hi)) in CUM_HOURS.iter().enumerate().skip(cur + 1).take(goal - cur) {
                if i != goal {
                    ui.label(RichText::new(format!("· {}까지 약 {:.0}~{:.0}시간", LEVEL_BANDS[i].2, (*lo as f32 - chi as f32 - studied_h).max(0.0), (*hi as f32 - clo as f32 - studied_h).max(0.0))).color(muted()));
                }
            }
        }
        ui.add_space(6.0);

        // 3. Where each daily amount leads by the end of the 6 months.
        ui.label(RichText::new(format!("남은 {:.0}일 동안 하루 학습량에 따라 쌓이는 시간", left_days)).strong());
        wide(ui, "pace_table", |ui| {
egui::Grid::new("pace_table").num_columns(4).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
            for h in ["하루 학습량", "6개월 끝 누적(추정)", "도달 예상", ""] {
                ui.label(RichText::new(h).color(muted()));
            }
            ui.end_row();
            let rows: [(String, f32); 4] = [
                ("앱 60분".into(), 60.0),
                ("앱 60분 + 듣기 30분 (권장)".into(), 90.0),
                ("하루 2시간".into(), 120.0),
                (format!("⭐ 지금 내 속도 (하루 평균 {pace_min:.0}분)"), pace_min),
            ];
            for (label, m) in rows {
                let h = at_end(m);
                let (v, c) = verdict(h);
                let mine = label.starts_with('⭐');
                ui.label(if mine { RichText::new(&label).strong().color(ACCENT) } else { RichText::new(&label) });
                ui.label(format!("약 {:.0}시간", h));
                ui.label(RichText::new(reached(h)).strong());
                ui.label(RichText::new(v).color(c).strong());
                ui.end_row();
            }
        })
});
        if goal > cur {
            let mid = (glo + ghi) as f32 / 2.0;
            let need_daily = ((mid - base_h - studied_h) * 60.0 / left_days).max(0.0);
            ui.label(RichText::new(format!("➡ {} 목표에 맞추려면 남은 기간 하루 평균 약 {:.0}분이 필요해요.", LEVEL_BANDS[goal].2, need_daily)).color(ACCENT).strong());
        }
        ui.add_space(6.0);

        // 4. Why speaking decides it.
        let vocab_end = words as f32 + 35.0 * left_days * 0.5;
        let goal_words = LEVEL_BANDS[goal].3;
        ui.label(RichText::new("어려운 건 말하기 유창성이에요").strong());
        ui.label(format!(
            "· 어휘: 지금 {words}단어 + 하루 35개씩 남은 {:.0}일(절반만 남아도) ≈ {:.0}단어 → {} 목표 어휘 {}단어 {}",
            left_days,
            vocab_end,
            LEVEL_BANDS[goal].2,
            crate::app::thousands(goal_words as u64),
            if vocab_end >= goal_words as f32 { "✓ 충분해요" } else { "— 단어 학습을 빼먹지 마세요" }
        ));
        ui.label("· 속도를 정하는 건 말하기예요. 의견과 이유를 길게 말하는 힘은 시간보다 실제로 말한 양이 쌓여야 늘어요.");
        ui.label("· 한국 성인 학습자는 읽기·어휘가 말하기보다 앞서는 경우가 많아요. 앱의 레벨은 말하기 평가가 따라와야 올라가요.");
        ui.add_space(6.0);

        // 5. Advice for this learner.
        ui.label(RichText::new("제안").strong());
        let next = (cur + 1).min(LEVEL_BANDS.len() - 1);
        if goal > next {
            ui.label(format!("· 목표를 \"기본 {}, 도전 {}\"로 생각하세요.", LEVEL_BANDS[next].2, LEVEL_BANDS[goal].2));
        }
        ui.label("· 하루 90분 이상(앱 60분 + 유튜브·팟캐스트 듣기 30분), 주 5일 이상");
        ui.label("· 자기 표현과 회화를 빼먹지 않기 (말하기 40% 비중 지키기)");
        ui.label("· 레벨 테스트를 자주 보기 — 레벨이 맞아야 단어·대화 난이도도 맞아요");
        if goal > cur {
            let check = LEVEL_BANDS[cur].0.min(LEVEL_BANDS[goal - 1].0);
            ui.label(format!("· 중간 점검: 3~4개월째(Day 90~120)에 레벨 {:.1} 이상({})이면 순조로운 흐름이에요.", check, LEVEL_BANDS[band_index(check)].2));
        }
        if pace_min >= 90.0 {
            ui.label(RichText::new(format!("· 지금 하루 평균 {pace_min:.0}분으로 권장량보다 많이 하고 있어요. 이 속도면 더 높은 목표도 좋아요!")).color(GOOD));
        } else if day > 3 {
            ui.label(RichText::new(format!("· 지금 하루 평균 {pace_min:.0}분이에요. 조금씩 늘려 하루 90분을 목표로 해 보세요.")).color(warn()));
        }
    });
    chosen
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
