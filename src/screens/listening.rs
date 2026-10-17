//! Listening lab: native YouTube content split into subtitle sentences. Each sentence can be
//! looped (section repeat), slowed down without changing pitch, hidden for blind listening,
//! shadowed (say it right after the speaker, scored), and saved to the review deck.

use crate::app::App;
use crate::audio;
use crate::content;
use crate::models::*;
use crate::srs;
use crate::widgets::*;
use crate::youtube::{self, Video};
use egui::{RichText, Ui};

pub struct ListeningState {
    /// 0 = YouTube shadowing, 1 = listen and respond.
    pub tab: usize,
    pub url: String,
    pub fetch: Pending<Video>,
    pub video: Option<Video>,
    pub library: Option<Vec<Video>>,
    pub error: Option<String>,
    pub selected: usize,
    pub looping: bool,
    pub speed: f32,
    pub blind: bool,
    pub auto_next: bool,
    pub clip: Pending<(Vec<f32>, u32)>,
    pub drill: Option<Drill>,
    playing_seg: Option<usize>,
}

impl Default for ListeningState {
    fn default() -> Self {
        Self {
            tab: 0,
            url: String::new(),
            fetch: Pending::default(),
            video: None,
            library: None,
            error: None,
            selected: 0,
            looping: true,
            speed: 1.0,
            blind: false,
            auto_next: false,
            clip: Pending::default(),
            drill: None,
            playing_seg: None,
        }
    }
}

impl App {
    fn play_segment(&mut self, i: usize) {
        let Some(v) = &self.listening.video else { return };
        let Some(seg) = v.segments.get(i) else { return };
        self.voice.speaker.stop();
        self.voice.player.stop();
        let (path, start, end, speed) = (v.audio.clone(), seg.start - 0.15, seg.end + 0.25, self.listening.speed);
        self.listening.selected = i;
        self.listening.playing_seg = Some(i);
        self.listening.clip.spawn(move || {
            let (samples, rate) = audio::decode_segment(&path, start, end)?;
            Ok((audio::time_stretch(&samples, rate, speed), rate))
        });
    }

    fn select_segment(&mut self, i: usize) {
        self.listening.selected = i;
        self.listening.drill = None;
        self.play_segment(i);
    }

