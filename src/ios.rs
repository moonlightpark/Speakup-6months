//! iOS entry point (iPad trial build): the app's data goes in its sandbox (Library/Application
//! Support), the audio session is set to record and play, and the screen is the desktop layout
//! drawn with wgpu (Metal).

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSString;

pub fn run() -> eframe::Result<()> {
    if let Some(home) = std::env::var_os("HOME") {
        // Before any other thread starts, so nothing reads the environment at the same time.
        unsafe { std::env::set_var("SPEAKUP_DATA_DIR", std::path::Path::new(&home).join("Library/Application Support/SpeakUp")) };
    }
    crate::applog::init(&crate::storage::data_dir());
    std::panic::set_hook(Box::new(|info| crate::applog!("panic: {info}")));
    start_audio_session();
    let options = eframe::NativeOptions { renderer: eframe::Renderer::Wgpu, ..Default::default() };
    eframe::run_native("SpeakUp 6M", options, Box::new(|cc| Ok(Box::new(crate::app::App::new(cc)))))
}

/// iOS gives an app the microphone only in a recording category: PlayAndRecord, sound from the
/// loudspeaker (not the earpiece), Bluetooth headsets allowed.
fn start_audio_session() {
    const DEFAULT_TO_SPEAKER: usize = 0x8;
    const ALLOW_BLUETOOTH: usize = 0x4;
    unsafe {
        let session: Option<Retained<AnyObject>> = msg_send![class!(AVAudioSession), sharedInstance];
        let Some(session) = session else { return };
        // The category constant's value is its own name.
        let category = NSString::from_str("AVAudioSessionCategoryPlayAndRecord");
        let error: *mut *mut AnyObject = std::ptr::null_mut();
        let ok: bool = msg_send![&session, setCategory: &*category, withOptions: DEFAULT_TO_SPEAKER | ALLOW_BLUETOOTH, error: error];
        let active: bool = msg_send![&session, setActive: true, error: error];
        crate::applog!("audio: 오디오 세션 (녹음+재생) {} · 켜기 {}", if ok { "설정" } else { "실패" }, if active { "성공" } else { "실패" });
    }
}
