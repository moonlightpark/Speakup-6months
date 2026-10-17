//! 📘 Reading library: a bookshelf of graded stories (fables → children's classics → novels)
//! and a reader where every sentence can be heard, glossed (직독직해), read aloud and saved.

use crate::app::App;
use crate::claude::{SimplifiedPage, WordMeaning};
use crate::library::{self, Book, Page, Source};
use crate::models::*;
use crate::widgets::*;
use egui::{Color32, RichText, Sense, Ui, Vec2};

#[derive(Default)]
pub struct LibraryState {
    pub open: Option<Reader>,
    download: Pending<std::path::PathBuf>,
    downloading: Option<&'static str>,
    pub error: Option<String>,
    show_all: bool,
}

pub struct Reader {
    pub book: &'static Book,
    pub pages: Vec<Page>,
    pub page: usize,
    /// Selected (paragraph, sentence).
    pub selected: Option<(usize, usize)>,
    simplify: Pending<SimplifiedPage>,
    simplified: Option<(usize, SimplifiedPage)>,
    show_simple: bool,
    gloss: Pending<Vec<Chunk>>,
    gloss_text: String,
    word: Pending<WordMeaning>,
    word_result: Option<(String, String, Result<WordMeaning, String>)>,
    pub drill: Option<Drill>,
    error: Option<String>,
}

impl Reader {
    fn new(book: &'static Book, pages: Vec<Page>, page: usize) -> Self {
        Self {
            book,
            page: page.min(pages.len().saturating_sub(1)),
            pages,
            selected: None,
            simplify: Pending::default(),
            simplified: None,
            show_simple: false,
            gloss: Pending::default(),
            gloss_text: String::new(),
            word: Pending::default(),
            word_result: None,
            drill: None,
            error: None,
        }
    }

    /// The paragraphs on screen (the easier rewrite when it's switched on).
    fn paragraphs(&self) -> Vec<String> {
        match &self.simplified {
            Some((p, s)) if self.show_simple && *p == self.page && !s.paragraphs.is_empty() => s.paragraphs.clone(),
            _ => self.pages[self.page].paragraphs.clone(),
        }
    }

    #[cfg(test)]
    pub fn paragraphs_shown(&self) -> Vec<String> {
        self.paragraphs()
    }

    fn reset_page(&mut self) {
        self.selected = None;
        self.drill = None;
        self.word_result = None;
        self.show_simple = false;
    }
}

impl App {
    pub fn library_ui(&mut self, ui: &mut Ui) {
        if let Some(r) = self.library.download.poll() {
            let id = self.library.downloading.take();
            match r {
                Ok(_) => {
                    if let Some(b) = id.and_then(library::book) {
                        self.open_book(b);
                    }
                }
                Err(e) => self.library.error = Some(format!("책 받기 실패: {e:#}")),
            }
        }
        if self.library.open.is_some() {
            self.reader_ui(ui);
        } else {
            self.bookshelf_ui(ui);
        }
    }

    fn open_book(&mut self, b: &'static Book) {
        match library::load_pages(&self.store.library_dir(), b) {
            Ok(pages) if !pages.is_empty() => {
                let prog = self.store.library.entry(b.id.to_string()).or_default();
                prog.pages = pages.len();
                let page = if prog.finished { 0 } else { prog.page };
                self.library.open = Some(Reader::new(b, pages, page));
                self.library.error = None;
            }
            Ok(_) => self.library.error = Some("책 내용을 읽을 수 없습니다.".into()),
            Err(e) => self.library.error = Some(format!("{e:#}")),
        }
    }

