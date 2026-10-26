//! Reusable UI pieces: background jobs, the big microphone button, and the speaking drill
//! (listen → say it out loud → word-by-word feedback → repeat N times).

use crate::claude::{Claude, CoachTip};
use crate::models::Chunk;
use crate::scoring::{self, ScoreResult, WordMark};
use crate::voice::Voice;
use anyhow::Result;
use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, Ui, Vec2};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub const ACCENT: Color32 = Color32::from_rgb(61, 123, 255);
pub const GOOD: Color32 = Color32::from_rgb(52, 199, 120);
pub const BAD: Color32 = Color32::from_rgb(255, 92, 92);

// ---------------------------------------------------------------- theme colours
// Colours that differ between the dark and the light theme (설정 > 테마). The accent colours
// above read well on both.

static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Switches every themed colour below (call `theme::apply` too for egui's own widgets).
pub fn set_light(on: bool) {
    LIGHT.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// `dark` in the dark theme, `light` in the light theme.
pub fn themed(dark: Color32, light: Color32) -> Color32 {
    if is_light() { light } else { dark }
}

pub fn warn() -> Color32 {
    themed(Color32::from_rgb(255, 184, 0), Color32::from_rgb(190, 115, 0))
}
/// Secondary text.
pub fn muted() -> Color32 {
    themed(Color32::from_rgb(168, 178, 198), Color32::from_rgb(92, 100, 120))
}
pub fn card_bg() -> Color32 {
    themed(Color32::from_rgb(22, 28, 44), Color32::WHITE)
}
pub fn border() -> Color32 {
    themed(Color32::from_rgb(40, 50, 76), Color32::from_rgb(220, 225, 236))
}
pub fn bg() -> Color32 {
    themed(Color32::from_rgb(12, 16, 27), Color32::from_rgb(243, 245, 250))
}
pub fn sidebar() -> Color32 {
    themed(Color32::from_rgb(9, 12, 21), Color32::from_rgb(230, 234, 244))
}
/// Main text where it must stand out (titles, the sentence to say).
pub fn text_strong() -> Color32 {
    themed(Color32::WHITE, Color32::from_rgb(18, 22, 34))
}
/// Rows and tiles inside a card.
pub fn subtle_fill() -> Color32 {
    themed(Color32::from_rgb(27, 34, 52), Color32::from_rgb(236, 240, 248))
}

/// Korean-support stage for this window (`curriculum::support_stage`), set by the app each frame.
pub fn support_stage(ctx: &egui::Context) -> u8 {
    ctx.data(|d| d.get_temp::<u8>(egui::Id::new("support_stage"))).unwrap_or(0)
}

pub fn set_support_stage(ctx: &egui::Context, stage: u8) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("support_stage"), stage));
}

/// Furigana above kanji (설정 > 후리가나), set by the app each frame.
pub fn furigana_on(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("furigana"))).unwrap_or(true)
}

pub fn set_furigana(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("furigana"), on));
}

// ---------------------------------------------------------------- Japanese text with furigana

/// Height of the furigana line above text of `size`.
fn ruby_size(size: f32) -> f32 {
    (size * 0.48).max(9.0)
}

/// One word: its reading (small, above) and its text, centred on each other. With `show`
/// false (or no reading) the space above stays, so a line keeps one baseline.
fn ruby_piece(ui: &mut Ui, text: &str, ruby: Option<&str>, size: f32, color: Color32, show: bool, sense: Sense) -> egui::Response {
    let font = FontId::proportional(size);
    let small = FontId::proportional(ruby_size(size));
    let g = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let rg = ruby.filter(|_| show).map(|r| ui.painter().layout_no_wrap(r.to_string(), small.clone(), muted()));
    let top = if show { ui.fonts(|f| f.row_height(&small)) } else { 0.0 };
    let w = g.size().x.max(rg.as_ref().map_or(0.0, |r| r.size().x));
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, top + g.size().y), sense);
    if ui.is_rect_visible(rect) {
        if let Some(r) = rg {
            let x = rect.center().x - r.size().x / 2.0;
            ui.painter().galley(egui::pos2(x, rect.top()), r, muted());
        }
        let x = rect.center().x - g.size().x / 2.0;
        ui.painter().galley(egui::pos2(x, rect.top() + top), g, color);
    }
    resp
}

/// Adds the words of `text` (with furigana when switched on) to the current wrapping row.
pub fn ja_pieces(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    let show = furigana_on(ui.ctx()) && crate::ja::has_kanji(text);
    for p in crate::ja::furigana_cached(text).iter() {
        ruby_piece(ui, &p.text, p.ruby.as_deref(), size, color, show, Sense::hover());
    }
}

/// Japanese text, wrapped, with furigana above the kanji (설정 > 후리가나).
pub fn ja_text(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, size * 0.3);
        ja_pieces(ui, text, size, color);
    });
}

