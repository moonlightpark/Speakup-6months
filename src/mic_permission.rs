//! Microphone privacy permission. On macOS an app without permission still gets an input
//! stream, but every sample is exactly zero — so the app must check and ask explicitly.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MicPermission {
    /// Not asked yet (the OS dialog appears on `request`).
    NotDetermined,
    Denied,
    Granted,
    /// The platform has no app-level microphone permission (Windows desktop apps, Linux).
    NotApplicable,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::MicPermission;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    pub fn status() -> MicPermission {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return MicPermission::NotApplicable };
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
            AVAuthorizationStatus::NotDetermined => MicPermission::NotDetermined,
            AVAuthorizationStatus::Authorized => MicPermission::Granted,
            _ => MicPermission::Denied,
        }
    }

    /// Shows the system permission dialog (only the first time; afterwards the user must change
    /// it in System Settings). `done` runs on a background queue with the answer.
    pub fn request(done: impl Fn(bool) + Send + Sync + 'static) {
        // A bundled app without NSMicrophoneUsageDescription is killed by the OS on request.
        if !usage_description_present() {
            done(false);
            return;
        }
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return };
        let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| done(granted.as_bool()));
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block) };
    }

    /// True for a plain binary (the terminal that launched it owns the permission) or an .app
    /// whose Info.plist explains why it needs the microphone.
    fn usage_description_present() -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let Some(contents) = exe.ancestors().find(|p| p.file_name().is_some_and(|n| n == "Contents")) else { return true };
        std::fs::read_to_string(contents.join("Info.plist")).map(|s| s.contains("NSMicrophoneUsageDescription")).unwrap_or(false)
    }

    pub fn open_settings() {
        let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone").spawn();
    }

    /// A MacBook with its lid closed (e.g. on an external monitor) disconnects the built-in
    /// microphone in hardware: it still "records", but only zeros.
    pub fn lid_closed() -> bool {
        std::process::Command::new("ioreg")
            .args(["-r", "-k", "AppleClamshellState", "-d", "1"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("\"AppleClamshellState\" = Yes"))
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::MicPermission;
    pub fn status() -> MicPermission {
        MicPermission::NotApplicable
    }
    pub fn request(done: impl Fn(bool) + Send + Sync + 'static) {
        done(true)
    }
    pub fn open_settings() {
        let _ = open::that("ms-settings:privacy-microphone");
    }
    pub fn lid_closed() -> bool {
        false
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::MicPermission;
    pub fn status() -> MicPermission {
        MicPermission::NotApplicable
    }
    pub fn request(done: impl Fn(bool) + Send + Sync + 'static) {
        done(true)
    }
    pub fn open_settings() {}
    pub fn lid_closed() -> bool {
        false
    }
}

pub use imp::{lid_closed, open_settings, request, status};

/// Where to fix a denied permission, for messages.
pub fn how_to_fix() -> &'static str {
    if cfg!(target_os = "macos") {
        "시스템 설정 > 개인정보 보호 및 보안 > 마이크 에서 SpeakUp 6M(터미널에서 실행했다면 그 터미널 앱)을 켜고 앱을 다시 실행하세요."
    } else if cfg!(target_os = "windows") {
        "설정 > 개인 정보 > 마이크 에서 '데스크톱 앱이 마이크에 액세스하도록 허용'을 켜세요."
    } else {
        "사운드 설정에서 입력 장치가 음소거되어 있지 않은지 확인하세요."
    }
}
