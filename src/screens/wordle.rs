//! 🔠 낱말 맞추기 (복습 menu): a round of several words already learned, each guessed in six
//! tries, Wordle style — green = right letter in the right place, yellow = in the word
//! elsewhere, grey = not in it. The Korean meanings are listed on the left (pick one to solve);
//! typed on the keyboard or with the on-screen keys; a finished word is read aloud with its
//! meaning and example.

use crate::app::App;
use crate::widgets::*;
use egui::{Color32, RichText, Ui, Vec2};
use rand::seq::IndexedRandom;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub const TRIES: usize = 6;
const MIN_LEN: usize = 4;
const MAX_LEN: usize = 12;
/// Words in a round.
pub const ROUND_WORDS: usize = 5;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mark {
    Hit,
    Near,
    Miss,
}

#[derive(Clone)]
pub struct Answer {
    pub word: String,
    pub ko: String,
    pub example: String,
}

/// One word of a round and the guesses made at it.
#[derive(Clone)]
pub struct Puzzle {
    pub answer: Answer,
    pub guesses: Vec<String>,
}

impl Puzzle {
    pub fn solved(&self) -> bool {
        self.guesses.last().is_some_and(|g| g == &self.answer.word)
    }
    pub fn over(&self) -> bool {
        self.solved() || self.guesses.len() >= TRIES
    }
}

#[derive(Default)]
pub struct WordleState {
    /// 복습 menu tab: 0 = review cards, 1 = this game.
    pub tab: usize,
    /// This round's words (meanings listed on the left).
    pub round: Vec<Puzzle>,
    /// The word being solved.
    pub sel: usize,
    pub current: String,
    /// A short popup over the board ("단어 목록에 없어요", the answer after six misses).
    pub popup: Option<(String, Instant)>,
    pub played: u32,
    pub won: u32,
    pub streak: u32,
    /// Words already given this session (not repeated until all were played).
    pub used: HashSet<String>,
}

impl WordleState {
    pub fn puzzle(&self) -> Option<&Puzzle> {
        self.round.get(self.sel)
    }
    /// The next unfinished word after the selected one.
    fn next_open(&self) -> Option<usize> {
        let n = self.round.len();
        (1..=n).map(|k| (self.sel + k) % n).find(|&i| !self.round[i].over())
    }
}

/// Colours for `guess` against `answer` (both lowercase, same length). A repeated letter is
/// marked only as many times as the answer has it, exact places first.
pub fn score(guess: &str, answer: &str) -> Vec<Mark> {
    let g: Vec<char> = guess.chars().collect();
    let a: Vec<char> = answer.chars().collect();
    let mut marks = vec![Mark::Miss; g.len()];
    let mut left: Vec<Option<char>> = a.iter().map(|&c| Some(c)).collect();
    for i in 0..g.len() {
        if a.get(i) == Some(&g[i]) {
            marks[i] = Mark::Hit;
            left[i] = None;
        }
    }
    for i in 0..g.len() {
        if marks[i] == Mark::Hit {
            continue;
        }
        if let Some(slot) = left.iter_mut().find(|c| **c == Some(g[i])) {
            marks[i] = Mark::Near;
            *slot = None;
        }
    }
    marks
}

/// A word that can be an answer: plain letters, 4–12 long.
fn playable(w: &str) -> bool {
    (MIN_LEN..=MAX_LEN).contains(&w.len()) && w.chars().all(|c| c.is_ascii_alphabetic())
}

fn hit() -> Color32 {
    Color32::from_rgb(106, 170, 100)
}
fn near() -> Color32 {
    Color32::from_rgb(201, 180, 88)
}
fn miss() -> Color32 {
    Color32::from_rgb(120, 124, 126)
}
fn mark_color(m: Mark) -> Color32 {
    match m {
        Mark::Hit => hit(),
        Mark::Near => near(),
        Mark::Miss => miss(),
    }
}

impl App {
    /// Learned words the game can use.
    pub fn wordle_pool(&self) -> Vec<Answer> {
        let mut seen = HashSet::new();
        self.store
            .items
            .iter()
            .filter(|i| i.is_vocab_word() && !i.meaning_ko.trim().is_empty())
            .filter_map(|i| {
                let w = i.text.trim().to_lowercase();
                (playable(&w) && seen.insert(w.clone())).then(|| Answer { word: w, ko: i.meaning_ko.clone(), example: i.example.clone() })
            })
            .collect()
    }