/// A list entry (a word with its dictionary `reading`, or a sentence) in the current wrapping
/// row, furigana above its kanji, clickable as a whole. A leading marker ("🔗 ", "💬 ") stays plain.
pub fn ja_entry(ui: &mut Ui, text: &str, reading: &str, size: f32, color: Color32) -> egui::Response {
    let (mark, body) = match text.split_once(' ') {
        Some((m, b)) if !m.chars().any(crate::ja::is_japanese_char) => (Some(m), b),
        _ => (None, text),
    };
    let show = furigana_on(ui.ctx()) && crate::ja::has_kanji(body);
    let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, 0.0);
    let mut resp = mark.map(|m| ruby_piece(ui, &format!("{m} "), None, size, color, show, Sense::click()));
    for p in crate::ja::furigana_read_cached(body, reading).iter() {
        let r = ruby_piece(ui, &p.text, p.ruby.as_deref(), size, color, show, Sense::click());
        resp = Some(match resp {
            Some(a) => a | r,
            None => r,
        });
    }
    ui.spacing_mut().item_spacing.x = gap;
    let resp = resp.unwrap_or_else(|| ui.allocate_response(Vec2::ZERO, Sense::hover()));
    // Painted, so it's named for screen readers (and tests) by its text.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    resp
}

/// A sentence (furigana kept) that can be clicked as a whole, e.g. in the reader: returns
/// (clicked, hovered). `highlight` paints behind it.
pub fn ja_sentence(ui: &mut Ui, text: &str, size: f32, color: Color32, highlight: Option<Color32>) -> (bool, bool) {
    let show = furigana_on(ui.ctx()) && crate::ja::has_kanji(text);
    let (mut clicked, mut hovered) = (false, false);
    for p in crate::ja::furigana_cached(text).iter() {
        let where_ = ui.painter().add(egui::Shape::Noop);
        let r = ruby_piece(ui, &p.text, p.ruby.as_deref(), size, color, show, Sense::click());
        if let Some(bg) = highlight {
            ui.painter().set(where_, egui::Shape::rect_filled(r.rect.expand(1.0), 2.0, bg));
        }
        clicked |= r.clicked();
        hovered |= r.hovered();
    }
    (clicked, hovered)
}

// ---------------------------------------------------------------- background jobs

pub struct Pending<T>(Option<Receiver<Result<T>>>);

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Send + 'static> Pending<T> {
    pub fn spawn(&mut self, f: impl FnOnce() -> Result<T> + Send + 'static) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        self.0 = Some(rx);
    }

    pub fn from_receiver(&mut self, rx: Receiver<Result<T>>) {
        self.0 = Some(rx);
    }

    pub fn running(&self) -> bool {
        self.0.is_some()
    }

    pub fn poll(&mut self) -> Option<Result<T>> {
        let rx = self.0.as_ref()?;
        match rx.try_recv() {
            Ok(r) => {
                self.0 = None;
                Some(r)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.0 = None;
                Some(Err(anyhow::anyhow!("작업이 중단되었습니다")))
            }
        }
    }
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(card_bg())
        .corner_radius(16.0)
        .inner_margin(20.0)
        .stroke(Stroke::new(1.0_f32, border()))
        .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Big page title with a one-line description underneath.
pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.add_space(4.0);
    let (icon, text) = crate::art::split_icon(title);
    ui.horizontal_wrapped(|ui| {
        if let Some(icon) = icon {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::hover());
            let (a, b) = crate::art::badge_colors(text);
            crate::art::icon_badge(ui.painter(), rect, icon, a, b);
            ui.add_space(4.0);
        }
        ui.vertical(|ui| {
            // Wrap within the page (a phone), not past it.
            ui.set_max_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).size(28.0).strong().color(text_strong())).wrap());
            if !subtitle.is_empty() {
                ui.add(egui::Label::new(RichText::new(subtitle).size(15.0).color(muted())).wrap());
            }
        });
    });
    ui.add_space(12.0);
}

/// Small rounded status pill.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    // In a wrapping row a frame doesn't wrap by itself: next line when it won't fit (phones),
    // and a chip longer than the whole line wraps its text.
    if ui.layout().main_wrap {
        let w = ui.fonts(|f| f.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), color).size().x) + 22.0;
        if ui.available_size_before_wrap().x < w && ui.cursor().left() > ui.max_rect().left() + 1.0 {
            ui.end_row();
        }
    }
    let max = ui.available_size_before_wrap().x.max(60.0);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(20.0)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.set_max_width(max - 22.0);
            ui.add(egui::Label::new(RichText::new(text).size(13.0).color(color)).wrap());
        });
}

/// The two button sizes used on every screen: big call-to-action buttons (시작하기, 대화 시작 …)
/// and action buttons (다음, 닫기, 그만하기, ◀ 목록 …; plain `ui.button`s come out this tall
/// from the theme's padding).
pub const BIG_BUTTON: Vec2 = Vec2::new(200.0, 46.0);
pub const ACTION_BUTTON: Vec2 = Vec2::new(120.0, 38.0);

