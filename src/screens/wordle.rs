//! 🔠 낱말 맞추기 (복습 menu): a round of several words already learned, each guessed in six
//! tries, Wordle style, in kana — the answer is the word's reading in hiragana; green = right
//! kana in the right place, yellow = in the word elsewhere, grey = not in it. The Korean
//! meanings are listed on the left (pick one to solve); typed with the keyboard's Japanese input
//! or with the on-screen 50음도 keys; a finished word is read aloud with its meaning and example.

use crate::app::App;
use crate::widgets::*;
use egui::{Color32, RichText, Ui, Vec2};
use rand::seq::IndexedRandom;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub const TRIES: usize = 6;
const MIN_LEN: usize = 2;
const MAX_LEN: usize = 7;
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
    /// The reading to guess (hiragana).
    pub word: String,
    /// The word as written (食べる).
    pub written: String,
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

/// A reading that can be an answer: hiragana (and ー), 2–7 kana long.
fn playable(w: &str) -> bool {
    (MIN_LEN..=MAX_LEN).contains(&w.chars().count()) && w.chars().all(|c| crate::ja::is_hiragana(c) || c == 'ー')
}

/// Kana as the board takes it: katakana typed or tapped becomes hiragana.
fn kana(c: char) -> Option<char> {
    let c = crate::ja::to_hiragana(&c.to_string()).chars().next()?;
    (crate::ja::is_hiragana(c) || c == 'ー').then_some(c)
}