    /// A new round of up to `ROUND_WORDS` learned words.
    fn wordle_new_round(&mut self) {
        let pool = self.wordle_pool();
        let st = &mut self.wordle;
        if pool.iter().filter(|a| !st.used.contains(&a.word)).count() < ROUND_WORDS.min(pool.len()) {
            st.used.clear();
        }
        let fresh: Vec<&Answer> = pool.iter().filter(|a| !st.used.contains(&a.word)).collect();
        st.round = fresh.choose_multiple(&mut rand::rng(), ROUND_WORDS).map(|a| Puzzle { answer: (*a).clone(), guesses: Vec::new() }).collect();
        for p in &st.round {
            st.used.insert(p.answer.word.clone());
        }
        st.sel = 0;
        st.current.clear();
        st.popup = None;
    }

    /// A guess is accepted when it is a real word: in the 6,000-word list, among the learned
    /// words, or a regular -s / -es / -ed / -ing form of one of them.
    fn wordle_known_word(&self, w: &str) -> bool {
        let known = |x: &str| crate::wordbank::all().iter().any(|b| b.word.eq_ignore_ascii_case(x)) || self.store.items.iter().any(|i| i.text.eq_ignore_ascii_case(x));
        if known(w) {
            return true;
        }
        ["s", "es", "ed", "d", "ing"].iter().any(|suf| w.strip_suffix(suf).is_some_and(|stem| stem.len() >= 2 && (known(stem) || known(&format!("{stem}e")))))
    }

    fn wordle_type(&mut self, c: char) {
        let Some(p) = self.wordle.puzzle() else { return };
        if !p.over() && self.wordle.current.len() < p.answer.word.len() && c.is_ascii_alphabetic() {
            self.wordle.current.push(c.to_ascii_lowercase());
        }
    }

    fn wordle_back(&mut self) {
        if self.wordle.puzzle().is_some_and(|p| !p.over()) {
            self.wordle.current.pop();
        }
    }

    fn wordle_enter(&mut self) {
        let Some(p) = self.wordle.puzzle().cloned() else { return };
        if p.over() {
            return;
        }
        let a = p.answer;
        let g = self.wordle.current.clone();
        if g.len() < a.word.len() {
            self.wordle.popup = Some((format!("{}글자를 모두 채워 주세요", a.word.len()), Instant::now()));
            return;
        }
        if g != a.word && !self.wordle_known_word(&g) {
            self.wordle.popup = Some(("단어 목록에 없는 단어예요".into(), Instant::now()));
            return;
        }
        let st = &mut self.wordle;
        let sel = st.sel;
        st.round[sel].guesses.push(g);
        st.current.clear();
        let p = &st.round[sel];
        if p.over() {
            st.played += 1;
            if p.solved() {
                st.won += 1;
                st.streak += 1;
                let praise = ["천재예요!", "대단해요!", "훌륭해요!", "잘했어요!", "좋아요!", "휴, 맞혔어요!"];
                st.popup = Some((praise[(p.guesses.len() - 1).min(5)].into(), Instant::now()));
            } else {
                st.streak = 0;
                st.popup = Some((a.word.to_uppercase(), Instant::now()));
            }
            crate::applog!("wordle: {} · {}", a.word, if p.solved() { "맞힘" } else { "못 맞힘" });
            self.voice.say(&a.word);
        }
    }

    fn wordle_select(&mut self, i: usize) {
        if i != self.wordle.sel {
            self.wordle.sel = i;
            self.wordle.current.clear();
            self.wordle.popup = None;
        }
    }

    /// Best mark seen for each letter of the selected word (for the on-screen keyboard).
    fn wordle_key_marks(&self) -> std::collections::HashMap<char, Mark> {
        let mut m = std::collections::HashMap::new();
        let Some(p) = self.wordle.puzzle() else { return m };
        for g in &p.guesses {
            for (c, mk) in g.chars().zip(score(g, &p.answer.word)) {
                let e = m.entry(c).or_insert(mk);
                if mk == Mark::Hit || (mk == Mark::Near && *e == Mark::Miss) {
                    *e = mk;
                }
            }
        }
        m
    }

