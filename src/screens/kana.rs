//! 글자 카드 at the top of 새 단어: 히라가나·가타카나 (N5 learners, 50음 + 탁음·반탁음) and the
//! 한자 every level must know (JLPT N5 79 → N1 2,211 in all). A card shows one character; the
//! learner picks its sound (kana) or 훈음 (kanji) from four. Three right answers in a row = 기억 완료
//! (counts in `Profile::kana` / `Profile::kanji`, by the character itself).

use crate::app::App;
use crate::widgets::*;
use egui::{RichText, Ui};
use rand::seq::SliceRandom;

/// (hiragana, katakana, romaji, 한글 발음) in 50음도 order, five columns (あ い う え お) per
/// row; "" = an empty cell of the chart (や行, わ行).
pub const KANA: &[(&str, &str, &str, &str)] = &[
    ("あ", "ア", "a", "아"), ("い", "イ", "i", "이"), ("う", "ウ", "u", "우"), ("え", "エ", "e", "에"), ("お", "オ", "o", "오"),
    ("か", "カ", "ka", "카"), ("き", "キ", "ki", "키"), ("く", "ク", "ku", "쿠"), ("け", "ケ", "ke", "케"), ("こ", "コ", "ko", "코"),
    ("さ", "サ", "sa", "사"), ("し", "シ", "shi", "시"), ("す", "ス", "su", "스"), ("せ", "セ", "se", "세"), ("そ", "ソ", "so", "소"),
    ("た", "タ", "ta", "타"), ("ち", "チ", "chi", "치"), ("つ", "ツ", "tsu", "츠"), ("て", "テ", "te", "테"), ("と", "ト", "to", "토"),
    ("な", "ナ", "na", "나"), ("に", "ニ", "ni", "니"), ("ぬ", "ヌ", "nu", "누"), ("ね", "ネ", "ne", "네"), ("の", "ノ", "no", "노"),
    ("は", "ハ", "ha", "하"), ("ひ", "ヒ", "hi", "히"), ("ふ", "フ", "fu", "후"), ("へ", "ヘ", "he", "헤"), ("ほ", "ホ", "ho", "호"),
    ("ま", "マ", "ma", "마"), ("み", "ミ", "mi", "미"), ("む", "ム", "mu", "무"), ("め", "メ", "me", "메"), ("も", "モ", "mo", "모"),
    ("や", "ヤ", "ya", "야"), ("", "", "", ""), ("ゆ", "ユ", "yu", "유"), ("", "", "", ""), ("よ", "ヨ", "yo", "요"),
    ("ら", "ラ", "ra", "라"), ("り", "リ", "ri", "리"), ("る", "ル", "ru", "루"), ("れ", "レ", "re", "레"), ("ろ", "ロ", "ro", "로"),
    ("わ", "ワ", "wa", "와"), ("", "", "", ""), ("ん", "ン", "n", "응(ㄴ)"), ("", "", "", ""), ("を", "ヲ", "wo", "오(조사)"),
    ("が", "ガ", "ga", "가"), ("ぎ", "ギ", "gi", "기"), ("ぐ", "グ", "gu", "구"), ("げ", "ゲ", "ge", "게"), ("ご", "ゴ", "go", "고"),
    ("ざ", "ザ", "za", "자"), ("じ", "ジ", "ji", "지"), ("ず", "ズ", "zu", "즈"), ("ぜ", "ゼ", "ze", "제"), ("ぞ", "ゾ", "zo", "조"),
    ("だ", "ダ", "da", "다"), ("ぢ", "ヂ", "di", "지(ぢ)"), ("づ", "ヅ", "du", "즈(づ)"), ("で", "デ", "de", "데"), ("ど", "ド", "do", "도"),
    ("ば", "バ", "ba", "바"), ("び", "ビ", "bi", "비"), ("ぶ", "ブ", "bu", "부"), ("べ", "ベ", "be", "베"), ("ぼ", "ボ", "bo", "보"),
    ("ぱ", "パ", "pa", "파"), ("ぴ", "ピ", "pi", "피"), ("ぷ", "プ", "pu", "푸"), ("ぺ", "ペ", "pe", "페"), ("ぽ", "ポ", "po", "포"),
];