/// The reading of a learned word in hiragana (from the word list, else the analyser).
fn reading_of(text: &str) -> String {
    let r = crate::wordbank::find(text).map(|b| b.reading.clone()).unwrap_or_else(|| crate::ja::reading(text));
    crate::ja::to_hiragana(&r)
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
                let w = reading_of(i.text.trim());
                (playable(&w) && seen.insert(w.clone())).then(|| Answer { word: w, written: i.text.trim().to_string(), ko: i.meaning_ko.clone(), example: i.example.clone() })
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

    /// A guess is accepted when it is the reading of a real word: in the word list, among the
    /// learned words, or one the dictionary knows.
    fn wordle_known_word(&self, w: &str) -> bool {
        crate::wordbank::all().iter().any(|b| crate::ja::to_hiragana(&b.reading) == w)
            || self.store.items.iter().any(|i| reading_of(&i.text) == w)
            || crate::ja::tokenize(w).len() == 1 && crate::ja::tokenize(w).iter().all(|t| !t.reading.is_empty() && t.pos != "助詞")
    }

    fn wordle_type(&mut self, c: char) {
        let Some(p) = self.wordle.puzzle() else { return };
        if let Some(c) = kana(c) {
            if !p.over() && self.wordle.current.chars().count() < p.answer.word.chars().count() {
                self.wordle.current.push(c);
            }
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
        if g.chars().count() < a.word.chars().count() {
            self.wordle.popup = Some((format!("{}글자를 모두 채워 주세요", a.word.chars().count()), Instant::now()));
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
                st.popup = Some((format!("{} ({})", a.written, a.word), Instant::now()));
            }
            crate::applog!("wordle: {} · {}", a.word, if p.solved() { "맞힘" } else { "못 맞힘" });
            self.voice.say(&a.written);
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
                ui.label(RichText::new(format!("새 단어에서 읽기가 {MIN_LEN}~{MAX_LEN}글자인 단어를 배우면 여기서 맞추기 게임을 할 수 있어요.")).color(muted()));
            });
            return;
        }

        // Typing on the keyboard (nothing else takes keys on this tab): the OS's Japanese input
        // sends its committed kana as IME events.
        let (typed, back, enter) = ui.input(|i| {
            let typed: String = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) | egui::Event::Ime(egui::ImeEvent::Commit(t)) => Some(t.clone()),
                    _ => None,
                })
                .collect();
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
            ui.label(RichText::new(format!("🔠 이미 배운 단어 {n}개의 읽기(히라가나)를 맞혀 보세요 — 왼쪽 뜻을 골라 단어마다 {TRIES}번 기회")).size(17.0).strong().color(text_strong()));
            ui.label(RichText::new("초록 = 자리까지 맞음 · 노랑 = 단어에 있지만 다른 자리 · 회색 = 없는 글자. 아래 50음도를 누르거나 일본어 입력기로 치고 Enter.").size(13.5).color(muted()));
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
                        let status = if p.over() { format!("{} ({})", p.answer.written, p.answer.word) } else { format!("{}글자 · {}/{TRIES}", p.answer.word.chars().count(), p.guesses.len()) };
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
        let len = a.word.chars().count();
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
                    painter.text(r.center(), egui::Align2::CENTER_CENTER, c, egui::FontId::proportional(tile * 0.52), col);
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
                    ui.label(RichText::new(&a.written).size(28.0).strong().color(text_strong()));
                    ui.label(RichText::new(&a.word).size(18.0).color(gloss()));
                    ui.label(RichText::new(&a.ko).size(17.0).color(muted()));
                    if ui.button("🔊 듣기").clicked() {
                        self.voice.say(&a.written);
                    }
                });
                if !a.example.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ja_text(ui, &a.example, 16.0, text_strong());
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

        // On-screen 50음도: one column per consonant row (あ か さ … わ), then voiced and small kana.
        let marks = self.wordle_key_marks();
        const COLUMNS: [&str; 16] = ["あいうえお", "かきくけこ", "さしすせそ", "たちつてと", "なにぬねの", "はひふへほ", "まみむめも", "や ゆ よ", "らりるれろ", "わ を ん", "がぎぐげご", "ざじずぜぞ", "だぢづでど", "ばびぶべぼ", "ぱぴぷぺぽ", "ゃゅょっー"];
        let key = 34.0;
        let mut pressed: Option<char> = None;
        let (mut back, mut enter) = (false, false);
        let w = COLUMNS.len() as f32 * (key + 3.0);
        for row in 0..5 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                // Right to left like a printed 50음도 table: あ column on the right.
                for col in COLUMNS.iter().rev() {
                    let c = col.chars().nth(row).unwrap_or(' ');
                    if c == ' ' {
                        ui.add_space(key);
                        continue;
                    }
                    let (fill, txt) = match marks.get(&c) {
                        Some(m) => (mark_color(*m), Color32::WHITE),
                        None => (subtle_fill(), text_strong()),
                    };
                    if ui.add(egui::Button::new(RichText::new(c.to_string()).size(16.0).strong().color(txt)).fill(fill).min_size(Vec2::new(key, key))).clicked() {
                        pressed = Some(c);
                    }
                }
            });
            ui.add_space(3.0);
        }
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - 2.0 * 90.0) / 2.0).max(0.0));
            if ui.add(egui::Button::new(RichText::new("입력").size(15.0).strong()).min_size(Vec2::new(84.0, 40.0))).clicked() {
                enter = true;
            }
            if ui.add(egui::Button::new(RichText::new("지우기").size(15.0)).min_size(Vec2::new(84.0, 40.0))).clicked() {
                back = true;
            }
        });
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
        assert_eq!(score("たべる", "たべる"), vec![Hit; 3]);
        assert_eq!(score("たてる", "かたる"), vec![Near, Miss, Hit]);
        assert_eq!(score("ここ", "こい"), vec![Hit, Miss], "a repeated kana counts once");
        assert_eq!(score("いえ", "えい"), vec![Near, Near]);
    }

    #[test]
    fn readings_are_hiragana_answers() {
        assert!(playable("たべる") && playable("コーヒー".chars().map(|c| kana(c).unwrap()).collect::<String>().as_str()));
        assert!(!playable("た") && !playable("食べる"));
        assert_eq!(kana('カ'), Some('か'));
        assert_eq!(kana('a'), None);
        assert_eq!(reading_of("食べる"), "たべる");
    }
}