/// An eye-catching button that invites a click: a gradient that breathes with a soft glow and
/// a band of light sweeping across now and then. Action-button height; redraws at ~20 fps
/// while it is on screen.
pub fn shiny_button(ui: &mut Ui, text: &str, from: Color32, to: Color32) -> egui::Response {
    let font = egui::FontId::proportional(16.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let size = Vec2::new((galley.size().x + 36.0).max(ACTION_BUTTON.x), ACTION_BUTTON.y);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    let t = ui.input(|i| i.time) as f32;
    let breathe = 0.5 + 0.5 * (t * 2.4).sin();
    let p = ui.painter();
    // Soft glow around it, swelling and fading.
    let glow = if resp.hovered() { 1.0 } else { breathe };
    p.rect_filled(rect.expand(2.0 + 3.0 * glow), 14.0, from.gamma_multiply(0.18 + 0.22 * glow));
    let r = if resp.hovered() { rect.expand(1.0) } else { rect };
    crate::art::gradient_rect(p, r, 11.0, from, to);
    // A light band sweeping left to right every 2.6 s.
    let phase = (t % 2.6) / 1.1;
    if phase < 1.0 {
        let x = r.left() - 30.0 + (r.width() + 60.0) * phase;
        let band = egui::Rect::from_min_max(egui::pos2((x - 14.0).max(r.left()), r.top()), egui::pos2((x + 14.0).min(r.right()), r.bottom()));
        if band.width() > 0.0 {
            p.rect_filled(band, 0.0, Color32::from_white_alpha(46));
        }
    }
    p.galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    resp
}

/// Main call-to-action button.
pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(17.0).strong().color(Color32::WHITE)).fill(ACCENT).corner_radius(12.0).min_size(BIG_BUTTON))
}

pub fn spinner_label(ui: &mut Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spinner();
        ui.label(RichText::new(text).color(muted()));
    });
}

pub fn error_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(format!("⚠ {text}")).color(BAD));
}

// ---------------------------------------------------------------- microphone capture

#[derive(Default)]
pub struct Capture {
    rx: Pending<String>,
    pub error: Option<String>,
    recording: bool,
    /// Voiced duration of the last recording, for speaking-speed (자/분) measurement.
    pub last_secs: f32,
    /// Stop automatically after ~0.9 s of silence following speech (speed drills).
    pub auto_stop: bool,
    heard_voice: bool,
    silent_since: Option<std::time::Instant>,
    /// When the last recording was stopped (for response-time measurement).
    pub stopped_at: Option<std::time::Instant>,
    /// Recognize Korean instead of the target language (translator).
    pub korean: bool,
    /// When the microphone really started listening, and when sound last came in.
    listening_since: Option<std::time::Instant>,
    last_sound: Option<std::time::Instant>,
    heard_sound: bool,
}

/// The mic stays grey ("마이크 준비 중…") this long after listening starts, then turns red with "듣고 있어요…".
pub const LISTEN_LABEL_DELAY: f32 = 1.5;
/// Listening ends after this long without any sound: cancelled if nothing was heard at all.
pub const SILENCE_CANCEL_SECS: f32 = 8.0;
const SOUND_LEVEL: f32 = 0.05;

/// What to do after `quiet` seconds without sound: None = keep listening, Some(true) = stop and
/// recognize what was said, Some(false) = cancel (nothing was said).
pub fn silence_action(quiet: f32, heard_sound: bool) -> Option<bool> {
    (quiet >= SILENCE_CANCEL_SECS).then_some(heard_sound)
}

impl Capture {
    /// A capture that recognizes Korean (translator).
    pub fn korean() -> Self {
        Self { korean: true, ..Default::default() }
    }