/// Right answers in a row that make a card 기억 완료.
pub const KANA_DONE: u32 = 3;
/// Cards per round.
pub const ROUND: usize = 10;
/// 한자 cards to answer every day (required, shown in the card and on the home checklist).
pub const KANJI_PER_DAY: u32 = 20;

/// Seconds a right answer stays on screen while its sound plays, before the next card.
const KANA_HOLD: f64 = 1.2;
/// Kanji: the example word is longer.
const KANJI_HOLD: f64 = 2.0;

/// Rows of KANA that are 탁음·반탁음 (shown under the 50음 in the chart).
const BASIC_ROWS: usize = 11;

/// The three decks of cards.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Deck {
    Hiragana,
    Katakana,
    /// 한자 up to this JLPT grade (5 = N5 … 1 = N1).
    Kanji(u8),
}

impl Deck {
    fn name(self) -> &'static str {
        match self {
            Deck::Hiragana => "히라가나",
            Deck::Katakana => "가타카나",
            Deck::Kanji(_) => "한자",
        }
    }
    fn icon(self) -> &'static str {
        if let Deck::Kanji(_) = self { "🈶" } else { "🔤" }
    }
    /// Cards of the deck in learning order (chart order / easiest grade, most frequent first).
    fn pool(self) -> Vec<usize> {
        match self {
            Deck::Kanji(g) => crate::kanji::required(g),
            _ => all(),
        }
    }
    /// The character shown on card `i`.
    pub fn front(self, i: usize) -> &'static str {
        match self {
            Deck::Hiragana => KANA[i].0,
            Deck::Katakana => KANA[i].1,
            Deck::Kanji(_) => &crate::kanji::all()[i].k,
        }
    }
    /// The right answer: "아 · a" / "날 일" + 読み方 + Japanese meaning.
    pub fn answer(self, i: usize) -> String {
        match self {
            Deck::Kanji(_) => crate::kanji::all()[i].card_label(),
            _ => sound(i),
        }
    }
    /// What the speaker reads after an answer.
    fn speak(self, i: usize) -> String {
        match self {
            Deck::Kanji(_) => crate::kanji::all()[i].speak(),
            _ => self.front(i).to_string(),
        }
    }
}

/// Answer label: "아 · a".
pub fn sound(i: usize) -> String {
    format!("{} · {}", KANA[i].3, KANA[i].2)
}

/// Indexes of the real kana (no empty cells).
fn all() -> Vec<usize> {
    (0..KANA.len()).filter(|&i| !KANA[i].0.is_empty()).collect()
}

/// The next round: cards not memorised yet in learning order (so learning goes row by row /
/// N5 → the learner's grade), then the next grade's kanji once those are all known, filled up
/// with memorised ones for review, shuffled.
pub fn round(known: &std::collections::HashMap<String, u32>, deck: Deck) -> Vec<usize> {
    let passes = |i: usize| known.get(deck.front(i)).copied().unwrap_or(0);
    let mut pool = deck.pool();
    if let Deck::Kanji(g) = deck {
        if g > 1 && pool.iter().all(|&i| passes(i) >= KANA_DONE) {
            pool = crate::kanji::required(g - 1);
        }
    }
    let mut pick: Vec<usize> = pool.iter().copied().filter(|&i| passes(i) < KANA_DONE).take(ROUND).collect();
    let mut done: Vec<usize> = pool.into_iter().filter(|&i| passes(i) >= KANA_DONE).collect();
    done.shuffle(&mut rand::rng());
    pick.extend(done.into_iter().take(ROUND - pick.len()));
    pick.shuffle(&mut rand::rng());
    pick
}

