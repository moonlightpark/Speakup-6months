//! 히라가나·가타카나 카드: N5 learners memorise the kana (50음 + 탁음·반탁음) first, at the top
//! of 새 단어. A card shows one kana; the learner picks its sound from four. Three right answers
//! in a row = 기억 완료 (counts in `Profile::kana`, by the kana itself).

use crate::app::App;
use crate::widgets::*;
use egui::{RichText, Ui};
use rand::seq::{IndexedRandom, SliceRandom};

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

/// Right answers in a row that make a kana 기억 완료.
pub const KANA_DONE: u32 = 3;
/// Cards per round.
pub const ROUND: usize = 10;

/// Rows of KANA that are 탁음·반탁음 (shown under the 50음 in the chart).
const BASIC_ROWS: usize = 11;

/// The kana shown on a card (hiragana or katakana).
fn glyph(i: usize, kata: bool) -> &'static str {
    if kata { KANA[i].1 } else { KANA[i].0 }
}

/// Answer label: "아 · a".
pub fn sound(i: usize) -> String {
    format!("{} · {}", KANA[i].3, KANA[i].2)
}

/// Indexes of the real kana (no empty cells).
fn all() -> Vec<usize> {
    (0..KANA.len()).filter(|&i| !KANA[i].0.is_empty()).collect()
}

/// The next round: kana not memorised yet in chart order (so learning goes row by row), filled
/// up with memorised ones for review, shuffled.
pub fn round(known: &std::collections::HashMap<String, u32>, kata: bool) -> Vec<usize> {
    let passes = |i: usize| known.get(glyph(i, kata)).copied().unwrap_or(0);
    let mut pick: Vec<usize> = all().into_iter().filter(|&i| passes(i) < KANA_DONE).take(ROUND).collect();
    let mut done: Vec<usize> = all().into_iter().filter(|&i| passes(i) >= KANA_DONE).collect();
    done.shuffle(&mut rand::rng());
    pick.extend(done.into_iter().take(ROUND - pick.len()));
    pick.shuffle(&mut rand::rng());
    pick
}

/// Four choices with the answer among them; never two with the same sound label.
fn choices(answer: usize) -> Vec<usize> {
    let others: Vec<usize> = all().into_iter().filter(|&i| i != answer && KANA[i].3 != KANA[answer].3).collect();
    let mut c: Vec<usize> = others.choose_multiple(&mut rand::rng(), 3).copied().collect();
    c.push(answer);
    c.shuffle(&mut rand::rng());
    c
}

/// One round of kana cards.
pub struct KanaQuiz {
    pub kata: bool,
    pub cards: Vec<usize>,
    pub idx: usize,
    pub choices: Vec<usize>,
    pub picked: Option<usize>,
    pub right: usize,
}

impl KanaQuiz {
    pub fn new(cards: Vec<usize>, kata: bool) -> Self {
        let choices = cards.first().map(|&a| choices(a)).unwrap_or_default();
        Self { kata, cards, idx: 0, choices, picked: None, right: 0 }
    }
    fn name(&self) -> &'static str {
        if self.kata { "가타카나" } else { "히라가나" }
    }
}

impl App {
    fn kana_known(&self, kata: bool) -> usize {
        all().into_iter().filter(|&i| self.store.profile.kana.get(glyph(i, kata)).copied().unwrap_or(0) >= KANA_DONE).count()
    }

    /// N5 (초급) learners get the kana cards first in 새 단어.
    pub fn kana_cards_shown(&self) -> bool {
        self.store.profile.level < crate::models::LEVEL_BANDS[0].0
    }