    /// A capture that stops by itself when the speaker goes quiet.
    pub fn auto_stopping() -> Self {
        Self { auto_stop: true, ..Default::default() }
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Starts recording immediately (used to auto-start the mic in speed drills).
    pub fn start_now(&mut self, voice: &mut Voice) {
        if !self.recording && !self.rx.running() {
            self.start(voice);
        }
    }

    fn start(&mut self, voice: &mut Voice) {
        self.error = None;
        self.heard_voice = false;
        self.silent_since = None;
        self.listening_since = None;
        self.last_sound = None;
        self.heard_sound = false;
        voice.speaker.stop();
        voice.player.stop();
        if voice.recorder.is_recording() {
            // Another capture is still recording; stop it first.
            voice.recorder.stop();
        }
        voice.recorder.start();
        // The recognizer loads while the learner speaks, so the result is ready when they stop.
        if self.korean { voice.stt_ko.warm() } else { voice.stt.warm() }
        self.recording = true;
    }

    fn finish(&mut self, voice: &mut Voice) {
        self.recording = false;
        self.stopped_at = Some(std::time::Instant::now());
        let problem = voice.recorder.problem();
        let audio = voice.recorder.stop();
        if let Some(p) = problem {
            // Nothing real was recorded; don't let the recognizer guess at silence.
            self.error = Some(p.message(&voice.recorder.active_device()));
            return;
        }
        self.last_secs = crate::audio::voiced_seconds(&audio, crate::audio::STT_RATE);
        voice.last_recording = audio.clone();
        let stt = if self.korean { &voice.stt_ko } else { &voice.stt };
        self.rx.from_receiver(stt.transcribe(audio));
    }

    /// Stops listening without recognizing anything (nothing was said).
    fn cancel_silent(&mut self, voice: &mut Voice) {
        self.recording = false;
        voice.recorder.stop();
        crate::applog::write("mic: 소리 없음 — 녹음 자동 취소");
        self.error = Some(format!("🔇 {SILENCE_CANCEL_SECS:.0}초 동안 소리가 들리지 않아 녹음을 취소했어요. 다시 눌러서 말해 보세요."));
    }

    /// Big round mic button. Returns the transcript once recognition finishes.
    pub fn ui(&mut self, ui: &mut Ui, voice: &mut Voice, max_secs: f32) -> Option<String> {
        // The mic failed to open, or someone else took the recorder.
        if self.recording {
            match voice.recorder.state() {
                crate::audio::RecState::Failed(e) => {
                    self.recording = false;
                    self.error = Some(e);
                    voice.recorder.reset();
                }
                crate::audio::RecState::Idle => self.recording = false,
                _ => {}
            }
        }
        let mut result = None;
        if let Some(r) = self.rx.poll() {
            match r {
                Ok(t) if !t.trim().is_empty() => result = Some(t),
                Ok(_) => self.error = Some("말한 내용을 알아듣지 못했어요. 다시 해볼까요?".into()),
                Err(e) => self.error = Some(format!("{e:#}")),
            }
        }
        let size = 84.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 16.0), Sense::click());
        let a11y = if self.recording { "⏹ 녹음 끝내기" } else { "🎤 말하기" };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, a11y));
        let c = rect.center();
        let p = ui.painter();
        let transcribing = self.rx.running();
        let starting = self.recording && voice.recorder.state() == crate::audio::RecState::Starting;
        // Grey (and "준비 중") until the mic has been listening LISTEN_LABEL_DELAY, then red.
        let warming = starting || self.recording && self.listening_since.is_none_or(|t| t.elapsed().as_secs_f32() < LISTEN_LABEL_DELAY);
        if warming {
            // Grey until the microphone is really listening, then red.
            p.circle_filled(c, size / 2.0, MIC_WAITING);
            draw_mic_icon(p, c);
            ui.ctx().request_repaint();
        } else if self.recording {
            let lvl = voice.recorder.level().min(1.0);
            p.circle_filled(c, size / 2.0 + 4.0 + lvl * 14.0, BAD.gamma_multiply(0.25));
            p.circle_filled(c, size / 2.0, BAD);
            p.rect_filled(egui::Rect::from_center_size(c, Vec2::splat(24.0)), 4.0, Color32::WHITE);
            ui.ctx().request_repaint();
        } else {
            let fill = if transcribing { muted() } else if resp.hovered() { ACCENT.gamma_multiply(1.2) } else { ACCENT };
            p.circle_filled(c, size / 2.0, fill);
            draw_mic_icon(p, c);
        }
        if resp.clicked() && !transcribing {
            if self.recording {
                self.finish(voice);
            } else {
                self.start(voice);
            }
        }
        if self.recording && voice.recorder.elapsed() > max_secs {
            self.finish(voice);
        }
        // A blocked microphone records pure zeros: stop right away and say why.
        if self.recording && voice.recorder.problem().is_some() && voice.recorder.elapsed() > 1.5 {
            self.finish(voice);
        }
        // Nothing heard for a while: cancel (or, after speech, stop and recognize it).
        if self.recording && !starting && voice.recorder.state() == crate::audio::RecState::Recording {
            let now = std::time::Instant::now();
            self.listening_since.get_or_insert(now);
            if voice.recorder.level() > SOUND_LEVEL {
                self.heard_sound = true;
                self.last_sound = Some(now);
            }
            let quiet = now.duration_since(self.last_sound.or(self.listening_since).unwrap_or(now)).as_secs_f32();
            match silence_action(quiet, self.heard_sound) {
                Some(true) => self.finish(voice),
                Some(false) => self.cancel_silent(voice),
                None => {}
            }
        }
        if self.recording && self.auto_stop {
            let lvl = voice.recorder.level();
            if lvl > 0.08 {
                self.heard_voice = true;
                self.silent_since = None;
            } else if self.heard_voice && lvl < 0.03 {
                let since = *self.silent_since.get_or_insert_with(std::time::Instant::now);
                if since.elapsed().as_secs_f32() > 0.9 {
                    self.finish(voice);
                }
            }
        }
        let status = if warming {
            "마이크 준비 중…".to_string()
        } else if self.recording {
            format!("듣고 있어요… {:.0}초 (다 말하면 다시 누르세요)", voice.recorder.elapsed())
        } else if transcribing {
            ui.ctx().request_repaint();
            "인식 중…".to_string()
        } else {
            "눌러서 말하기".to_string()
        };
        ui.label(RichText::new(status).color(muted()));
        if let Some(e) = &self.error {
            error_label(ui, e);
        }
        result
    }
}

/// The mic button while the microphone is still opening.
pub const MIC_WAITING: Color32 = Color32::from_gray(150);

fn draw_mic_icon(p: &egui::Painter, c: egui::Pos2) {
    let w = Color32::WHITE;
    p.rect_filled(egui::Rect::from_center_size(c - Vec2::new(0.0, 6.0), Vec2::new(16.0, 26.0)), 8.0, w);
    let s = Stroke::new(3.0_f32, w);
    let pts: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 12.0);
            c + Vec2::new(-14.0 * a.cos(), 2.0 + 12.0 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, s));
    p.line_segment([c + Vec2::new(0.0, 14.0), c + Vec2::new(0.0, 22.0)], s);
}

// ---------------------------------------------------------------- the speaking drill

pub fn speaker_button(ui: &mut Ui, voice: &mut Voice, text: &str) {
    if ui.button("🔊").on_hover_text("듣기").clicked() {
        voice.say(text);
    }
}

/// The target with each word green (heard) or red (missed), furigana kept.
pub fn word_feedback(ui: &mut Ui, r: &ScoreResult, size: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, size * 0.3);
        for m in &r.marks {
            ja_pieces(ui, &m.word, size, if m.ok { GOOD } else { BAD });
        }
    });
}