/// Four choices with the answer among them; never two with the same label. Kanji choices come
/// from the same grade, so they are about as familiar.
fn choices(deck: Deck, answer: usize) -> Vec<usize> {
    let label = deck.answer(answer);
    let pool: Vec<usize> = match deck {
        Deck::Kanji(_) => {
            let n = crate::kanji::all()[answer].n;
            (0..crate::kanji::all().len()).filter(|&i| crate::kanji::all()[i].n == n).collect()
        }
        _ => all(),
    };
    let mut c: Vec<usize> = Vec::new();
    let mut others: Vec<usize> = pool.into_iter().filter(|&i| i != answer).collect();
    others.shuffle(&mut rand::rng());
    for i in others {
        let l = deck.answer(i);
        if l != label && c.iter().all(|&j| deck.answer(j) != l) {
            c.push(i);
        }
        if c.len() == 3 {
            break;
        }
    }
    c.push(answer);
    c.shuffle(&mut rand::rng());
    c
}

/// One round of cards.
pub struct KanaQuiz {
    pub deck: Deck,
    pub cards: Vec<usize>,
    pub idx: usize,
    pub choices: Vec<usize>,
    pub picked: Option<usize>,
    pub right: usize,
    /// "✓ 정답! 日 = 날 일" for the card just answered right (the next one comes at once).
    pub last_ok: Option<String>,
    /// After a right answer the card stays (green) until this egui time, so the sound heard
    /// matches the character on screen; then the next card comes by itself.
    pub advance_at: Option<f64>,
    /// Card whose sound was synthesized ahead (VOICEVOX), so it plays the moment it's answered.
    pub prefetched: Option<usize>,
}

impl KanaQuiz {
    pub fn new(cards: Vec<usize>, deck: Deck) -> Self {
        let choices = cards.first().map(|&a| choices(deck, a)).unwrap_or_default();
        Self { deck, cards, idx: 0, choices, picked: None, right: 0, last_ok: None, advance_at: None, prefetched: None }
    }
}

impl App {
    fn known_map(&self, deck: Deck) -> &std::collections::HashMap<String, u32> {
        if let Deck::Kanji(_) = deck { &self.store.profile.kanji } else { &self.store.profile.kana }
    }

    /// Memorised cards of the deck.
    fn deck_known(&self, deck: Deck) -> usize {
        let m = self.known_map(deck);
        deck.pool().into_iter().filter(|&i| m.get(deck.front(i)).copied().unwrap_or(0) >= KANA_DONE).count()
    }

    /// N5 (초급) learners get the kana cards first in 새 단어.
    pub fn kana_cards_shown(&self) -> bool {
        self.store.profile.level < crate::models::LEVEL_BANDS[0].0
    }

    /// The 한자 deck for the learner's level.
    pub fn kanji_deck(&self) -> Deck {
        Deck::Kanji(crate::kanji::grade_of(crate::models::cefr_of(self.store.profile.level)))
    }

    /// 한자 cards answered today (KANJI_PER_DAY a day is required).
    pub fn kanji_today(&self) -> u32 {
        self.store.profile.today.kanji_cards
    }

    fn start_round(&mut self, deck: Deck) {
        crate::applog!("words: {} 카드 시작", deck.name());
        let cards = round(self.known_map(deck), deck);
        self.kana_quiz = Some(KanaQuiz::new(cards, deck));
    }

