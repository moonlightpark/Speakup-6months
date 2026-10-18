//! Grammatical-role colours (주어 · 동사 · 목적어 · 보어 · 대명사 …) for 직독직해 chunks and
//! Korean passages. Important roles (subject, verb) are vivid; the less a word matters to the
//! sentence, the closer its colour is to grey.
//!
//! Korean passages are annotated by Claude in small batches in the background and cached in
//! `ko_roles.json`; until then (or offline) they are shown in their normal colour.

use crate::claude::Claude;
use crate::widgets::Pending;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Ui};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Role names Claude may use, most important first: (id, 한국어 이름, colour).
pub const ROLES: &[(&str, &str, Color32)] = &[
    ("subject", "주어", Color32::from_rgb(255, 82, 82)),
    ("verb", "동사", Color32::from_rgb(64, 150, 255)),
    ("object", "목적어", Color32::from_rgb(186, 120, 255)),
    ("complement", "보어", Color32::from_rgb(56, 214, 120)),
    ("pronoun", "대명사", Color32::from_rgb(255, 120, 196)),
    ("noun", "명사", Color32::from_rgb(110, 200, 190)),
    ("determiner", "관사·수", Color32::from_rgb(255, 160, 64)),
    ("modifier", "수식어", Color32::from_rgb(196, 184, 150)),
    ("connector", "전치사·접속사", Color32::from_rgb(140, 148, 164)),
    ("other", "기타", Color32::from_rgb(120, 126, 140)),
];

pub fn role_ids() -> Vec<&'static str> {
    ROLES.iter().map(|r| r.0).collect()
}

/// Colour of a role, `None` for an unknown or missing role.
pub fn color(role: &str) -> Option<Color32> {
    ROLES.iter().find(|r| r.0 == role).map(|r| r.2)
}

/// Prompt text that defines the roles (shared by the 직독직해 and Korean requests).
pub const ROLE_RULE: &str = "role is the grammatical function of that piece in its sentence, one of: \
     subject (the subject, including subject pronouns: I, she, 나는, 그녀가), verb (the predicate verb with its auxiliaries/endings: am, have been working, 이다, 일어난다), \
     object (direct/indirect object, including object pronouns), complement (subject/object complement: a boy in \"I am a boy\", 소년), \
     pronoun (any other pronoun or possessive: my, it, 내, 그것의), noun (any other noun, e.g. inside a prepositional phrase), \
     determiner (articles, numbers, demonstratives before a noun: a, the, this, 한, 그), modifier (adjectives and adverbs), \
     connector (prepositions, conjunctions, particles that only link: in, and, because, 그리고), other (interjections, fillers, punctuation-only pieces).";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct KoSeg {
    pub text: String,
    pub role: String,
}

/// True for text worth annotating: a Korean phrase or sentence (single words have no structure).
fn wants_roles(text: &str) -> bool {
    text.split_whitespace().nth(1).is_some() && text.chars().any(|c| ('가'..='힣').contains(&c))
}

/// Segments are only used when they spell the text exactly (otherwise colours would misalign).
pub fn segments_match(text: &str, segs: &[KoSeg]) -> bool {
    !segs.is_empty() && segs.iter().map(|s| s.text.as_str()).collect::<String>() == text
}

// ---------------------------------------------------------------- annotation cache

#[derive(Default)]
struct State {
    /// Annotated passages; an empty list means "tried, show plainly".
    cache: HashMap<String, Vec<KoSeg>>,
    order: VecDeque<String>,
    wanted: Vec<String>,
    batch: Vec<String>,
    /// Failed this session (network/API); retried next launch.
    failed: HashSet<String>,
    job: Pending<Vec<Vec<KoSeg>>>,
    path: Option<PathBuf>,
    /// Settings switch; off = Korean passages in their normal colour and no requests.
    off: bool,
}

impl State {
    const MAX: usize = 4000;
    const BATCH: usize = 20;

    fn insert(&mut self, text: String, segs: Vec<KoSeg>) {
        if self.cache.insert(text.clone(), segs).is_none() {
            self.order.push_back(text);
        }
        while self.order.len() > Self::MAX {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            let list: Vec<(&String, &Vec<KoSeg>)> = self.order.iter().filter_map(|t| self.cache.get_key_value(t)).collect();
            if let Ok(json) = serde_json::to_string(&list) {
                let _ = std::fs::write(p, json);
            }
        }
    }
}