    pub fn listening_ui(&mut self, ui: &mut Ui) {
        if let Some(r) = self.listening.fetch.poll() {
            match r {
                Ok(v) => {
                    self.listening.video = Some(v);
                    self.listening.library = None;
                    self.listening.selected = 0;
                    self.listening.drill = None;
                }
                Err(e) => self.listening.error = Some(format!("{e:#}")),
            }
        }
        if let Some(r) = self.listening.clip.poll() {
            match r {
                Ok((samples, rate)) => {
                    let looping = self.listening.looping;
                    self.voice.player.play_samples(samples, rate, looping);
                }
                Err(e) => self.listening.error = Some(format!("{e:#}")),
            }
        }
        // Auto-advance to the next sentence when not looping.
        if self.listening.auto_next && !self.listening.looping && !self.voice.player.is_playing() && !self.listening.clip.running() {
            if let (Some(i), Some(v)) = (self.listening.playing_seg, &self.listening.video) {
                if i + 1 < v.segments.len() {
                    self.select_segment(i + 1);
                } else {
                    self.listening.playing_seg = None;
                }
            }
        }

        page_title(ui, "🎧 리스닝 랩 · 구간반복", "유튜브 원어민 영상을 자막 문장 단위로 반복해서 듣고, 바로 따라 말하세요(쉐도잉). 오늘 목표: 쉐도잉 5문장 통과.");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.listening.tab, 0, "📺 유튜브 구간반복");
            ui.selectable_value(&mut self.listening.tab, 1, "👂 듣고 대답하기");
        });
        ui.add_space(6.0);
        if self.listening.tab == 1 {
            self.listen_respond_ui(ui);
            return;
        }

        let ytdlp = youtube::find_ytdlp(&self.store.settings.ytdlp_path, &self.store.dir.join("bin"));
        card(ui, |ui| {
            if ytdlp.is_none() {
                ui.label(RichText::new("유튜브 오디오·자막을 받으려면 yt-dlp 가 필요합니다.").color(WARN));
                if self.ytdlp_job.running() {
                    spinner_label(ui, "yt-dlp 설치 중…");
                } else if ui.button("⬇ yt-dlp 자동 설치").clicked() {
                    let bin = self.store.dir.join("bin");
                    self.ytdlp_job.spawn(move || youtube::install_ytdlp(&bin));
                }
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.listening.url).hint_text("유튜브 영상 주소 붙여넣기").desired_width(420.0));
                let can = ytdlp.is_some() && !self.listening.fetch.running();
                if ui.add_enabled(can, egui::Button::new("불러오기")).clicked() {
                    let (yt, url, dir) = (ytdlp.clone().unwrap(), self.listening.url.clone(), self.store.youtube_dir());
                    self.listening.error = None;
                    self.listening.fetch.spawn(move || youtube::fetch(&yt, &url, &dir));
                }
            });
            if self.listening.fetch.running() {
                spinner_label(ui, "오디오와 영어 자막을 받는 중… (영상 길이에 따라 수십 초)");
            }
            if let Some(e) = &self.listening.error {
                error_label(ui, e);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("영상 찾기 (브라우저에서 열림 → 마음에 드는 영상 주소를 복사해 붙여넣기)").color(MUTED).small());
            let queries = self.lesson.as_ref().map(|l| l.listening.search_queries.clone()).unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for q in &queries {
                    if ui.button(format!("🔎 {q}")).clicked() {
                        let _ = open::that(content::youtube_search_url(q));
                    }
                }
                for (name, desc) in content::CHANNELS {
                    if ui.small_button(*name).on_hover_text(*desc).clicked() {
                        let _ = open::that(content::youtube_search_url(name));
                    }
                }
            });
            if let Some(focus) = self.lesson.as_ref().map(|l| l.listening.focus_ko.clone()).filter(|f| !f.is_empty()) {
                ui.label(RichText::new(format!("🎯 {focus}")).color(ACCENT));
            }
        });

        if self.listening.video.is_none() {
            let lib = self.listening.library.get_or_insert_with(|| youtube::library(&self.store.dir.join("youtube"))).clone();
            if !lib.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("받아둔 영상").strong());
                for v in lib {
                    if ui.link(format!("▶ {} ({}문장)", v.title, v.segments.len())).clicked() {
                        self.listening.video = Some(v);
                        self.listening.selected = 0;
                    }
                }
            }
            return;
        }

        let video = self.listening.video.clone().unwrap();
        let pass = self.pass_score();
        let claude = self.claude();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&video.title).size(18.0).strong());
            if ui.small_button("다른 영상").clicked() {
                self.voice.player.stop();
                self.listening.video = None;
                self.listening.drill = None;
            }
        });

        // Player controls for the selected sentence.
        let sel = self.listening.selected.min(video.segments.len().saturating_sub(1));
        let seg = video.segments[sel].clone();
        let mut action: Option<usize> = None;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} / {}", sel + 1, video.segments.len())).color(MUTED));
                ui.label(RichText::new(format!("{:.1}s – {:.1}s", seg.start, seg.end)).color(MUTED).small());
            });
            ui.add_space(4.0);
            if self.listening.blind && self.listening.drill.is_none() {
                ui.label(RichText::new("🙈 자막 숨김 — 먼저 귀로만 들어 보세요").size(20.0).color(MUTED));
            } else {
                ui.label(RichText::new(&seg.text).size(22.0).strong());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(sel > 0, egui::Button::new("⏮")).clicked() {
                    action = Some(sel - 1);
                }
                let playing = self.voice.player.is_playing() || self.listening.clip.running();
                if playing {
                    if ui.button("⏹ 정지").clicked() {
                        self.voice.player.stop();
                        self.listening.playing_seg = None;
                    }
                } else if ui.add(egui::Button::new(RichText::new("▶ 재생").size(16.0)).fill(ACCENT)).clicked() {
                    action = Some(sel);
                }
                if ui.add_enabled(sel + 1 < video.segments.len(), egui::Button::new("⏭")).clicked() {
                    action = Some(sel + 1);
                }
                ui.separator();
                ui.checkbox(&mut self.listening.looping, "🔁 구간반복");
                ui.checkbox(&mut self.listening.auto_next, "이어 듣기");
                ui.checkbox(&mut self.listening.blind, "자막 숨기기");
            });
            ui.horizontal(|ui| {
                ui.label("속도");
                for s in [0.6f32, 0.75, 0.9, 1.0] {
                    if ui.selectable_label((self.listening.speed - s).abs() < 0.01, format!("{s}x")).clicked() {
                        self.listening.speed = s;
                        action = Some(sel);
                    }
                }
                if self.voice.player.is_playing() {
                    let pos = self.voice.player.position();
                    let len = self.voice.player.clip_len.max(0.1);
                    ui.add(egui::ProgressBar::new((pos / len).min(1.0)).desired_width(160.0));
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("💬 따라 말하기 (쉐도잉)").clicked() {
                    // Replay the native audio right before speaking (TTS autoplay would cut it off).
                    self.listening.drill = Some(Drill::new(&seg.text, "", "", 2, false, false).without_autoplay().with_chunks(self.store.gloss_for(&seg.text)));
                    action = Some(sel);
                }
                let id = srs::make_id(ItemKind::Sentence, &seg.text);
                let saved = self.store.items.iter().any(|i| i.id == id);
                if ui.add_enabled(!saved, egui::Button::new(if saved { "✅ 단어장에 있음" } else { "⭐ 이 문장 저장" })).clicked() {
                    srs::add_item(
                        &mut self.store.items,
                        StudyItem { kind: ItemKind::Sentence, text: seg.text.clone(), category: "youtube".into(), ..Default::default() },
                    );
                    self.store.save_items();
                    self.toast("문장을 저장했어요. 복습에 나옵니다.");
                }
            });
        });
        if let Some(i) = action {
            if Some(i) != Some(sel) {
                self.listening.drill = None;
            }
            self.play_segment(i);
        }

        if let Some(drill) = self.listening.drill.as_mut() {
            ui.add_space(6.0);
            let ev = card(ui, |ui| drill.ui(ui, &mut self.voice, pass, claude.as_ref()));
            if let Some((text, chunks)) = drill.fresh_gloss.take() {
                self.store.put_gloss(&text, &chunks);
            }
            match ev {
                DrillEvent::Attempt(a) => {
                    self.record_attempt("listening", &seg.text, &a);
                    if a.score >= pass {
                        self.store.profile.today.listening_passes += 1;
                        self.store.save_profile();
                        if self.store.profile.today.listening_passes >= 5 && !self.step_done("listening") {
                            self.complete_step("listening");
                        }
                    }
                }
                DrillEvent::Completed => {
                    self.listening.drill = None;
                    if sel + 1 < video.segments.len() {
                        self.select_segment(sel + 1);
                    }
                }
                DrillEvent::None => {}
            }
        }

        ui.add_space(8.0);
        ui.label(RichText::new("전체 문장").strong());
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("segs").max_height(260.0).show(ui, |ui| {
            for (i, s) in video.segments.iter().enumerate() {
                let text = if self.listening.blind { format!("{:>3}. ··· ({:.0}s)", i + 1, s.start) } else { format!("{:>3}. {}", i + 1, s.text) };
                if ui.selectable_label(i == sel, text).clicked() {
                    pick = Some(i);
                }
            }
        });
        if let Some(i) = pick {
            self.select_segment(i);
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("오늘 쉐도잉 통과: {}/5", self.store.profile.today.listening_passes)).color(MUTED));
            if ui.small_button("🌐 유튜브에서 열기").clicked() {
                let _ = open::that(format!("{}&t={}s", video.url, seg.start as u32));
            }
        });
    }
}