    /// 🔤 card at the top of 새 단어: progress per script, start buttons and the 50음도 chart.
    pub fn kana_card(&mut self, ui: &mut Ui) {
        let total = all().len();
        let (hira, kata) = (self.deck_known(Deck::Hiragana), self.deck_known(Deck::Katakana));
        let mut start = None;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🔤 히라가나·가타카나 카드").size(18.0).strong());
                chip(ui, "N5 · 글자부터", crate::art::VIOLET);
            });
            ui.label(RichText::new(format!("단어보다 먼저 글자를 외워요. 카드의 글자를 보고 소리를 4개 중에서 고르세요. {KANA_DONE}번 연속 맞히면 기억 완료!")).color(muted()));
            ui.add_space(6.0);
            for (deck, known) in [(Deck::Hiragana, hira), (Deck::Katakana, kata)] {
                let name = deck.name();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{name} {known}/{total}")).strong().color(if known >= total { GOOD } else { text_strong() }));
                    ui.add(egui::ProgressBar::new(known as f32 / total as f32).desired_width(160.0).desired_height(8.0).fill(if known >= total { GOOD } else { ACCENT }));
                    let label = if known >= total { format!("↻ {name} 복습") } else { format!("▶ {name} 카드 {ROUND}장") };
                    let fill = if known >= total || (deck == Deck::Katakana && hira < total) { subtle_fill() } else { ACCENT };
                    if ui.add(egui::Button::new(RichText::new(label).size(15.0)).fill(fill).min_size(ACTION_BUTTON)).clicked() {
                        start = Some(deck);
                    }
                });
            }
            ui.add_space(4.0);
            egui::CollapsingHeader::new("50음도 표 보기 (글자를 누르면 소리가 나요)").id_salt("kana_chart").show(ui, |ui| {
                for deck in [Deck::Hiragana, Deck::Katakana] {
                    ui.label(RichText::new(deck.name()).strong());
                    self.kana_chart(ui, deck);
                    ui.add_space(8.0);
                }
            });
        });
        if let Some(deck) = start {
            self.start_round(deck);
        }
        ui.add_space(14.0);
    }

    /// 🈶 한자 카드 (every level): the level's required kanji, today's required cards, the chart.
    pub fn kanji_card(&mut self, ui: &mut Ui) {
        let deck = self.kanji_deck();
        let Deck::Kanji(g) = deck else { return };
        let total = deck.pool().len();
        let known = self.deck_known(deck);
        let today = self.kanji_today();
        let jlpt = crate::models::cefr_of(self.store.profile.level);
        let mut start = false;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🈶 한자 카드").size(18.0).strong());
                chip(ui, &format!("{jlpt} 필수 한자 {total}자"), crate::art::VIOLET);
                let (c, t) = if today >= KANJI_PER_DAY { (GOOD, format!("✓ 오늘 {today}/{KANJI_PER_DAY}장")) } else { (warn(), format!("오늘 {today}/{KANJI_PER_DAY}장 (필수)")) };
                chip(ui, &t, c);
            });
            ui.label(RichText::new(format!("{jlpt}까지 꼭 알아야 하는 한자 {total}자(N5부터 차례로)를 모두 외워요. 한자를 보고 훈음·읽는 법(음독·훈독)·일본어 뜻을 4개 중에서 고르고, 예시 단어로 확인해요. 하루 {KANJI_PER_DAY}장은 꼭!")).color(muted()));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("기억 완료 {known}/{total}")).strong().color(if known >= total { GOOD } else { text_strong() }));
                ui.add(egui::ProgressBar::new(known as f32 / total.max(1) as f32).desired_width(160.0).desired_height(8.0).fill(if known >= total { GOOD } else { ACCENT }));
                let label = if known >= total && g > 1 { format!("▶ 다음 레벨 한자 {ROUND}장") } else { format!("▶ 한자 카드 {ROUND}장") };
                let fill = if today >= KANJI_PER_DAY { subtle_fill() } else { ACCENT };
                start = ui.add(egui::Button::new(RichText::new(label).size(15.0)).fill(fill).min_size(ACTION_BUTTON)).clicked();
            });
            ui.horizontal_wrapped(|ui| {
                for n in (g..=5).rev() {
                    let ids: Vec<usize> = deck.pool().into_iter().filter(|&i| crate::kanji::all()[i].n == n).collect();
                    let k = ids.iter().filter(|&&i| self.store.profile.kanji.get(deck.front(i)).copied().unwrap_or(0) >= KANA_DONE).count();
                    chip(ui, &format!("N{n} {k}/{}", ids.len()), if k >= ids.len() { GOOD } else { muted() });
                }
            });
            ui.add_space(4.0);
            egui::CollapsingHeader::new(format!("{jlpt}까지 한자 표 보기 (누르면 훈음·읽기)")).id_salt("kanji_chart").show(ui, |ui| {
                self.kanji_chart(ui, deck);
            });
        });
        if start {
            self.start_round(deck);
        }
        ui.add_space(14.0);
    }

    /// Colour of a card by right answers in a row, like 새 단어 (red → green).
    fn card_color(&self, deck: Deck, i: usize) -> egui::Color32 {
        let passes = self.known_map(deck).get(deck.front(i)).copied().unwrap_or(0);
        crate::screens::practice::word_color(passes.min(KANA_DONE) * crate::models::WORD_DONE_PASSES / KANA_DONE)
    }

    /// 50음도 chart coloured like 새 단어 (red → green as right answers add up); click to hear.
    fn kana_chart(&mut self, ui: &mut Ui, deck: Deck) {
        egui::Grid::new(("kana_grid", deck.name())).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, k) in KANA.iter().enumerate() {
                if i == BASIC_ROWS * 5 {
                    ui.end_row();
                }
                if k.0.is_empty() {
                    ui.label("");
                } else {
                    let g = deck.front(i);
                    let text = RichText::new(format!("{g}\n{}", k.2)).size(20.0).color(self.card_color(deck, i));
                    if ui.add(egui::Button::new(text).min_size(egui::vec2(52.0, 52.0))).on_hover_text(KANA[i].3).clicked() {
                        self.voice.say(g);
                    }
                }
                if i % 5 == 4 {
                    ui.end_row();
                }
            }
        });
    }

    /// The level's kanji by grade, coloured by progress; hover for 훈음, click to hear.
    fn kanji_chart(&mut self, ui: &mut Ui, deck: Deck) {
        let Deck::Kanji(g) = deck else { return };
        for n in (g..=5).rev() {
            ui.label(RichText::new(format!("N{n}")).strong());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for i in deck.pool().into_iter().filter(|&i| crate::kanji::all()[i].n == n) {
                    let k = &crate::kanji::all()[i];
                    let text = RichText::new(&k.k).size(22.0).color(self.card_color(deck, i));
                    if ui.add(egui::Button::new(text).min_size(egui::vec2(38.0, 38.0))).on_hover_text(kanji_info(k)).clicked() {
                        self.voice.say(&k.speak());
                    }
                }
            });
            ui.add_space(6.0);
        }
    }

    /// Runs a round of cards when one is open. Returns true while it is shown.
    pub fn kana_quiz_ui(&mut self, ui: &mut Ui) -> bool {
        let Some(mut q) = self.kana_quiz.take() else { return false };
        let deck = q.deck;
        let mut close = false;
        if q.idx >= q.cards.len() {
            let n = q.cards.len();
            page_title(ui, &format!("{} {} 카드 완료", deck.icon(), deck.name()), "");
            card(ui, |ui| {
                ui.label(RichText::new(format!("{n}장 중 {}장 정답", q.right)).size(24.0).strong().color(if q.right == n { GOOD } else { text_strong() }));
                ui.label(RichText::new(format!("{} 기억 완료 {}/{}", deck.name(), self.deck_known(deck), deck.pool().len())).color(muted()));
                if let Deck::Kanji(_) = deck {
                    let today = self.kanji_today();
                    let (c, t) = if today >= KANJI_PER_DAY { (GOOD, format!("✅ 오늘의 필수 한자 카드 {KANJI_PER_DAY}장 완료!")) } else { (warn(), format!("오늘 {today}/{KANJI_PER_DAY}장 — {}장 더 해야 해요", KANJI_PER_DAY - today)) };
                    ui.label(RichText::new(t).color(c));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(format!("↻ {ROUND}장 더")).size(16.0)).fill(ACCENT).min_size(ACTION_BUTTON)).clicked() {
                    q = KanaQuiz::new(round(self.known_map(deck), deck), deck);
                }
                close = ui.button("◀ 새 단어 목록").clicked();
            });
            if !close {
                self.kana_quiz = Some(q);
            }
            return true;
        }
        let answer = q.cards[q.idx];
        let g = deck.front(answer);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {} 카드", deck.icon(), deck.name())).size(20.0).strong());
            ui.label(RichText::new(format!("{} / {}", q.idx + 1, q.cards.len())).color(muted()));
        });
        ui.add(egui::ProgressBar::new(q.idx as f32 / q.cards.len() as f32).desired_height(6.0).fill(ACCENT));
        ui.add_space(4.0);
        // The last right answer, while the next card is already up.
        ui.label(RichText::new(q.last_ok.as_deref().unwrap_or(" ")).size(16.0).color(GOOD));
        ui.add_space(4.0);
        let mut next = false;
        if q.prefetched != Some(q.idx) {
            q.prefetched = Some(q.idx);
            self.voice.speaker.prefetch(&deck.speak(answer), self.voice.base_rate);
        }
        let now = ui.input(|i| i.time);
        if let Some(t) = q.advance_at {
            if now >= t {
                next = true;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(t - now));
            }
        }
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(g).size(120.0).strong().color(text_strong()));
                let ask = if let Deck::Kanji(_) = deck { "이 한자의 훈음·읽는 법·일본어 뜻은?" } else { "이 글자는 어떻게 읽을까요?" };
                ui.label(RichText::new(ask).color(muted()));
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for &c in &q.choices {
                    let label = if let Deck::Kanji(_) = deck { kanji_choice(&crate::kanji::all()[c]) } else { RichText::new(deck.answer(c)).size(20.0).into() };
                    let mut b = egui::Button::new(label).min_size(egui::vec2(if let Deck::Kanji(_) = deck { 220.0 } else { 140.0 }, 52.0));
                    if let Some(p) = q.picked {
                        if c == answer {
                            b = b.fill(GOOD.gamma_multiply(0.45));
                        } else if c == p {
                            b = b.fill(BAD.gamma_multiply(0.45));
                        }
                    }
                    if ui.add_enabled(q.picked.is_none() || c == answer || Some(c) == q.picked, b).clicked() && q.picked.is_none() {
                        q.picked = Some(c);
                        let ok = c == answer;
                        let map = if let Deck::Kanji(_) = deck { &mut self.store.profile.kanji } else { &mut self.store.profile.kana };
                        let st = map.entry(g.to_string()).or_insert(0);
                        *st = if ok { (*st + 1).min(KANA_DONE + 2) } else { 0 };
                        if ok {
                            // Right: straight on to the next card (time saved); wrong: stay and show the answer.
                            q.right += 1;
                            q.advance_at = Some(now + if let Deck::Kanji(_) = deck { KANJI_HOLD } else { KANA_HOLD });
                        }
                        if let Deck::Kanji(_) = deck {
                            self.store.profile.today.kanji_cards += 1;
                        }
                        self.store.save_profile();
                        self.voice.say(&deck.speak(answer));
                    }
                }
            });
            if let Some(p) = q.picked {
                ui.add_space(8.0);
                let short = match deck {
                    Deck::Kanji(_) => crate::kanji::all()[answer].hun_eum(),
                    _ => deck.answer(answer),
                };
                if p == answer {
                    ui.label(RichText::new(format!("✓ 정답! {g} = {short}")).size(18.0).color(GOOD));
                } else {
                    ui.label(RichText::new(format!("✗ 정답은 {short} — 다음 판에 다시 나와요")).size(18.0).color(BAD));
                }
                if let Deck::Kanji(_) = deck {
                    ui.label(RichText::new(kanji_info(&crate::kanji::all()[answer])).size(17.0).color(text_strong()));
                }
                ui.horizontal(|ui| {
                    if ui.button("🔊").on_hover_text("듣기").clicked() {
                        self.voice.say(&deck.speak(answer));
                    }
                    next |= ui.add(egui::Button::new(RichText::new("다음 ▶").size(16.0)).fill(ACCENT).min_size(ACTION_BUTTON)).clicked();
                });
            }
        });
        ui.horizontal(|ui| {
            close = ui.button("그만하기").clicked();
        });
        if next {
            q.advance_at = None;
            q.last_ok = (q.picked == Some(answer)).then(|| match deck {
                Deck::Kanji(_) => {
                    let k = &crate::kanji::all()[answer];
                    format!("✓ 정답! {g} = {} · {} · {}", k.hun_eum(), k.readings(), k.ja)
                }
                _ => format!("✓ 정답! {g} = {}", deck.answer(answer)),
            });
            q.idx += 1;
            q.picked = None;
            if let Some(&a) = q.cards.get(q.idx) {
                q.choices = choices(deck, a);
            }
        }
        if !close {
            self.kana_quiz = Some(q);
        }
        true
    }
}

