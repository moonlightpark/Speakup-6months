//! Listening lab: real people saying what was learned today, inside the window —
//! 🎬 today's 문장 읽기 sentences and 🔤 today's words in YouTube videos (YouGlish) — then
//! said after them and scored; and 👂 listen and respond.

use crate::app::App;
use crate::models::*;
use crate::widgets::*;
use egui::{RichText, Ui};

pub struct ListeningState {
    /// 0 = today's sentences on YouGlish, 1 = listen and respond, 2 = today's words on YouGlish.
    pub tab: usize,
    /// 🎬 tab: the sentence shown.
    pub phrase_idx: usize,
    /// 🔤 tab: the word shown.
    pub word_idx: usize,
    /// Saying the sentence or word after the clips.
    pub cap: Capture,
    pub result: Option<(String, crate::scoring::ScoreResult)>,
}

impl Default for ListeningState {
    fn default() -> Self {
        Self { tab: 0, phrase_idx: 0, word_idx: 0, cap: Capture::default(), result: None }
    }
}

/// YouGlish search for a word or sentence: "peckish" →
/// https://youglish.com/pronounce/peckish/english (a sentence without its end mark).
pub fn youglish_url(text: &str) -> String {
    let q = text.trim().trim_end_matches(['.', '!', '?', ',']).trim();
    let mut enc = String::new();
    for c in q.chars() {
        match c {
            ' ' => enc.push_str("%20"),
            c if c.is_ascii_alphanumeric() || "'-_.~".contains(c) => enc.push(c),
            '’' => enc.push('\''),
            c => {
                let mut b = [0u8; 4];
                for byte in c.encode_utf8(&mut b).bytes() {
                    enc.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    format!("https://youglish.com/pronounce/{enc}/english")
}

impl App {
    /// 🎬 Today's 문장 읽기 sentences: pick one and YouGlish shows YouTube videos where people
    /// say it, inside the window; then say it yourself.
    fn sentence_clips_ui(&mut self, ui: &mut Ui) {
        // Today's longer sentences come with the lesson.
        self.ensure_lesson();
        let lesson = self.lesson.clone().filter(|l| l.date == Some(today())).unwrap_or_default();
        let sentences: Vec<(String, String)> = self.idiom_set().iter().map(|i| (i.en.clone(), i.ko.clone())).chain(lesson.sentences.iter().map(|s| (s.text.clone(), s.ko.clone()))).collect();
        if sentences.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("오늘 문장이 아직 없어요. 📖 문장 읽기를 먼저 열어 주세요.").color(muted()));
            });
            return;
        }
        let i = self.listening.phrase_idx.min(sentences.len() - 1);
        let (en, ko) = sentences[i].clone();
        let url = youglish_url(&en);
        let mut pick: Option<usize> = None;
        // The sentence, its controls and the mic.
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                speaker_button(ui, &mut self.voice, &en);
                ui.label(RichText::new(&en).size(self.store.settings.list_size().max(20.0)).strong().color(text_strong()));
                ui.label(RichText::new(format!("{}/{}", i + 1, sentences.len())).color(muted()));
            });
            crate::grammar::ko_label(ui, &ko, 16.0, muted());
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("◀ 이전 문장").min_size(ACTION_BUTTON)).clicked() {
                    pick = Some(i - 1);
                }
                if ui.add_enabled(i + 1 < sentences.len(), egui::Button::new("다음 문장 ▶").min_size(ACTION_BUTTON)).clicked() {
                    pick = Some(i + 1);
                }
                if ui.add(egui::Button::new("🌐 브라우저로 열기").min_size(ACTION_BUTTON)).on_hover_text(&url).clicked() {
                    let _ = open::that(&url);
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("📖 오늘 문장 {}개 — 누르면 그 문장의 영상으로 바뀌어요", sentences.len())).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (k, (s, _)) in sentences.iter().enumerate() {
                    let text = RichText::new(s).size(15.0).color(if k == i { egui::Color32::WHITE } else { text_strong() });
                    if ui.selectable_label(k == i, if k == i { text.strong() } else { text }).clicked() && k != i {
                        pick = Some(k);
                    }
                }
            });
        });
        ui.add_space(6.0);
        // The video as wide as the page, then the mic.
        self.youglish_frame(ui, &url);
        ui.add_space(6.0);
        self.say_after_ui(ui, &en, "youglish_sentence", "🎤 영상 속 원어민처럼 따라 말해 보세요");
        if let Some(k) = pick {
            self.listening.phrase_idx = k;
            self.listening.cap = Capture::default();
            self.listening.result = None;
        }
    }

    /// 🔤 Today's 새 단어 words: pick one and YouGlish shows YouTube videos where people say
    /// it, inside the window; then say it yourself.
    fn youglish_ui(&mut self, ui: &mut Ui) {
        let ws = self.word_set();
        let words: Vec<LessonWord> = ws.words.iter().chain(&ws.expressions).filter(|w| !w.word.contains(' ')).cloned().collect();
        if words.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("오늘 단어가 아직 없어요. 📚 새 단어를 먼저 열어 주세요.").color(muted()));
            });
            return;
        }
        let i = self.listening.word_idx.min(words.len() - 1);
        let w = words[i].clone();
        let url = youglish_url(&w.word);
        let mut pick: Option<usize> = None;
        let learned = |t: &str| self.store.items.iter().any(|it| it.text.eq_ignore_ascii_case(t));
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                speaker_button(ui, &mut self.voice, &w.word);
                ui.label(RichText::new(&w.word).size(self.store.settings.list_size().max(24.0)).strong().color(text_strong()));
                ui.label(RichText::new(&w.meaning_ko).size(17.0).color(muted()));
                ui.label(RichText::new(format!("{}/{}", i + 1, words.len())).color(muted()));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("◀ 이전 단어").min_size(ACTION_BUTTON)).clicked() {
                    pick = Some(i - 1);
                }
                if ui.add_enabled(i + 1 < words.len(), egui::Button::new("다음 단어 ▶").min_size(ACTION_BUTTON)).clicked() {
                    pick = Some(i + 1);
                }
                if ui.add(egui::Button::new("🌐 브라우저로 열기").min_size(ACTION_BUTTON)).on_hover_text(&url).clicked() {
                    let _ = open::that(&url);
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(format!("📚 오늘 단어 {}개 — 누르면 그 단어의 영상으로 바뀌어요 (✓ = 연습한 단어)", words.len())).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (k, x) in words.iter().enumerate() {
                    let label = if learned(&x.word) { format!("✓ {}", x.word) } else { x.word.clone() };
                    let text = RichText::new(label).size(15.0).color(if k == i { egui::Color32::WHITE } else { text_strong() });
                    if ui.selectable_label(k == i, if k == i { text.strong() } else { text }).on_hover_text(&x.meaning_ko).clicked() && k != i {
                        pick = Some(k);
                    }
                }
            });
        });
        ui.add_space(6.0);
        self.youglish_frame(ui, &url);
        ui.add_space(6.0);
        self.say_after_ui(ui, &w.word, "youglish", "🎤 영상 속 원어민 발음처럼 따라 말해 보세요");
        if let Some(k) = pick {
            self.listening.word_idx = k;
            self.listening.cap = Capture::default();
            self.listening.result = None;
        }
    }

    /// YouGlish, showing only the video (16:9, as wide as the page), its control bar and the
    /// caption box under it.
    fn youglish_frame(&mut self, ui: &mut Ui, url: &str) {
        let video_w = ui.available_width();
        self.web_frame(ui, url, video_w, video_w * 9.0 / 16.0 + 190.0, 1.0, "YouGlish");
    }

    /// The native web view laid over a `w`×`h` box (only the part on screen).
    fn web_frame(&mut self, ui: &mut Ui, url: &str, w: f32, h: f32, zoom: f32, site: &str) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        ui.painter().rect_filled(rect, 12.0, subtle_fill());
        let msg = match &self.web.failed {
            Some(e) => format!("이 컴퓨터에서는 앱 안에 웹 화면을 띄울 수 없어요 ({e}).\n🌐 브라우저로 열기를 눌러 주세요."),
            None => format!("🎬 {site} 영상을 불러오는 중…"),
        };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(16.0), muted());
        let shown = rect.intersect(ui.clip_rect());
        if shown.height() > 60.0 && shown.width() > 60.0 {
            self.web_req = Some(crate::web::WebRequest { url: url.to_string(), rect: shown, zoom });
        }
    }

    /// Say the sentence or word after the clips: scored, and a pass counts toward today's
    /// listening goal (5).
    fn say_after_ui(&mut self, ui: &mut Ui, target: &str, kind: &str, title: &str) {
        card(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.vertical_centered(|ui| {
                if let Some(t) = self.listening.cap.ui(ui, &mut self.voice, 15.0) {
                    let r = crate::scoring::score(target, &t);
                    self.store.log_attempt(kind, target, &t, r.score);
                    if r.score >= self.store.settings.pass_score {
                        self.store.profile.today.listening_passes += 1;
                        self.store.save_profile();
                        if self.store.profile.today.listening_passes >= 5 && !self.step_done("listening") {
                            self.complete_step("listening");
                        }
                    }
                    self.listening.result = Some((t, r));
                }
            });
            if let Some((t, r)) = &self.listening.result {
                let ok = r.score >= self.store.settings.pass_score;
                ui.label(RichText::new(format!("{} {:.0}점 — \"{t}\"", if ok { "✅" } else { "🔁" }, r.score)).color(if ok { GOOD } else { warn() }));
            }
            ui.label(RichText::new(format!("오늘 듣고 따라 말하기 통과: {}/5", self.store.profile.today.listening_passes.min(5))).color(muted()));
        });
    }

    pub fn listening_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "🎧 리스닝 랩", "오늘 배운 문장과 단어를 실제 원어민이 말하는 영상으로 보고 듣고, 따라 말하세요.");
        ui.horizontal_wrapped(|ui| {
            let before = self.listening.tab;
            ui.selectable_value(&mut self.listening.tab, 0, "🎬 오늘 문장 영상");
            ui.selectable_value(&mut self.listening.tab, 2, "🔤 오늘 단어 발음");
            ui.selectable_value(&mut self.listening.tab, 1, "👂 듣고 대답하기");
            if self.listening.tab != before {
                self.listening.cap = Capture::default();
                self.listening.result = None;
            }
        });
        ui.add_space(6.0);
        match self.listening.tab {
            1 => self.listen_respond_ui(ui),
            2 => self.youglish_ui(ui),
            _ => self.sentence_clips_ui(ui),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn youglish_search_urls() {
        assert_eq!(super::youglish_url("peckish"), "https://youglish.com/pronounce/peckish/english");
        assert_eq!(super::youglish_url(" café "), "https://youglish.com/pronounce/caf%C3%A9/english");
        assert_eq!(super::youglish_url("I'm a little peckish."), "https://youglish.com/pronounce/I'm%20a%20little%20peckish/english");
        assert_eq!(super::youglish_url("Rock & roll, baby!"), "https://youglish.com/pronounce/Rock%20%26%20roll%2C%20baby/english");
        assert_eq!(super::youglish_url("I’m in"), "https://youglish.com/pronounce/I'm%20in/english");
    }
}