pub struct Attempt {
    pub score: f32,
    pub transcript: String,
    /// Target words the recognizer didn't hear (likely mispronounced). Empty for recall tries.
    pub missed: Vec<String>,
}

/// What the speaking score measures (Study.md: say so plainly).
pub const SCORE_MEANING: &str = "점수는 음성 인식이 내 말을 얼마나 잘 알아들었는지(알아듣기 쉬운 정도)예요. 원어민처럼 들리는지는 재지 않아요.";

pub fn gloss() -> Color32 {
    themed(Color32::from_rgb(255, 214, 120), Color32::from_rgb(165, 105, 0))
}

/// Space between sentences of a passage, so each one stands apart (scaled by font size).
fn sentence_gap(ui: &mut Ui, i: usize, size: f32) {
    if i > 0 {
        ui.add_space(size * 0.6);
    }
}

/// True for a word or phrase that ends a sentence ("行きました。", "ですか？」").
fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', '”', '’', ')', '」', '』', '）']).ends_with(['.', '!', '?', '。', '！', '？'])
}

/// Word-by-word feedback with each sentence on its own line.
pub fn word_feedback_lines(ui: &mut Ui, r: &ScoreResult, size: f32) {
    let (mut start, mut n) = (0, 0);
    for (i, m) in r.marks.iter().enumerate() {
        if ends_sentence(&m.word) || i + 1 == r.marks.len() {
            sentence_gap(ui, n, size);
            n += 1;
            word_feedback(ui, &ScoreResult { score: r.score, marks: r.marks[start..=i].to_vec() }, size);
            start = i + 1;
        }
    }
}

/// Colour key above a 직독직해 view whose chunks carry grammatical roles.
pub fn role_legend(ui: &mut Ui, chunks: &[Chunk]) {
    if chunks.iter().any(|c| !c.role.is_empty()) {
        crate::grammar::legend(ui);
    }
}

/// 직독직해 with each sentence on its own line; `marks` (one per word) color the chunks.
pub fn chunk_view_lines(ui: &mut Ui, chunks: &[Chunk], marks: Option<&[WordMark]>, size: f32) {
    role_legend(ui, chunks);
    let (mut start, mut n) = (0, 0);
    // Marks are words of the whole text: each sentence gets the marks inside its characters.
    let mut offset = 0;
    for (i, c) in chunks.iter().enumerate() {
        if ends_sentence(&c.ja) || i + 1 == chunks.len() {
            sentence_gap(ui, n, size);
            n += 1;
            let len: usize = chunks[start..=i].iter().map(|c| compact_len(&c.ja)).sum();
            let m = marks.map(|m| marks_in(m, offset, offset + len));
            chunk_view(ui, &chunks[start..=i], m.as_deref(), size);
            offset += len;
            start = i + 1;
        }
    }
}

/// Characters of a text without whitespace (chunks and marks both spell the text this way).
fn compact_len(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// The marks whose characters fall in `from..to` of the text, re-based to start at `from`.
fn marks_in(marks: &[WordMark], from: usize, to: usize) -> Vec<WordMark> {
    let mut out = Vec::new();
    let mut pos = 0;
    for m in marks {
        let len = compact_len(&m.word);
        if pos + len > from && pos < to {
            out.push(m.clone());
        }
        pos += len;
    }
    out
}

/// Is the text range `from..to` all heard? (Marks cover the text from position 0.)
fn range_ok(marks: &[WordMark], from: usize, to: usize) -> bool {
    let mut pos = 0;
    let mut ok = true;
    for m in marks {
        let len = compact_len(&m.word);
        if pos + len > from && pos < to && !m.ok {
            ok = false;
        }
        pos += len;
    }
    ok
}

/// Plain text with each sentence on its own line: Japanese with furigana, Korean coloured by
/// grammatical role.
pub fn sentence_lines(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    for (i, s) in crate::voice::sentences(text).iter().enumerate() {
        sentence_gap(ui, i, size);
        if crate::ja::is_japanese(s) {
            ja_text(ui, s, size, color);
        } else {
            crate::grammar::ko_label(ui, s, size, color);
        }
    }
}

/// 직독직해 view: Japanese 문절 separated by " / " with the Korean meaning under each.
/// With `marks` (words of these chunks' text, in order), each chunk is green/red by whether
/// its words were recognized; before that it shows its grammatical role's colour.
pub fn chunk_view(ui: &mut Ui, chunks: &[Chunk], marks: Option<&[WordMark]>, size: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 10.0);
        let mut pos = 0;
        for (i, c) in chunks.iter().enumerate() {
            // A chunk (Japanese over Korean) is a group, which doesn't wrap by itself: next line
            // when it and its " / " won't fit (phones).
            let w = ui.fonts(|f| {
                let ja = f.layout_no_wrap(c.ja.clone(), FontId::proportional(size), Color32::WHITE).size().x;
                let ko = f.layout_no_wrap(c.ko.clone(), FontId::proportional(size * 0.62), Color32::WHITE).size().x;
                ja.max(ko)
            });
            wrap_before(ui, w + if i > 0 { size * 0.6 } else { 0.0 });
            if i > 0 {
                ui.vertical(|ui| {
                    if furigana_on(ui.ctx()) {
                        ui.add_space(ui.fonts(|f| f.row_height(&FontId::proportional(ruby_size(size)))));
                    }
                    ui.label(RichText::new("/").size(size).color(muted()));
                    ui.label(RichText::new("/").size(size * 0.62).color(muted()));
                });
            }
            let len = compact_len(&c.ja);
            let role = crate::grammar::color(&c.role);
            let color = match marks {
                Some(m) if !m.is_empty() => {
                    if range_ok(m, pos, pos + len) { GOOD } else { BAD }
                }
                _ => role.unwrap_or(text_strong()),
            };
            pos += len;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // Furigana space on every chunk keeps the row on one baseline.
                    let show = furigana_on(ui.ctx());
                    for p in crate::ja::furigana_cached(&c.ja).iter() {
                        ruby_piece(ui, &p.text, p.ruby.as_deref(), size, color, show, Sense::hover());
                    }
                });
                ui.label(RichText::new(&c.ko).size(size * 0.62).strong().color(role.unwrap_or(gloss())));
            });
        }
    });
}

