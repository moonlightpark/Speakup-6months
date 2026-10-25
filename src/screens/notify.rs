//! 🔔 Notifications (top right, every few minutes, 설정 > 알림 간격): in turn, what is still
//! unfinished (words, sentences), which week of the program this is, how far along it is and
//! what has been learned, and how to reach the next level sooner. A click opens the screen.

use crate::app::{App, Screen};
use crate::curriculum::{self, TOTAL_DAYS};
use crate::models::*;
use crate::srs;
use crate::widgets::*;
use egui::{Color32, RichText, Vec2};
use std::time::{Duration, Instant};

pub struct Notice {
    pub icon: &'static str,
    pub title: String,
    pub body: String,
    pub go: Option<(Screen, &'static str)>,
}

pub struct Notifier {
    pub shown: Option<(Notice, Instant)>,
    pub next_at: Instant,
    /// Which kind comes next (they take turns).
    pub idx: usize,
}

impl Default for Notifier {
    fn default() -> Self {
        Self { shown: None, next_at: Instant::now() + FIRST_AFTER, idx: 0 }
    }
}

/// The first notification comes this long after the app opens.
const FIRST_AFTER: Duration = Duration::from_secs(20);
/// A notification stays this long (longer while the mouse is on it).
const SHOW_FOR: Duration = Duration::from_secs(12);
/// Kinds of notification, shown in turn.
const KINDS: usize = 5;

/// "a, b, c 외 N개".
fn sample(list: &[String], n: usize) -> String {
    let head: Vec<&str> = list.iter().take(n).map(|s| s.as_str()).collect();
    let more = list.len().saturating_sub(n);
    if more > 0 { format!("{} 외 {more}개", head.join(", ")) } else { head.join(", ") }
}

impl App {
    /// The notification of `kind`, or None when it has nothing to say now.
    pub fn notice(&mut self, kind: usize) -> Option<Notice> {
        let p = &self.store.profile;
        let day = curriculum::day_number(p);
        let week_n = curriculum::week_number(p);
        let week = curriculum::week(week_n);
        match kind {
            0 => {
                let (words, _) = self.unfinished_items();
                (!words.is_empty()).then(|| Notice {
                    icon: "📚",
                    title: format!("아직 못 끝낸 단어 {}개", words.len()),
                    body: format!("{} — 소리 내어 연습하면 기억 완료에 가까워져요.", sample(&words, 4)),
                    go: Some((Screen::Words, "📚 새 단어로 ▶")),
                })
            }
            1 => {
                let (_, sentences) = self.unfinished_items();
                (!sentences.is_empty()).then(|| Notice {
                    icon: "📖",
                    title: format!("아직 못 끝낸 문장 {}개", sentences.len()),
                    body: format!("{} — 문장 읽기에서 이어서 읽어요.", sample(&sentences, 2)),
                    go: Some((Screen::Reading, "📖 문장 읽기로 ▶")),
                })
            }
            2 => {
                let left: Vec<&str> = self.todays_steps().iter().filter(|s| !self.step_done(s.0)).map(|s| s.1).collect();
                let today = if left.is_empty() { "오늘 학습을 모두 마쳤어요! 👏".to_string() } else { format!("오늘 남은 학습 {}개: {}", left.len(), left.iter().take(3).copied().collect::<Vec<_>>().join(" → ")) };
                Some(Notice {
                    icon: "📅",
                    title: format!("{week_n}주차 프로그램 · {}", week.theme_ko),
                    body: format!(
                        "6개월 과정 {day}일차 / {TOTAL_DAYS}일 ({:.0}%). 이번 주 문법: {}.\n{today}",
                        day as f32 / TOTAL_DAYS as f32 * 100.0,
                        week.grammar
                    ),
                    go: Some((Screen::Home, "🏠 홈으로 ▶")),
                })
            }
            3 => {
                let p = &self.store.profile;
                let words = self.item_progress("words").map(|x| (x.0, x.2)).unwrap_or_default();
                let idioms = self.item_progress("reading").map(|x| (x.0, x.2)).unwrap_or_default();
                let hours = p.study_seconds as f32 / 3600.0;
                let mut recent: Vec<&StudyItem> = self.store.items.iter().filter(|i| !i.known).collect();
                recent.sort_by(|a, b| b.added.cmp(&a.added));
                let recent: Vec<String> = recent.iter().take(4).map(|i| i.text.clone()).collect();
                Some(Notice {
                    icon: "📊",
                    title: format!("지금까지 {}일 · {hours:.1}시간 공부했어요", p.days_studied.max(1)),
                    body: format!(
                        "단어 {}개 (기억 완료 {}) · 회화 문장 {}개 (기억 완료 {}).{}",
                        words.0,
                        words.1,
                        idioms.0,
                        idioms.1,
                        if recent.is_empty() { String::new() } else { format!("\n최근 배운 것: {}", recent.join(", ")) }
                    ),
                    go: Some((Screen::Progress, "📈 레벨 · 평가로 ▶")),
                })
            }
            _ => Some(self.next_level_notice()),
        }
    }

