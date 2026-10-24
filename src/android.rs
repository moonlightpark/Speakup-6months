//! Android entry point (tablet trial build): the activity loads this library and calls
//! `android_main`. The app's data lives in the activity's internal storage, and the screen is
//! the same desktop layout (a tablet in landscape is about as wide as the desktop window).

use jni::objects::JObject;
use winit::platform::android::activity::AndroidApp;

/// Runs `f` with this thread attached to the Java VM and the activity at hand. A Java exception
/// is cleared (and the call reports `None`) so it never stays pending on the thread.
pub(crate) fn with_activity<R>(f: impl FnOnce(&mut jni::JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let r = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return None;
    }
    r.ok()
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    if let Some(dir) = app.internal_data_path() {
        // Before any other thread starts, so nothing reads the environment at the same time.
        unsafe { std::env::set_var("SPEAKUP_DATA_DIR", dir.join("speakup")) };
    }
    crate::applog::init(&crate::storage::data_dir());
    std::panic::set_hook(Box::new(|info| crate::applog!("panic: {info}")));
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = eframe::run_native("SpeakUp 6M", options, Box::new(|cc| Ok(Box::new(crate::app::App::new(cc))))) {
        crate::applog!("앱을 시작할 수 없음 · {e}");
    }
}