pub enum DrillEvent {
    None,
    /// A scored attempt.
    Attempt(Attempt),
    /// The required number of passing repetitions has been reached.
    Completed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_cancels_or_stops_after_eight_seconds() {
        assert_eq!(silence_action(7.9, false), None);
        assert_eq!(silence_action(8.0, false), Some(false), "nothing said: cancel");
        assert_eq!(silence_action(8.5, true), Some(true), "said something, then quiet: recognize it");
    }

    #[test]
    fn sentence_end_detection() {
        assert!(ends_sentence("行きました。") && ends_sentence("ですか？」") && ends_sentence("すごい！"));
        assert!(!ends_sentence("昨日、") && !ends_sentence("友達と"));
    }

    #[test]
    fn hints_reveal_letters_gradually() {
        let mut d = Drill::new("予定", "일정", "", 1, true, true);
        assert_eq!(d.hidden_text(), "＿＿＿");
        d.hint = 1;
        assert_eq!(d.hidden_text(), "よ＿＿");
        d.hint = 50;
        assert_eq!(d.hidden_text(), "よて＿", "the last kana is never given away");
        let mut s = Drill::new("はじめまして。", "처음 뵙겠습니다.", "", 1, true, false);
        s.hint = 2;
        assert_eq!(s.hidden_text(), "はじ＿＿＿＿。");
    }
}

#[derive(Default)]
pub struct Drill {
    pub target: String,
    pub ko: String,
    pub tip: String,
    pub reps_needed: u32,
    pub reps_passed: u32,
    /// Production practice: show the Korean, the learner says the Japanese from memory.
    pub recall: bool,
    /// Single word / short phrase (recall is checked by "did they say it").
    pub is_word: bool,
    pub revealed: bool,
    pub hint: usize,
    pub first_score: Option<f32>,
    pub last: Option<(String, ScoreResult)>,
    pub last_wpm: f32,
    /// 직독직해 for the target (empty = not available yet).
    pub chunks: Vec<Chunk>,
    pub show_gloss: bool,
    gloss_job: Pending<Vec<Chunk>>,
    /// A gloss fetched by this drill, for the app to cache.
    pub fresh_gloss: Option<(String, Vec<Chunk>)>,
    pub capture: Capture,
    coach: Pending<CoachTip>,
    pub coach_tip: Option<Result<CoachTip, String>>,
    autoplayed: bool,
    done: bool,
    /// The support stage has been applied to `show_gloss` (on first display).
    staged: bool,
    /// Months 5-6: the Korean meaning is shown only after an attempt or on request.
    ko_shown: bool,
}

impl Drill {
    pub fn new(target: &str, ko: &str, tip: &str, reps: u32, recall: bool, is_word: bool) -> Self {
        Self {
            target: target.to_string(),
            ko: ko.to_string(),
            tip: tip.to_string(),
            reps_needed: reps.max(1),
            recall: recall && !ko.is_empty(),
            is_word,
            revealed: !(recall && !ko.is_empty()),
            ..Default::default()
        }
    }

    pub fn with_chunks(mut self, chunks: Vec<Chunk>) -> Self {
        self.chunks = chunks;
        self.show_gloss = true;
        self
    }

    /// Don't speak the target with TTS when the drill appears (e.g. native audio is playing).
    pub fn without_autoplay(mut self) -> Self {
        self.autoplayed = true;
        self
    }