    pub fn wordle_ui(&mut self, ui: &mut Ui) {
        if self.wordle.round.is_empty() {
            self.wordle_new_round();
        }
        if self.wordle.round.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("🔠 낱말 맞추기").size(18.0).strong());
                ui.label(RichText::new(format!("새 단어에서 {MIN_LEN}글자 이상 단어를 배우면 여기서 맞추기 게임을 할 수 있어요.")).color(muted()));
            });
            return;
        }

        // Typing on the keyboard (nothing else takes keys on this tab).
        let (typed, back, enter) = ui.input(|i| {
            let typed: String = i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect();
            (typed, i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::Enter))
        });
        for c in typed.chars() {
            self.wordle_type(c);
        }
        if back {
            self.wordle_back();
        }
        if enter {
            self.wordle_enter();
        }

        card(ui, |ui| {
            let n = self.wordle.round.len();
            let solved = self.wordle.round.iter().filter(|p| p.solved()).count();
            ui.label(RichText::new(format!("🔠 이미 배운 단어 {n}개를 맞혀 보세요 — 왼쪽 뜻을 골라 단어마다 {TRIES}번 기회")).size(17.0).strong().color(text_strong()));
            ui.label(RichText::new("초록 = 자리까지 맞음 · 노랑 = 단어에 있지만 다른 자리 · 회색 = 없는 글자. 키보드로 입력하고 Enter.").size(13.5).color(muted()));
            let win = if self.wordle.played > 0 { self.wordle.won * 100 / self.wordle.played } else { 0 };
            ui.label(RichText::new(format!("이번 라운드 {solved}/{n} 맞힘 · 전체 {}단어 · 맞힘 {win}% · 연속 {}", self.wordle.played, self.wordle.streak)).size(13.5).color(muted()));
        });
        ui.add_space(10.0);

        ui.horizontal_top(|ui| {
            // Left: the meanings (pick the word to solve).
            ui.allocate_ui_with_layout(Vec2::new(250.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(250.0);
                card(ui, |ui| {
                    ui.label(RichText::new("📖 한글 뜻").size(15.0).strong().color(text_strong()));
                    ui.add_space(4.0);
                    let mut pick = None;
                    for (i, p) in self.wordle.round.iter().enumerate() {
                        let (icon, col) = if p.solved() {
                            ("✓", GOOD)
                        } else if p.over() {
                            ("✖", BAD)
                        } else if i == self.wordle.sel {
                            ("▶", ACCENT)
                        } else {
                            ("·", muted())
                        };
                        let status = if p.over() { p.answer.word.clone() } else { format!("{}글자 · {}/{TRIES}", p.answer.word.len(), p.guesses.len()) };
                        let text = format!("{icon} {}. {}\n     {status}", i + 1, p.answer.ko);
                        let selected = i == self.wordle.sel;
                        let resp = ui.add(
                            egui::Button::new(RichText::new(text).size(15.0).color(if selected { Color32::WHITE } else { col }))
                                .fill(if selected { ACCENT } else { subtle_fill() })
                                .wrap()
                                .min_size(Vec2::new(ui.available_width(), 44.0)),
                        );
                        if resp.clicked() {
                            pick = Some(i);
                        }
                        ui.add_space(2.0);
                    }
                    if let Some(i) = pick {
                        self.wordle_select(i);
                    }
                });
            });
            ui.add_space(14.0);
            ui.vertical(|ui| self.wordle_board_ui(ui));
        });
    }

    /// The selected word's board, then its result or the on-screen keyboard.
    fn wordle_board_ui(&mut self, ui: &mut Ui) {
        let Some(p) = self.wordle.puzzle().cloned() else { return };
        let a = &p.answer;
        let len = a.word.len();
        let gap = 6.0;
        let tile = ((ui.available_width() - (len - 1) as f32 * gap) / len as f32).clamp(30.0, 58.0);
        let board_w = len as f32 * tile + (len - 1) as f32 * gap;
        let board_h = TRIES as f32 * tile + (TRIES - 1) as f32 * gap;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(board_w), board_h), egui::Sense::hover());
        let origin = egui::pos2(rect.center().x - board_w / 2.0, rect.top());
        let painter = ui.painter_at(rect.expand(4.0));
        for row in 0..TRIES {
            let (text, marks): (String, Option<Vec<Mark>>) = match p.guesses.get(row) {
                Some(g) => (g.clone(), Some(score(g, &a.word))),
                None if row == p.guesses.len() => (self.wordle.current.clone(), None),
                None => (String::new(), None),
            };
            let chars: Vec<char> = text.chars().collect();
            for col in 0..len {
                let r = egui::Rect::from_min_size(origin + Vec2::new(col as f32 * (tile + gap), row as f32 * (tile + gap)), Vec2::splat(tile));
                let ch = chars.get(col).copied();
                match &marks {
                    Some(m) => {
                        painter.rect_filled(r, 3.0, mark_color(m[col]));
                    }
                    None => {
                        painter.rect_filled(r, 3.0, card_bg());
                        let stroke = if ch.is_some() { muted() } else { border() };
                        painter.rect_stroke(r, 3.0, egui::Stroke::new(2.0, stroke), egui::StrokeKind::Inside);
                    }
                }
                if let Some(c) = ch {
                    let col = if marks.is_some() { Color32::WHITE } else { text_strong() };
                    painter.text(r.center(), egui::Align2::CENTER_CENTER, c.to_ascii_uppercase(), egui::FontId::proportional(tile * 0.52), col);
                }
            }
        }
        // Popup over the top of the board.
        if let Some((msg, at)) = &self.wordle.popup {
            let stays = p.over() && !p.solved();
            if stays || at.elapsed() < Duration::from_millis(1600) {
                let galley = ui.painter().layout_no_wrap(msg.clone(), egui::FontId::proportional(18.0), Color32::WHITE);
                let pr = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 2.0), galley.size() + Vec2::new(28.0, 18.0));
                let fg = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("wordle_popup")));
                fg.rect_filled(pr, 6.0, Color32::BLACK);
                fg.galley(pr.center() - galley.size() / 2.0, galley, Color32::WHITE);
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            } else {
                self.wordle.popup = None;
            }
        }
        ui.add_space(14.0);

        if p.over() {
            card(ui, |ui| {
                let solved = p.solved();
                ui.label(RichText::new(if solved { format!("🎉 {}번 만에 맞혔어요!", p.guesses.len()) } else { "😅 아쉬워요. 정답은:".to_string() }).size(16.0).color(if solved { GOOD } else { warn() }));
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&a.word).size(28.0).strong().color(text_strong()));
                    ui.label(RichText::new(&a.ko).size(17.0).color(muted()));
                    if ui.button("🔊 듣기").clicked() {
                        self.voice.say(&a.word);
                    }
                });
                if !a.example.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&a.example).size(16.0).italics());
                        if ui.small_button("🔊").on_hover_text("예문 듣기").clicked() {
                            self.voice.say(&a.example);
                        }
                    });
                }
                ui.add_space(6.0);
                match self.wordle.next_open() {
                    Some(i) => {
                        if ui.add(egui::Button::new(RichText::new("다음 단어 ▶").size(17.0).color(Color32::WHITE)).fill(ACCENT).min_size(BIG_BUTTON)).clicked() {
                            self.wordle_select(i);
                        }
                    }
                    None => {
                        let n = self.wordle.round.len();
                        let solved = self.wordle.round.iter().filter(|p| p.solved()).count();
                        ui.label(RichText::new(format!("🏁 라운드 끝 — {n}개 중 {solved}개 맞혔어요")).size(16.0).strong().color(text_strong()));
                        if ui.add(egui::Button::new(RichText::new("새 라운드 ▶").size(17.0).color(Color32::WHITE)).fill(ACCENT).min_size(BIG_BUTTON)).clicked() {
                            self.wordle_new_round();
                        }
                    }
                }
            });
            return;
        }

        // On-screen keyboard.
        let marks = self.wordle_key_marks();
        let rows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
        let key_w = 40.0;
        let key_h = 48.0;
        let mut pressed: Option<char> = None;
        let (mut back, mut enter) = (false, false);
        for (ri, keys) in rows.iter().enumerate() {
            let extra = if ri == 2 { 2.0 * (64.0 + 5.0) } else { 0.0 };
            let w = keys.len() as f32 * (key_w + 5.0) - 5.0 + extra;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                if ri == 2 && ui.add(egui::Button::new(RichText::new("입력").size(15.0).strong()).min_size(Vec2::new(64.0, key_h))).clicked() {
                    enter = true;
                }
                for c in keys.chars() {
                    let (fill, txt) = match marks.get(&c) {
                        Some(m) => (mark_color(*m), Color32::WHITE),
                        None => (subtle_fill(), text_strong()),
                    };
                    if ui.add(egui::Button::new(RichText::new(c.to_ascii_uppercase().to_string()).size(18.0).strong().color(txt)).fill(fill).min_size(Vec2::new(key_w, key_h))).clicked() {
                        pressed = Some(c);
                    }
                }
                if ri == 2 && ui.add(egui::Button::new(RichText::new("지우기").size(15.0)).min_size(Vec2::new(64.0, key_h))).clicked() {
                    back = true;
                }
            });
            ui.add_space(5.0);
        }
        if let Some(c) = pressed {
            self.wordle_type(c);
        }
        if back {
            self.wordle_back();
        }
        if enter {
            self.wordle_enter();
        }
        ui.add_space(6.0);
        if ui.small_button("🔄 새 라운드").on_hover_text("다른 단어들로 새로 시작해요").clicked() {
            self.wordle_new_round();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_repeated_letters_like_wordle() {
        use Mark::*;
        assert_eq!(score("asset", "askew"), vec![Hit, Hit, Miss, Hit, Miss]);
        assert_eq!(score("trade", "askew"), vec![Miss, Miss, Near, Miss, Near]);
        assert_eq!(score("agent", "askew"), vec![Hit, Miss, Near, Miss, Miss]);
        assert_eq!(score("eerie", "there"), vec![Near, Miss, Near, Miss, Hit]);
        assert_eq!(score("askew", "askew"), vec![Hit; 5]);
    }
}
