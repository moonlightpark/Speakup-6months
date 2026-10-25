//! 설정 > 📜 오픈소스 · 출처: the open-source software, models and fonts the app is built
//! with, and the sites its videos, photos and books come from.

use crate::widgets::*;
use egui::{RichText, Ui};

/// (name, licence, what it does here, link).
type Credit = (&'static str, &'static str, &'static str, &'static str);

pub const LIBRARIES: &[Credit] = &[
    ("egui / eframe", "MIT OR Apache-2.0", "앱 화면(UI)", "https://github.com/emilk/egui"),
    ("wry", "Apache-2.0 OR MIT", "앱 안 웹 화면 (리스닝 랩 영상)", "https://github.com/tauri-apps/wry"),
    ("whisper.cpp", "MIT", "음성 인식 엔진", "https://github.com/ggerganov/whisper.cpp"),
    ("whisper-rs", "Unlicense", "whisper.cpp 연결", "https://codeberg.org/tazz4843/whisper-rs"),
    ("lindera", "MIT", "일본어 형태소 분석 (후리가나 · 발음 채점 · 단어 찾기)", "https://github.com/lindera/lindera"),
    ("IPADIC (mecab-ipadic)", "IPADIC 라이선스 (BSD 계열)", "lindera에 들어 있는 일본어 사전", "https://github.com/lindera/lindera"),
    ("VOICEVOX CORE", "MIT", "자연스러운 AI 일본어 음성 엔진", "https://github.com/VOICEVOX/voicevox_core"),
    ("VOICEVOX ONNX Runtime", "VOICEVOX ONNX Runtime 이용약관 (재배포 가능, 출처 표기)", "AI 음성 실행", "https://github.com/VOICEVOX/onnxruntime-builder"),
    ("Open JTalk 사전", "Modified BSD", "AI 음성의 일본어 읽기·억양 분석", "https://github.com/r9y9/open_jtalk"),
    ("tts", "MIT", "일본어 음성 (운영체제 음성, AI 음성이 없을 때)", "https://github.com/ndarilek/tts-rs"),
    ("cpal", "Apache-2.0", "마이크 · 스피커", "https://github.com/rustaudio/cpal"),
    ("rodio", "MIT OR Apache-2.0", "소리 재생", "https://github.com/RustAudio/rodio"),
    ("reqwest", "MIT OR Apache-2.0", "인터넷 연결 (Claude, 사진, 모델 받기)", "https://github.com/seanmonstar/reqwest"),
    ("image", "MIT OR Apache-2.0", "사진 읽기", "https://github.com/image-rs/image"),
    ("serde / serde_json", "MIT OR Apache-2.0", "학습 기록 저장", "https://github.com/serde-rs/serde"),
    ("chrono", "MIT OR Apache-2.0", "날짜 · 학습 일정", "https://github.com/chronotope/chrono"),
    ("rand", "MIT OR Apache-2.0", "문제 섞기", "https://github.com/rust-random/rand"),
    ("directories", "MIT OR Apache-2.0", "데이터 폴더 위치", "https://github.com/soc/directories-rs"),
    ("open", "MIT", "브라우저 · 폴더 열기", "https://github.com/Byron/open-rs"),
    ("zip / encoding_rs", "MIT · MIT OR Apache-2.0", "아오조라 문고 책 풀기 (Shift_JIS)", "https://github.com/zip-rs/zip2"),
    ("anyhow", "MIT OR Apache-2.0", "오류 처리", "https://github.com/dtolnay/anyhow"),
];

pub const MODELS_FONTS: &[Credit] = &[
    ("Whisper 음성 인식 모델 (OpenAI)", "MIT", "내 말을 글자로 (ggml 형식, Hugging Face에서 받기)", "https://huggingface.co/ggerganov/whisper.cpp"),
    ("VOICEVOX Nemo", "VOICEVOX 음성 모델 이용약관 · Nemo 이용약관 (출처 표기)", "AI 일본어 목소리 (여성 1~6 · 남성 1~3)", "https://voicevox.hiroshiba.jp/nemo/"),
    ("Pretendard 글꼴 (길형진)", "SIL OFL 1.1", "한글 · 라틴 글꼴", "https://github.com/orioncactus/pretendard"),
    ("Noto Sans JP 글꼴 (Google · Adobe)", "SIL OFL 1.1", "일본어 (가나 · 한자) 글꼴", "https://github.com/notofonts/noto-cjk"),
    ("Ubuntu · Hack · Noto Emoji 글꼴", "Ubuntu Font Licence · MIT · SIL OFL 1.1", "egui 기본 글꼴 · 이모지", "https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts"),
];

pub const SITES: &[Credit] = &[
    ("YouGlish", "서비스 이용약관", "리스닝 랩 🎬 오늘 문장 영상 · 🔤 오늘 단어 발음 (영상 검색 · 플레이어)", "https://youglish.com"),
    ("YouTube", "각 영상 제작자 소유", "YouGlish가 보여 주는 원본 영상", "https://www.youtube.com"),
    ("Wikimedia Commons", "사진마다 CC BY · CC BY-SA · CC0 등", "받아쓰기 상황 사진 (작가 · 라이선스는 사진 아래 표시)", "https://commons.wikimedia.org"),
    ("kanji-data (David Gouveia)", "MIT", "한자 카드의 JLPT 급수·음독·훈독", "https://github.com/davidluzgouveia/kanji-data"),
    ("Unicode Unihan Database", "Unicode License v3", "한자 카드의 한국어 음 확인", "https://www.unicode.org/charts/unihan.html"),
    ("青空文庫 (아오조라 문고)", "퍼블릭 도메인 (저작권 만료 작품)", "리딩 라이브러리의 책", "https://www.aozora.gr.jp"),
    ("Anthropic Claude API", "유료 서비스 (오픈소스 아님)", "레슨 · 문장 · 피드백 · 사진 검색어 만들기", "https://www.anthropic.com"),
];

/// Ads on the video sites are theirs, not the app's.
pub const AD_NOTICE: &str = "※ 영상(YouGlish · YouTube)에 나오는 광고와 그 광고 수익은 각 사이트와 영상 제작자의 것이며, 이 앱과는 아무 관련이 없습니다. 이 앱은 광고 수익을 받지 않습니다.";

fn section(ui: &mut Ui, title: &str, rows: &[Credit]) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().color(text_strong()));
    egui::Grid::new(title).num_columns(3).spacing([14.0, 4.0]).striped(true).show(ui, |ui| {
        for (name, licence, what, url) in rows {
            ui.hyperlink_to(*name, *url).on_hover_text(*url);
            ui.label(RichText::new(*licence).color(muted()).small());
            ui.label(RichText::new(*what).small());
            ui.end_row();
        }
    });
}

/// The 설정 card.
pub fn credits_ui(ui: &mut Ui) {
    card(ui, |ui| {
        ui.label(RichText::new("📜 오픈소스 · 출처").strong());
        ui.label(RichText::new("이 앱은 아래 오픈소스 소프트웨어와 모델 · 글꼴로 만들어졌고, 영상 · 사진 · 책은 아래 사이트에서 가져옵니다. 이름을 누르면 해당 페이지가 열려요.").color(muted()).small());
        section(ui, "오픈소스 라이브러리", LIBRARIES);
        section(ui, "모델 · 글꼴", MODELS_FONTS);
        section(ui, "영상 · 사진 · 책을 가져오는 사이트", SITES);
        ui.add_space(8.0);
        ui.label(RichText::new(AD_NOTICE).color(warn()).small());
    });
}