    /// The answer's reading with its kana hidden; each hint reveals one more kana from the start
    /// (never the last one, so hints can't give the whole answer away). Punctuation stays.
    fn hidden_text(&self) -> String {
        let reading = crate::ja::reading(&self.target);
        let letters = reading.chars().filter(|c| c.is_alphanumeric()).count();
        let keep = self.hint.min(letters.saturating_sub(1));
        let mut seen = 0;
        reading
            .chars()
            .map(|c| {
                if !c.is_alphanumeric() {
                    return c;
                }
                seen += 1;
                if seen <= keep { c } else { '＿' }
            })
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, voice: &mut Voice, pass: f32, claude: Option<&Claude>) -> DrillEvent {
        let mut event = DrillEvent::None;
        let stage = support_stage(ui.ctx());
        if !self.staged {
            // 직독직해 is on by default only in months 1-2; later it's one click away.
            self.staged = true;
            self.show_gloss &= stage == 0;
        }
        if !self.autoplayed && self.revealed {
            self.autoplayed = true;
            voice.say(&self.target);
        }
        if let Some(r) = self.coach.poll() {
            self.coach_tip = Some(r.map_err(|e| format!("{e:#}")));
        }
        if let Some(Ok(chunks)) = self.gloss_job.poll() {
            if crate::storage::chunks_match(&self.target, &chunks) {
                self.chunks = chunks.clone();
                self.show_gloss = true;
                self.fresh_gloss = Some((self.target.clone(), chunks));
            }
        }

        ui.vertical_centered(|ui| {
            if self.recall && !self.revealed {
                ui.label(RichText::new("일본어로 말해 보세요").color(muted()));
                crate::grammar::ko_label(ui, &self.ko, 26.0, text_strong());
                ui.add_space(6.0);
                ui.label(RichText::new(self.hidden_text()).size(22.0).color(muted()));
                if !self.tip.is_empty() && self.hint > 0 {
                    ui.label(RichText::new(&self.tip).color(muted()));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((ui.available_width() - 220.0).max(0.0) / 2.0);
                    if ui.button("💡 힌트").clicked() {
                        self.hint += 1;
                    }
                    if ui.button("정답 보기").clicked() {
                        self.revealed = true;
                        self.first_score.get_or_insert(0.0);
                        voice.say(&self.target);
                    }
                });
            } else {
                let marks = self.last.as_ref().map(|(_, r)| r.marks.as_slice());
                if self.show_gloss && !self.chunks.is_empty() {
                    role_legend(ui, &self.chunks);
                    chunk_view(ui, &self.chunks, marks, 26.0);
                } else {
                    match &self.last {
                        Some((_, r)) => word_feedback(ui, r, 26.0),
                        None => ja_text(ui, &self.target, 26.0, text_strong()),
                    }
                }
                if !self.ko.is_empty() {
                    if stage < 2 || self.ko_shown || self.last.is_some() {
                        crate::grammar::ko_label(ui, &self.ko, 16.0, muted());
                    } else if ui.small_button("한국어 뜻 보기").on_hover_text("5~6개월 차: 일본어만 보고, 한국어는 확인할 때만 봐요").clicked() {
                        self.ko_shown = true;
                    }
                }
                if !self.tip.is_empty() {
                    ui.label(RichText::new(format!("💡 {}", self.tip)).color(warn()));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                    if ui.button("🔊 듣기").clicked() {
                        voice.say(&self.target);
                    }
                    if ui.button("🐢 천천히").clicked() {
                        voice.say_slow(&self.target);
                    }
                    if ui.button("🐇 빠르게").clicked() {
                        voice.say_fast(&self.target);
                    }
                    if !voice.last_recording.is_empty() && ui.button("🎧 내 목소리").clicked() {
                        voice.play_last_recording();
                    }
                    if !self.is_word && crate::ja::char_count(&self.target) > 4 {
                        if !self.chunks.is_empty() {
                            ui.toggle_value(&mut self.show_gloss, "📖 직독직해");
                        } else if self.gloss_job.running() {
                            ui.spinner();
                        } else if ui.button("📖 직독직해").clicked() {
                            match claude {
                                Some(c) => {
                                    let (c, t) = (c.clone(), self.target.clone());
                                    self.gloss_job.spawn(move || c.chunk_gloss(&t));
                                }
                                // Offline: 문절 and roles from the analyser, meanings from the dictionary.
                                None => {
                                    self.chunks = crate::ja::local_chunks(&self.target);
                                    self.show_gloss = true;
                                }
                            }
                        }
                    }
                });
            }
            ui.add_space(8.0);

            if let Some(transcript) = self.capture.ui(ui, voice, 30.0) {
                let recall_try = self.recall && !self.revealed;
                let result = if recall_try && self.is_word {
                    let hit = scoring::contains_phrase(&transcript, &self.target);
                    let mut r = scoring::score(&self.target, &transcript);
                    if hit {
                        r.score = 100.0;
                        r.marks.iter_mut().for_each(|m| m.ok = true);
                    }
                    r
                } else {
                    scoring::score(&self.target, &transcript)
                };
                let s = result.score;
                self.first_score.get_or_insert(s);
                self.revealed = true;
                self.autoplayed = true;
                if s >= pass {
                    self.reps_passed += 1;
                }
                self.coach_tip = None;
                let missed = if recall_try { Vec::new() } else { result.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect() };
                let chars: usize = result.marks.iter().filter(|m| m.ok).map(|m| crate::ja::char_count(&m.word)).sum();
                self.last_wpm = crate::audio::wpm(chars, self.capture.last_secs);
                self.last = Some((transcript.clone(), result));
                event = DrillEvent::Attempt(Attempt { score: s, transcript, missed });
                if self.reps_passed >= self.reps_needed && !self.done {
                    self.done = true;
                }
            }

            if let Some((heard, r)) = &self.last {
                let color = if r.score >= pass { GOOD } else if r.score >= pass - 20.0 { warn() } else { BAD };
                ui.label(RichText::new(format!("{:.0}점", r.score)).size(30.0).strong().color(color))
                    .on_hover_text(SCORE_MEANING);
                ui.label(RichText::new(format!("들린 말: \"{heard}\"")).color(muted()));
                ui.label(RichText::new("점수 = 알아듣기 쉬운 정도 (원어민 발음과 비교한 점수가 아니에요)").small().color(muted())).on_hover_text(SCORE_MEANING);
                if self.last_wpm > 0.0 && crate::ja::char_count(&self.target) >= 8 {
                    ui.label(RichText::new(format!("⚡ 말하기 속도 분당 {:.0}자 (원어민 대화 약 350자)", self.last_wpm)).color(muted()).small());
                }
                if r.score < pass {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space((ui.available_width() - 260.0).max(0.0) / 2.0);
                        if let Some(c) = claude {
                            if self.coach.running() {
                                ui.spinner();
                            } else if ui.button("💡 AI 발음 코칭").clicked() {
                                let (c, t, h) = (c.clone(), self.target.clone(), heard.clone());
                                let missed: Vec<String> = r.marks.iter().filter(|m| !m.ok).map(|m| m.word.clone()).collect();
                                self.coach.spawn(move || c.pronunciation_coach(&t, &h, &missed));
                            }
                        }
                    });
                }
            }
            match &self.coach_tip {
                Some(Ok(t)) => {
                    ui.label(RichText::new(format!("💡 {}", t.tip_ko)).color(Color32::LIGHT_BLUE));
                    if !t.focus_words.is_empty() {
                        ui.label(RichText::new(format!("집중 연습: {}", t.focus_words.join(", "))).color(warn()));
                    }
                }
                Some(Err(e)) => error_label(ui, e),
                None => {}
            }

            ui.add_space(6.0);
            reps_dots(ui, self.reps_passed, self.reps_needed);
            if self.done {
                ui.add_space(6.0);
                if ui.add(egui::Button::new(RichText::new("다음 ▶").size(16.0)).fill(GOOD).min_size(ACTION_BUTTON)).clicked() {
                    event = DrillEvent::Completed;
                }
            } else if self.last.is_some() {
                ui.label(RichText::new(format!("{}점 이상으로 {}번 말하면 통과", pass as i32, self.reps_needed)).color(muted()));
            }
        });
        event
    }
}