/// A kanji choice: 훈음 big, then 読み方 and the Japanese meaning smaller.
fn kanji_choice(k: &crate::kanji::Kanji) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    let font = |size: f32| egui::FontId::proportional(size);
    let fmt = |size: f32, color| egui::TextFormat { font_id: font(size), color, ..Default::default() };
    job.append(&k.hun_eum(), 0.0, fmt(20.0, text_strong()));
    job.append(&format!("\n{}", k.readings()), 0.0, fmt(14.0, ACCENT));
    job.append(&format!("\n{}", k.ja), 0.0, fmt(14.0, muted()));
    job.into()
}

/// "날 일\n읽기: 음 ニチ・ジツ  훈 ひ・か\n일본어 뜻: 太陽・ひる・一日\n예: 今日(きょう) 오늘".
fn kanji_info(k: &crate::kanji::Kanji) -> String {
    let mut s = format!("{}\n읽기: {}\n일본어 뜻: {}", k.hun_eum(), k.readings(), k.ja);
    if let [w, r, ko, ..] = k.ex.as_slice() {
        s += &format!("\n예: {w}({r}) {ko}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_has_71_kana_in_rows_of_five() {
        assert_eq!(KANA.len() % 5, 0);
        assert_eq!(all().len(), 71);
        let mut seen = std::collections::HashSet::new();
        for i in all() {
            assert!(seen.insert(KANA[i].0) && seen.insert(KANA[i].1), "{} listed twice", KANA[i].0);
        }
    }

    #[test]
    fn round_learns_in_chart_order_then_reviews() {
        let mut known = std::collections::HashMap::new();
        let mut r = round(&known, Deck::Hiragana);
        r.sort();
        assert_eq!(r, (0..10).collect::<Vec<_>>(), "あ row and か row first");
        for i in all() {
            known.insert(Deck::Hiragana.front(i).to_string(), KANA_DONE);
        }
        known.remove("ぽ");
        let r = round(&known, Deck::Hiragana);
        assert_eq!(r.len(), ROUND);
        assert!(r.contains(&(KANA.len() - 1)), "the one left is in the round");
    }

    #[test]
    fn choices_hold_the_answer_once_and_no_same_sound() {
        for a in all() {
            let c = choices(Deck::Hiragana, a);
            assert_eq!(c.len(), 4);
            assert_eq!(c.iter().filter(|&&x| x == a).count(), 1);
            let labels: std::collections::HashSet<_> = c.iter().map(|&x| KANA[x].3).collect();
            assert_eq!(labels.len(), 4);
        }
    }

    #[test]
    fn kanji_rounds_go_from_n5_and_choices_differ() {
        let all = crate::kanji::all();
        let mut known = std::collections::HashMap::new();
        let deck = Deck::Kanji(4);
        assert!(round(&known, deck).iter().all(|&i| all[i].n == 5), "N5 kanji first");
        for i in crate::kanji::required(4) {
            known.insert(all[i].k.clone(), KANA_DONE);
        }
        assert!(round(&known, deck).iter().all(|&i| all[i].n == 3), "all N4 known: on to N3");
        for i in crate::kanji::required(1) {
            let c = choices(deck, i);
            assert_eq!(c.len(), 4);
            let labels: std::collections::HashSet<_> = c.iter().map(|&x| deck.answer(x)).collect();
            assert_eq!(labels.len(), 4, "{}", all[i].k);
        }
    }
}