type Shared = Arc<Mutex<State>>;

fn state(ctx: &egui::Context) -> Shared {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Shared>(egui::Id::new("grammar_ko_roles")).clone())
}

/// Loads cached annotations from the data folder.
pub fn load(ctx: &egui::Context, dir: &Path) {
    let st = state(ctx);
    let mut s = st.lock().unwrap();
    let path = dir.join("ko_roles.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let list: Vec<(String, Vec<KoSeg>)> = serde_json::from_str(&raw).unwrap_or_default();
        for (t, segs) in list {
            s.insert(t, segs);
        }
    }
    s.path = Some(path);
}

/// Called once per frame: collects finished annotations and sends the next batch.
/// `enabled` is the settings switch for colouring Korean passages.
pub fn pump(ctx: &egui::Context, claude: Option<Claude>, enabled: bool) {
    let st = state(ctx);
    let mut s = st.lock().unwrap();
    s.off = !enabled;
    if let Some(res) = s.job.poll() {
        let batch = std::mem::take(&mut s.batch);
        match res {
            Ok(results) => {
                let mut results = results.into_iter();
                for text in batch {
                    let segs = results.next().unwrap_or_default();
                    let segs = if segments_match(&text, &segs) { segs } else { Vec::new() };
                    s.insert(text, segs);
                }
                s.save();
            }
            Err(e) => {
                crate::applog!("ko roles: {e}");
                s.failed.extend(batch);
            }
        }
        ctx.request_repaint();
    }
    if s.off {
        s.wanted.clear();
    }
    if s.job.running() || s.wanted.is_empty() {
        return;
    }
    let Some(c) = claude else {
        s.wanted.clear();
        return;
    };
    let n = s.wanted.len().min(State::BATCH);
    let batch: Vec<String> = s.wanted.drain(..n).collect();
    s.batch = batch.clone();
    s.job.spawn(move || c.ko_roles(&batch));
}

/// Korean text coloured by grammatical role once annotated (plain `fallback` colour until then).
pub fn ko_label(ui: &mut Ui, text: &str, size: f32, fallback: Color32) -> egui::Response {
    let segs = {
        let st = state(ui.ctx());
        let mut s = st.lock().unwrap();
        match s.cache.get(text).filter(|_| !s.off) {
            Some(segs) => segs.clone(),
            None => {
                if !s.off && wants_roles(text) && !s.failed.contains(text) && !s.wanted.iter().chain(&s.batch).any(|t| t == text) {
                    s.wanted.push(text.to_string());
                }
                Vec::new()
            }
        }
    };
    if segs.is_empty() {
        return ui.label(egui::RichText::new(text).size(size).color(fallback));
    }
    let mut job = LayoutJob::default();
    for seg in &segs {
        let format = TextFormat { font_id: FontId::proportional(size), color: color(&seg.role).unwrap_or(fallback), ..Default::default() };
        job.append(&seg.text, 0.0, format);
    }
    ui.label(job).on_hover_ui(legend)
}

/// Colour key: "■ 주어 ■ 동사 …".
pub fn legend(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (_, name, c) in ROLES.iter().filter(|r| r.0 != "other") {
            ui.label(egui::RichText::new(format!("■ {name}")).small().color(*c));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importance_fades_to_grey() {
        // Saturation (max - min channel) falls from subject/verb to connector.
        let sat = |r: &str| {
            let c = color(r).unwrap();
            let v = [c.r(), c.g(), c.b()];
            v.iter().max().unwrap() - v.iter().min().unwrap()
        };
        assert!(sat("subject") > sat("modifier") && sat("verb") > sat("modifier"));
        assert!(sat("modifier") > sat("connector"));
        assert!(color("nope").is_none());
    }

    #[test]
    fn only_exact_segments_are_used() {
        let seg = |t: &str, r: &str| KoSeg { text: t.into(), role: r.into() };
        let segs = [seg("나는", "subject"), seg(" ", "other"), seg("소년이다.", "verb")];
        assert!(segments_match("나는 소년이다.", &segs));
        assert!(!segments_match("나는 소년이다", &segs));
        assert!(!segments_match("나는", &[]));
        assert!(wants_roles("나는 소년이다.") && !wants_roles("일정") && !wants_roles("I am a boy."));
    }
}