pub fn reps_dots(ui: &mut Ui, passed: u32, needed: u32) {
    let r = 7.0;
    let w = needed as f32 * (r * 2.0 + 8.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, r * 2.0 + 4.0), Sense::hover());
        for i in 0..needed {
            let c = rect.left_center() + Vec2::new(r + 4.0 + i as f32 * (r * 2.0 + 8.0), 0.0);
            if i < passed {
                ui.painter().circle_filled(c, r, GOOD);
            } else {
                ui.painter().circle_stroke(c, r, Stroke::new(2.0_f32, muted()));
            }
        }
    });
}

/// Progress ring (daily goal), drawn like Speak's home screen.
pub fn ring(ui: &mut Ui, frac: f32, label: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let c = rect.center();
    let rad = size / 2.0 - 6.0;
    let p = ui.painter();
    p.circle_stroke(c, rad, Stroke::new(9.0_f32, themed(Color32::from_rgb(40, 48, 70), Color32::from_rgb(222, 227, 238))));
    let n = 64;
    let pts: Vec<egui::Pos2> = (0..=((n as f32 * frac.clamp(0.0, 1.0)) as usize))
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * (i as f32 / n as f32);
            c + Vec2::new(a.cos(), a.sin()) * rad
        })
        .collect();
    if pts.len() > 1 {
        p.add(egui::Shape::line(pts, Stroke::new(9.0_f32, ACCENT)));
    }
    p.text(c, Align2::CENTER_CENTER, label, FontId::proportional(size * 0.2), Color32::WHITE);
}

/// Debug aid for the phone layout: prints how wide the page has grown so far (only with
/// SPEAKUP_DBGW set, in tests).
#[cfg(test)]
#[allow(dead_code)]
pub fn dbgw(ui: &Ui, tag: &str) {
    if std::env::var_os("SPEAKUP_DBGW").is_some() {
        eprintln!("DBGW {tag} {:.0}", ui.min_rect().width());
    }
}

/// The rest of a row, right-aligned (a chip or button at the end of a line). In a wrapping row
/// with less than `room` left (a phone), it goes on the next line instead of past the edge.
pub fn at_right<R>(ui: &mut Ui, room: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    if ui.layout().main_wrap && ui.available_size_before_wrap().x < room {
        ui.end_row();
        // A phone: on its own line, left-aligned like the rest.
        if ui.max_rect().width() < 420.0 {
            return add(ui);
        }
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner
}

/// A table (or anything of a fixed width) that scrolls sideways when the screen is narrower
/// (phones), instead of pushing the page wider.
pub fn wide<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::ScrollArea::horizontal().id_salt(id).auto_shrink([true, true]).show(ui, add).inner
}

/// A slider with its name: beside it on a wide screen, above it on a phone (a slider's own text
/// doesn't wrap).
pub fn named_slider(ui: &mut Ui, slider: egui::Slider<'_>, name: &str) -> egui::Response {
    if ui.available_width() < 420.0 {
        ui.label(name);
        ui.add(slider)
    } else {
        ui.add(slider.text(name))
    }
}

/// In a wrapping row, starts the next line when something `width` wide won't fit in what's
/// left of this one (groups and frames don't wrap by themselves).
pub fn wrap_before(ui: &mut Ui, width: f32) {
    if ui.layout().main_wrap && ui.available_size_before_wrap().x < width && ui.cursor().left() > ui.max_rect().left() + 1.0 {
        ui.end_row();
    }
}