    /// 🔤 card at the top of 새 단어: progress per script, start buttons and the 50음도 chart.
    pub fn kana_card(&mut self, ui: &mut Ui) {
        let total = all().len();
        let (hira, kata) = (self.kana_known(false), self.kana_known(true));
        let mut start = None;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🔤 히라가나·가타카나 카드").size(18.0).strong());
                chip(ui, "N5 · 글자부터", crate::art::VIOLET);
            });
            ui.label(RichText::new(format!("단어보다 먼저 글자를 외워요. 카드의 글자를 보고 소리를 4개 중에서 고르세요. {KANA_DONE}번 연속 맞히면 기억 완료!")).color(muted()));
            ui.add_space(6.0);
            for (kata, known) in [(false, hira), (true, kata)] {
                let name = if kata { "가타카나" } else { "히라가나" };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{name} {known}/{total}")).strong().color(if known >= total { GOOD } else { text_strong() }));
                    ui.add(egui::ProgressBar::new(known as f32 / total as f32).desired_width(160.0).desired_height(8.0).fill(if known >= total { GOOD } else { ACCENT }));
                    let label = if known >= total { format!("↻ {name} 복습") } else { format!("▶ {name} 카드 {ROUND}장") };
                    let fill = if known >= total || (kata && hira < total) { subtle_fill() } else { ACCENT };
                    if ui.add(egui::Button::new(RichText::new(label).size(15.0)).fill(fill).min_size(ACTION_BUTTON)).clicked() {
                        start = Some(kata);
                    }
                });
            }
            ui.add_space(4.0);
            egui::CollapsingHeader::new("50음도 표 보기 (글자를 누르면 소리가 나요)").id_salt("kana_chart").show(ui, |ui| {
                for kata in [false, true] {
                    ui.label(RichText::new(if kata { "가타카나" } else { "히라가나" }).strong());
                    self.kana_chart(ui, kata);
                    ui.add_space(8.0);
                }
            });
        });
        if let Some(kata) = start {
            crate::applog!("words: {} 카드 시작", if kata { "가타카나" } else { "히라가나" });
            self.kana_quiz = Some(KanaQuiz::new(round(&self.store.profile.kana, kata), kata));
        }
        ui.add_space(14.0);
    }

    /// 50음도 chart coloured like 새 단어 (red → green as right answers add up); click to hear.
    fn kana_chart(&mut self, ui: &mut Ui, kata: bool) {
        egui::Grid::new(("kana_grid", kata)).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, k) in KANA.iter().enumerate() {
                if i == BASIC_ROWS * 5 {
                    ui.end_row();
                }
                if k.0.is_empty() {
                    ui.label("");
                } else {
                    let g = glyph(i, kata);
                    let passes = self.store.profile.kana.get(g).copied().unwrap_or(0);
                    let color = crate::screens::practice::word_color(passes * crate::models::WORD_DONE_PASSES / KANA_DONE);
                    let text = RichText::new(format!("{g}\n{}", k.2)).size(20.0).color(color);
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

    /// Runs a round of kana cards when one is open. Returns true while it is shown.
    pub fn kana_quiz_ui(&mut self, ui: &mut Ui) -> bool {
        let Some(mut q) = self.kana_quiz.take() else { return false };
        let mut close = false;
        if q.idx >= q.cards.len() {
            let n = q.cards.len();
            page_title(ui, &format!("🔤 {} 카드 완료", q.name()), "");
            card(ui, |ui| {
                ui.label(RichText::new(format!("{n}장 중 {}장 정답", q.right)).size(24.0).strong().color(if q.right == n { GOOD } else { text_strong() }));
                ui.label(RichText::new(format!("{} 기억 완료 {}/{}", q.name(), self.kana_known(q.kata), all().len())).color(muted()));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(format!("↻ {ROUND}장 더")).size(16.0)).fill(ACCENT).min_size(ACTION_BUTTON)).clicked() {
                    q = KanaQuiz::new(round(&self.store.profile.kana, q.kata), q.kata);
                }
                close = ui.button("◀ 새 단어 목록").clicked();
            });
            if !close {
                self.kana_quiz = Some(q);
            }
            return true;
        }
        let answer = q.cards[q.idx];
        let g = glyph(answer, q.kata);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("🔤 {} 카드", q.name())).size(20.0).strong());
            ui.label(RichText::new(format!("{} / {}", q.idx + 1, q.cards.len())).color(muted()));
        });
        ui.add(egui::ProgressBar::new(q.idx as f32 / q.cards.len() as f32).desired_height(6.0).fill(ACCENT));
        ui.add_space(10.0);
        let mut next = false;
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(g).size(120.0).strong().color(text_strong()));
                ui.label(RichText::new("이 글자는 어떻게 읽을까요?").color(muted()));
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for &c in &q.choices {
                    let mut b = egui::Button::new(RichText::new(sound(c)).size(20.0)).min_size(egui::vec2(140.0, 52.0));
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
                        let st = self.store.profile.kana.entry(g.to_string()).or_insert(0);
                        *st = if ok { (*st + 1).min(KANA_DONE + 2) } else { 0 };
                        if ok {
                            q.right += 1;
                        }
                        self.store.save_profile();
                        self.voice.say(g);
                    }
                }
            });
            if let Some(p) = q.picked {
                ui.add_space(8.0);
                if p == answer {
                    ui.label(RichText::new(format!("✓ 정답! {g} = {}", sound(answer))).size(18.0).color(GOOD));
                } else {
                    ui.label(RichText::new(format!("✗ 정답은 {} — 다음 판에 다시 나와요", sound(answer))).size(18.0).color(BAD));
                }
                ui.horizontal(|ui| {
                    speaker_button(ui, &mut self.voice, g);
                    next = ui.add(egui::Button::new(RichText::new("다음 ▶").size(16.0)).fill(ACCENT).min_size(ACTION_BUTTON)).clicked();
                });
            }
        });
        ui.horizontal(|ui| {
            close = ui.button("그만하기").clicked();
        });
        if next {
            q.idx += 1;
            q.picked = None;
            if let Some(&a) = q.cards.get(q.idx) {
                q.choices = choices(a);
            }
        }
        if !close {
            self.kana_quiz = Some(q);
        }
        true
    }
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
        let mut r = round(&known, false);
        r.sort();
        assert_eq!(r, (0..10).collect::<Vec<_>>(), "あ row and か row first");
        for i in all() {
            known.insert(glyph(i, false).to_string(), KANA_DONE);
        }
        known.remove("ぽ");
        let r = round(&known, false);
        assert_eq!(r.len(), ROUND);
        assert!(r.contains(&(KANA.len() - 1)), "the one left is in the round");
    }

    #[test]
    fn choices_hold_the_answer_once_and_no_same_sound() {
        for a in all() {
            let c = choices(a);
            assert_eq!(c.len(), 4);
            assert_eq!(c.iter().filter(|&&x| x == a).count(), 1);
            let labels: std::collections::HashSet<_> = c.iter().map(|&x| KANA[x].3).collect();
            assert_eq!(labels.len(), 4);
        }
    }
}