    /// Hours to the next level at the current pace, what 20 more minutes a day would save, and
    /// the one thing that would help most today.
    fn next_level_notice(&self) -> Notice {
        let p = &self.store.profile;
        let cur = band_index(p.level);
        let tip = if srs::due_count(&self.store.items) > 0 {
            format!("🔁 복습 {}개가 기다려요 — 복습을 미루면 같은 것을 다시 배우느라 더 오래 걸려요.", srs::due_count(&self.store.items))
        } else if !self.step_done("speed") {
            "⚡ 스피드 리딩 2분 대본을 매일 따라 읽으면 말하는 속도가 빨리 붙어요.".to_string()
        } else if !self.step_done("speaking") {
            "💬 자기 표현에서 오늘 단어를 넣어 말해 보세요 — 써 본 단어가 내 것이 돼요.".to_string()
        } else {
            "📶 레벨 테스트를 자주 해서 실력이 오른 것을 확인하세요.".to_string()
        };
        let Some(next) = LEVEL_BANDS.get(cur + 1) else {
            return Notice { icon: "🏆", title: "최고 레벨이에요!".into(), body: tip, go: Some((Screen::Progress, "📈 레벨 · 평가로 ▶")) };
        };
        let day = curriculum::day_number(p).max(1) as f32;
        let studied_h = p.study_seconds as f32 / 3600.0;
        let (lo, hi) = CUM_HOURS[cur];
        let need_h = (CUM_HOURS[cur + 1].0 as f32 - (lo + hi) as f32 / 2.0 - studied_h).max(5.0);
        // The learner's own pace so far (the daily goal until there is some history).
        let pace = (p.study_seconds as f32 / 60.0 / day).max(if p.days_studied < 3 { self.store.settings.daily_goal_minutes as f32 } else { 10.0 });
        let days_now = (need_h * 60.0 / pace).ceil();
        let days_fast = (need_h * 60.0 / (pace + 20.0)).ceil();
        Notice {
            icon: "🚀",
            title: format!("다음 레벨 {}까지 약 {need_h:.0}시간", next.2),
            body: format!("지금 속도(하루 {pace:.0}분)면 약 {days_now:.0}일. 하루 20분만 더 하면 약 {days_fast:.0}일 — {:.0}일 단축!\n{tip}", days_now - days_fast),
            go: Some((Screen::Progress, "📈 레벨 · 평가로 ▶")),
        }
    }

    /// Takes turns through the kinds, skipping those with nothing to say.
    fn next_notice(&mut self) -> Option<Notice> {
        for _ in 0..KINDS {
            let k = self.notifier.idx % KINDS;
            self.notifier.idx += 1;
            if let Some(n) = self.notice(k) {
                return Some(n);
            }
        }
        None
    }

    /// Shows the current notification (top right) and brings the next one when it is time.
    pub fn notifications_ui(&mut self, ctx: &egui::Context) {
        let minutes = self.store.settings.notify_minutes;
        let now = Instant::now();
        let quiet = minutes == 0 || self.store.settings.notify_off_date == Some(today()) || !self.store.profile.placement_done || matches!(self.screen, Screen::Onboarding | Screen::LevelTest) || self.voice.recorder.is_recording();
        if self.notifier.shown.is_none() && now >= self.notifier.next_at {
            if quiet {
                // Not while speaking or testing: try again shortly.
                self.notifier.next_at = now + Duration::from_secs(15);
            } else {
                self.notifier.shown = self.next_notice().map(|n| (n, now));
                self.notifier.next_at = now + Duration::from_secs(minutes.max(1) as u64 * 60);
                if let Some((n, _)) = &self.notifier.shown {
                    crate::applog!("notify: {}", n.title);
                }
            }
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        let Some((n, at)) = &mut self.notifier.shown else { return };
        let mut close = false;
        let mut off_today = false;
        let mut open: Option<Screen> = None;
        // Slides in from the right.
        let t = (at.elapsed().as_secs_f32() / 0.35).min(1.0);
        let slide = (1.0 - t) * (1.0 - t) * 60.0;
        if t < 1.0 {
            ctx.request_repaint();
        }
        let resp = egui::Area::new(egui::Id::new("notification")).order(egui::Order::Foreground).anchor(egui::Align2::RIGHT_TOP, Vec2::new(-20.0 + slide, 16.0)).show(ctx, |ui| {
            ui.set_max_width(380.0);
            egui::Frame::new()
                .fill(subtle_fill())
                .stroke(egui::Stroke::new(1.5, ACCENT))
                .corner_radius(12.0)
                .inner_margin(14.0)
                .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(80) })
                .show(ui, |ui| {
                    ui.set_width(352.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("🔔 {} {}", n.icon, n.title)).size(16.0).strong().color(text_strong()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("닫기").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(&n.body).size(14.5).color(text_strong()));
                    if let Some((screen, label)) = n.go {
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                            open = Some(screen);
                        }
                    }
                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        off_today = ui.add(egui::Button::new(RichText::new("오늘은 그만보기").size(13.0).color(muted())).frame(false)).on_hover_text("내일까지 알림을 띄우지 않아요").clicked();
                    });
                });
        });
        // Stays while the mouse is on it.
        if resp.response.contains_pointer() {
            *at = Instant::now() - Duration::from_secs(1);
        }
        if off_today {
            crate::applog!("notify: 오늘은 그만보기");
            self.store.settings.notify_off_date = Some(today());
            self.store.save_settings();
        }
        if close || off_today || open.is_some() || at.elapsed() > SHOW_FOR {
            self.notifier.shown = None;
        }
        if let Some(s) = open {
            self.go(s);
        }
    }
}
