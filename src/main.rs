//! Desktop entry point (macOS / Windows): opens the window at the size the layout needs.
//! On iOS the same binary starts the iPad app (ios.rs).

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

#[cfg(target_os = "ios")]
fn main() -> eframe::Result<()> {
    speakup::ios::run()
}

#[cfg(not(target_os = "ios"))]
use speakup::{app, applog, storage};

#[cfg(not(target_os = "ios"))]
fn main() -> eframe::Result<()> {
    applog::init(&storage::data_dir());
    // A release build on Windows has no console, so a panic or a failed start would just close
    // the window: write it to the log, and show it in a message box when the app can't go on.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        applog::write(&format!("panic: {info}"));
        if std::thread::current().name() == Some("main") {
            fatal_box(&format!("SpeakUp 6M JP에 오류가 생겨 종료합니다.\n\n{info}\n\n로그: {}", log_path()));
        }
        default_hook(info);
    }));
    // Exactly sidebar + content column + margins (32 + 24) + scrollbar, so nothing is wasted.
    let width = app::SIDEBAR_WIDTH + app::CONTENT_WIDTH + 32.0 + 24.0 + 12.0;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakUp 6M JP — 일본어 6개월 완성")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon_256.png")).unwrap_or_default())
            .with_inner_size([width, 800.0])
            .with_min_inner_size([width - 160.0, 600.0]),
        // Always open at the fitted size (sidebar + content column, ~1,200 wide). eframe's own
        // persistence is not compiled in: in eframe 0.31 `persist_window: false` only stops saving,
        // and an old saved size (2,186 wide) kept being restored.
        persist_window: false,
        ..Default::default()
    };
    let result = eframe::run_native("SpeakUp 6M JP", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))));
    if let Err(e) = &result {
        applog::write(&format!("창을 열 수 없음: {e}"));
        fatal_box(&format!(
            "SpeakUp 6M JP 창을 열 수 없습니다.\n\n{e}\n\n그래픽 드라이버(OpenGL 2.0 이상)가 필요합니다. 그래픽 드라이버를 업데이트한 뒤 다시 실행해 주세요.\n\n로그: {}",
            log_path()
        ));
    }
    result
}

#[cfg(not(target_os = "ios"))]
fn log_path() -> String {
    storage::data_dir().join("logs").join("speakup.log").display().to_string()
}

/// A message box on Windows (the release build has no console); elsewhere stderr is enough.
#[cfg(target_os = "windows")]
fn fatal_box(msg: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(msg), wide("SpeakUp 6M JP"));
    // MB_OK | MB_ICONERROR
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10) };
}

#[cfg(all(not(target_os = "windows"), not(target_os = "ios")))]
fn fatal_box(msg: &str) {
    eprintln!("{msg}");
}