    fn bookshelf_ui(&mut self, ui: &mut Ui) {
        page_title(ui, "📘 리딩 라이브러리", "동화책부터 소설까지 — 내 수준에 맞는 책을 소리 내어 읽고, 모르는 문장은 직독직해로 확인하세요.");
        let level = self.store.profile.level;
        let today_read = self.store.profile.today.paragraphs_read;
        ui.horizontal_wrapped(|ui| {
            chip(ui, &format!("내 레벨 {} {}", level_name_of(level), cefr_of(level)), ACCENT);
            chip(ui, &format!("오늘 읽은 문단 {today_read}개 (목표 {READ_GOAL}개)"), if today_read >= READ_GOAL { GOOD } else { MUTED });
            ui.checkbox(&mut self.library.show_all, "모든 책 보기");
        });
        if let Some(e) = &self.library.error {
            error_label(ui, e);
        }
        ui.add_space(6.0);
        let books: Vec<&'static Book> = if self.library.show_all { library::BOOKS.iter().collect() } else { library::recommended(level) };
        let mut open: Option<&'static Book> = None;
        let mut fetch: Option<&'static Book> = None;
        for b in books {
            let prog = self.store.library.get(b.id).cloned().unwrap_or_default();
            let have = library::is_downloaded(&self.store.library_dir(), b);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(54.0, 72.0), Sense::hover());
                    let (a, c) = crate::art::badge_colors(b.id);
                    crate::art::gradient_rect(ui.painter(), rect, 8.0, a, c);
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, Vec2::new(6.0, rect.height())), 3.0, Color32::from_black_alpha(60));
                    let icon = match b.kind {
                        "동화" => "🐰",
                        "어린이 고전" => "🌈",
                        _ => "📖",
                    };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(24.0), Color32::WHITE);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(b.title).size(18.0).strong());
                            chip(ui, b.kind, crate::art::badge_colors(b.kind).0);
                            chip(ui, &format!("{}~{}", cefr_of(b.min_level), cefr_of(b.max_level)), MUTED);
                        });
                        ui.label(RichText::new(format!("{} · {}", b.title_ko, b.author)).color(MUTED));
                        ui.label(RichText::new(b.blurb_ko).color(MUTED).small());
                        if prog.pages > 0 {
                            let frac = if prog.finished { 1.0 } else { prog.page as f32 / prog.pages as f32 };
                            ui.add(egui::ProgressBar::new(frac).desired_width(260.0).desired_height(8.0).fill(GOOD).text(if prog.finished {
                                "다 읽었어요 ✓".to_string()
                            } else {
                                format!("{}/{} 페이지", prog.page + 1, prog.pages)
                            }));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.library.downloading == Some(b.id) {
                            ui.spinner();
                        } else if have {
                            let label = if prog.pages > 0 && !prog.finished { "이어 읽기 ›" } else { "읽기 ›" };
                            if ui.add(egui::Button::new(RichText::new(label).strong()).fill(ACCENT)).clicked() {
                                open = Some(b);
                            }
                        } else if ui.button("⬇ 받기 (원문, 무료)").on_hover_text("Project Gutenberg의 퍼블릭 도메인 원문을 한 번만 받습니다.").clicked() {
                            fetch = Some(b);
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        if let Some(b) = open {
            self.open_book(b);
        }
        if let Some(b) = fetch {
            if let Source::Gutenberg(id) = b.source {
                let dir = self.store.library_dir();
                self.library.downloading = Some(b.id);
                self.library.download.spawn(move || library::download(&dir, id));
            }
        }
        ui.label(RichText::new("원문 책은 저작권이 만료된 퍼블릭 도메인 작품(Project Gutenberg)입니다. 짧은 동화는 학습용으로 다시 쓴 이야기입니다.").color(MUTED).small());
    }

    fn reader_ui(&mut self, ui: &mut Ui) {
        let claude = self.claude();
        let level = self.store.profile.level;
        let pass = self.pass_score();
        let Some(r) = self.library.open.as_mut() else { return };
        // Background results
        if let Some(res) = r.simplify.poll() {
            match res {
                Ok(s) => {
                    r.simplified = Some((r.page, s));
                    r.show_simple = true;
                }
                Err(e) => r.error = Some(format!("{e:#}")),
            }
        }
        if let Some(res) = r.gloss.poll() {
            match res {
                Ok(chunks) => self.store.put_gloss(&r.gloss_text, &chunks),
                Err(e) => r.error = Some(format!("{e:#}")),
            }
        }
        if let Some(res) = r.word.poll() {
            if let Some(w) = r.word_result.as_mut() {
                w.2 = res.map_err(|e| format!("{e:#}"));
            }
        }

        let mut close = false;
        ui.horizontal(|ui| {
            if ui.button("← 책장").clicked() {
                close = true;
            }
            ui.label(RichText::new(r.book.title).size(22.0).strong());
            ui.label(RichText::new(r.book.title_ko).color(MUTED));
        });
        let n = r.pages.len();
        ui.add(egui::ProgressBar::new((r.page + 1) as f32 / n as f32).desired_height(6.0).fill(ACCENT));
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&r.pages[r.page].heading).color(MUTED));
            ui.label(RichText::new(format!("· {}/{n} 페이지", r.page + 1)).color(MUTED));
        });
        if let Some(e) = &r.error {
            error_label(ui, e);
        }
        let paras = r.paragraphs();
        // Toolbar
        ui.horizontal_wrapped(|ui| {
            let whole = paras.join(" ");
            if ui.add(egui::Button::new(RichText::new("▶ 이 페이지 듣기").strong()).fill(ACCENT)).clicked() {
                self.voice.say(&whole);
            }
            if ui.button("🐢 천천히 듣기").clicked() {
                self.voice.say_slow(&whole);
            }
            if ui.button("⏹ 멈춤").clicked() {
                self.voice.speaker.stop();
                self.voice.player.stop();
            }
            if r.simplified.as_ref().is_some_and(|(p, _)| *p == r.page) {
                ui.checkbox(&mut r.show_simple, "✨ 쉬운 영어로 보기");
            } else if r.simplify.running() {
                ui.spinner();
                ui.label(RichText::new("쉬운 영어로 바꾸는 중…").color(MUTED));
            } else if let Some(c) = claude.clone() {
                if ui.button("✨ 쉬운 영어로 다시 쓰기").on_hover_text("Claude가 내 레벨에 맞게 이 페이지를 쉽게 바꿔 줍니다.").clicked() {
                    let p = r.pages[r.page].paragraphs.clone();
                    r.simplify.spawn(move || c.simplify_page(&p, level));
                }
            }
        });
        if let Some((p, s)) = &r.simplified {
            if *p == r.page && r.show_simple && !s.summary_ko.is_empty() {
                ui.label(RichText::new(format!("요약: {}", s.summary_ko)).color(GLOSS));
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new("문장을 누르면 듣기 · 직독직해 · 소리 내어 읽기 · 단어 뜻을 볼 수 있어요.").color(MUTED).small());

        // Page text
        let mut clicked: Option<(usize, usize)> = None;
        let mut event = DrillEvent::None;
        let mut save: Option<String> = None;
        let mut save_word: Option<(String, WordMeaning)> = None;
        egui::Frame::new().fill(Color32::from_rgb(250, 246, 236)).corner_radius(14.0).inner_margin(egui::Margin::symmetric(26, 20)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (pi, para) in paras.iter().enumerate() {
                let sents = crate::neural_tts::sentences(para);
                ui.horizontal_wrapped(|ui| {
                    // Clear gaps between sentences (and lines) for easier reading.
                    ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                    for (si, s) in sents.iter().enumerate() {
                        let sel = r.selected == Some((pi, si));
                        let text = RichText::new(s).size(18.0).color(if sel { Color32::from_rgb(20, 40, 120) } else { Color32::from_rgb(40, 36, 30) });
                        let text = if sel { text.background_color(Color32::from_rgb(255, 226, 140)) } else { text };
                        let resp = ui.add(egui::Label::new(text).sense(Sense::click()));
                        if resp.hovered() && !sel {
                            ui.painter().rect_stroke(resp.rect.expand(1.0), 3.0, egui::Stroke::new(1.0, Color32::from_rgb(200, 180, 120)), egui::StrokeKind::Outside);
                        }
                        if resp.clicked() {
                            clicked = Some((pi, si));
                        }
                    }
                });
                // Tools for the selected sentence, right under its paragraph.
                if let Some((sp, ss)) = r.selected {
                    if sp == pi {
                        let sentence = sents.get(ss).cloned().unwrap_or_default();
                        egui::Frame::new().fill(CARD).corner_radius(12.0).inner_margin(14.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("🔊 듣기").clicked() {
                                    self.voice.say(&sentence);
                                }
                                if ui.button("🐢 천천히").clicked() {
                                    self.voice.say_slow(&sentence);
                                }
                                if ui.button("🎤 소리 내어 읽기").clicked() {
                                    r.drill = Some(Drill::new(&sentence, "", "", 1, false, false).without_autoplay().with_chunks(self.store.gloss_for(&sentence)));
                                }
                                if ui.button("⭐ 문장 저장 (복습)").clicked() {
                                    save = Some(sentence.clone());
                                }
                                let has_gloss = !self.store.gloss_for(&sentence).is_empty();
                                if !has_gloss {
                                    if r.gloss.running() {
                                        ui.spinner();
                                    } else if let Some(c) = claude.clone() {
                                        if ui.button("📖 직독직해").clicked() {
                                            r.gloss_text = sentence.clone();
                                            let t = sentence.clone();
                                            r.gloss.spawn(move || c.chunk_gloss(&t));
                                        }
                                    }
                                }
                            });
                            let chunks = self.store.gloss_for(&sentence);
                            if !chunks.is_empty() {
                                role_legend(ui, &chunks);
                                chunk_view(ui, &chunks, None, 17.0);
                            }
                            // Word lookup
                            if claude.is_some() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("단어 뜻:").color(MUTED).small());
                                    for w in sentence.split_whitespace() {
                                        let clean: String = w.chars().filter(|c| c.is_alphabetic() || *c == '\'' || *c == '-').collect();
                                        if clean.len() < 3 {
                                            continue;
                                        }
                                        if ui.small_button(&clean).clicked() {
                                            if let Some(c) = claude.clone() {
                                                let (cw, cs) = (clean.clone(), sentence.clone());
                                                r.word_result = Some((clean.clone(), sentence.clone(), Err("…".into())));
                                                r.word.spawn(move || c.word_in_context(&cw, &cs));
                                            }
                                        }
                                    }
                                });
                            }
                            if let Some((w, _, res)) = &r.word_result {
                                match res {
                                    Ok(m) => {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new(w).strong().color(GLOSS));
                                            ui.label(&m.meaning_ko);
                                            if !m.note_ko.is_empty() {
                                                ui.label(RichText::new(&m.note_ko).color(MUTED).small());
                                            }
                                            if m.proper_noun || is_proper_noun(w, &m.meaning_ko) {
                                                ui.label(RichText::new("지명·이름 — 단어장에 넣지 않아요").color(MUTED).small());
                                            } else if ui.small_button("＋ 단어장").clicked() {
                                                save_word = Some((w.clone(), m.clone()));
                                            }
                                        });
                                    }
                                    Err(_) if r.word.running() => spinner_label(ui, "뜻 찾는 중…"),
                                    Err(e) => error_label(ui, e),
                                }
                            }
                            if let Some(d) = r.drill.as_mut() {
                                if d.target == sentence {
                                    event = d.ui(ui, &mut self.voice, pass, claude.as_ref());
                                }
                            }
                        });
                    }
                }
                ui.add_space(10.0);
            }
        });

        if let Some(sel) = clicked {
            if r.selected != Some(sel) {
                r.selected = Some(sel);
                r.drill = None;
                r.word_result = None;
                if let Some(s) = crate::neural_tts::sentences(&paras[sel.0]).get(sel.1) {
                    self.voice.say(s);
                }
            }
        }

        // Page navigation
        let mut go: Option<usize> = None;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if r.page > 0 && ui.button("◀ 이전 페이지").clicked() {
                go = Some(r.page - 1);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let last = r.page + 1 >= n;
                if primary_button(ui, if last { "✓ 다 읽었어요" } else { "다음 페이지 ▶" }).clicked() {
                    go = Some(r.page + 1);
                }
            });
        });

        // Apply effects (after the reader borrow ends).
        let book = r.book;
        let page = r.page;
        let page_paras = r.pages[page].paragraphs.len() as u32;
        let target = r.drill.as_ref().map(|d| d.target.clone()).unwrap_or_default();
        if let DrillEvent::Attempt(a) = &event {
            self.record_attempt("library", &target, a);
        }
        if let DrillEvent::Completed = event {
            if let Some(r) = self.library.open.as_mut() {
                r.drill = None;
            }
            self.toast("👏 잘 읽었어요!");
        }
        if let Some(text) = save {
            crate::srs::add_item(&mut self.store.items, StudyItem { kind: ItemKind::Sentence, text, category: "reading".into(), ..Default::default() });
            self.store.save_items();
            self.toast("⭐ 문장을 복습 목록에 저장했어요");
        }
        if let Some((w, m)) = save_word {
            crate::srs::add_item(
                &mut self.store.items,
                // Stored in lower case like lesson words ("The" at a sentence start is still "the").
                StudyItem { kind: ItemKind::Word, text: w.to_lowercase(), meaning_ko: m.meaning_ko, example: m.example, example_ko: m.example_ko, category: "vocab".into(), ..Default::default() },
            );
            self.store.save_items();
            self.toast("📒 단어장에 추가했어요");
        }
        if let Some(next) = go {
            self.voice.speaker.stop();
            self.voice.player.stop();
            let pages = self.library.open.as_ref().map(|r| r.pages.len()).unwrap_or(1);
            if next > page {
                self.store.profile.today.paragraphs_read += page_paras;
            }
            let prog = self.store.library.entry(book.id.to_string()).or_default();
            prog.pages = pages;
            prog.last_read = Some(today());
            if next > page {
                prog.paragraphs_read += page_paras;
            }
            if next >= pages {
                prog.finished = true;
                prog.page = pages - 1;
                close = true;
                self.toast(format!("🎉 『{}』를 다 읽었어요!", book.title_ko));
            } else {
                prog.page = next;
                if let Some(r) = self.library.open.as_mut() {
                    r.page = next;
                    r.reset_page();
                }
            }
            self.store.save_library();
            self.store.save_profile();
            if self.store.profile.today.paragraphs_read >= READ_GOAL {
                self.complete_step("library");
            }
        }
        if close {
            self.voice.speaker.stop();
            self.library.open = None;
        }
    }
}

/// Paragraphs to read for the daily reading step.
pub const READ_GOAL: u32 = 8;
