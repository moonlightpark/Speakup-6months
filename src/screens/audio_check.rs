//! Audio check: microphone permission, device choice, a live input meter, "record 3 seconds
//! and play it back", and a speaker test. Shown in onboarding and settings.

use crate::app::App;
use crate::audio::{self, MicProblem};
use crate::mic_permission::{self, MicPermission};
use crate::widgets::*;
use egui::{Color32, RichText, Ui};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TEST_SECS: f32 = 3.0;

pub struct AudioCheckState {
    /// Latest known permission (the OS answer to a request arrives on another thread).
    pub permission: Arc<Mutex<MicPermission>>,
    checked_at: Option<Instant>,
    /// Microphones and speakers; listing them can take seconds on macOS, so it runs off the UI thread.
    devices: Arc<Mutex<Option<(Vec<String>, Vec<String>)>>>,
    listing: bool,
    testing: bool,
    peak: f32,
    /// The microphone in use is a MacBook's built-in one and the lid is closed.
    builtin_mic_off: Arc<std::sync::atomic::AtomicBool>,
    pub result: Option<Result<String, String>>,
}

impl Default for AudioCheckState {
    fn default() -> Self {
        Self {
            permission: Arc::new(Mutex::new(mic_permission::status())),
            checked_at: None,
            devices: Arc::default(),
            listing: false,
            testing: false,
            peak: 0.0,
            builtin_mic_off: Arc::default(),
            result: None,
        }
    }
}

impl AudioCheckState {
    pub fn permission(&self) -> MicPermission {
        *self.permission.lock().unwrap()
    }

    /// Re-reads the OS permission every few seconds (the user may change it in System Settings).
    /// Runs off the UI thread: device enumeration and `ioreg` take a few milliseconds.
    pub fn refresh(&mut self, chosen_mic: &str) {
        if !cfg!(test) && self.checked_at.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            self.checked_at = Some(Instant::now());
            let (perm, off, mic) = (self.permission.clone(), self.builtin_mic_off.clone(), chosen_mic.to_string());
            std::thread::spawn(move || {
                *perm.lock().unwrap() = mic_permission::status();
                let lid = mic_permission::lid_closed() && is_builtin_mic(&audio::current_input_name(&mic));
                off.store(lid, std::sync::atomic::Ordering::Relaxed);
            });
        }
    }

    fn list_devices(&mut self) {
        if self.listing || cfg!(test) {
            return;
        }
        self.listing = true;
        let out = self.devices.clone();
        std::thread::spawn(move || *out.lock().unwrap() = Some((audio::input_devices(), audio::output_devices())));
    }

    pub fn builtin_mic_off(&self) -> bool {
        self.builtin_mic_off.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Shows the OS permission dialog if the user hasn't been asked yet.
    pub fn request_if_needed(&self) {
        if self.permission() == MicPermission::NotDetermined {
            let p = self.permission.clone();
            mic_permission::request(move |ok| *p.lock().unwrap() = if ok { MicPermission::Granted } else { MicPermission::Denied });
        }
    }
}

/// MacBook built-in microphones ("MacBook Pro 마이크", "MacBook Air Microphone", "Built-in Microphone").
fn is_builtin_mic(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("macbook") || n.contains("built-in") || n.contains("내장")
}

const LID_MSG: &str = "🎤 노트북 덮개가 닫혀 있어 내장 마이크가 꺼져 있습니다. 덮개를 열거나 헤드셋·이어폰 마이크를 연결하세요.";

impl App {
    /// Top-of-window warning when the microphone is blocked by the OS or the closed lid.
    pub fn mic_permission_banner(&mut self, ctx: &egui::Context) {
        self.audio_check.refresh(&self.store.settings.input_device);
        let msg = match self.audio_check.permission() {
            MicPermission::Denied => format!("🎤 마이크 권한이 꺼져 있어 말하기 연습을 할 수 없습니다. {}", mic_permission::how_to_fix()),
            MicPermission::NotDetermined => "🎤 마이크 사용 권한을 허용해 주세요.".to_string(),
            _ if self.audio_check.builtin_mic_off() => LID_MSG.to_string(),
            _ => return,
        };
        egui::TopBottomPanel::top("mic_warn").frame(egui::Frame::new().fill(Color32::from_rgb(70, 46, 8)).inner_margin(egui::Margin::symmetric(16, 8))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(msg).color(Color32::from_rgb(255, 222, 150)).strong());
                if self.audio_check.permission() == MicPermission::NotDetermined {
                    if ui.button("권한 요청").clicked() {
                        self.audio_check.request_if_needed();
                    }
                } else if self.audio_check.permission() == MicPermission::Denied {
                    if ui.button("⚙ 마이크 설정 열기").clicked() {
                        mic_permission::open_settings();
                    }
                } else if ui.button("🎤 오디오 장치 설정").clicked() {
                    self.go(crate::app::Screen::Settings);
                }
            });
        });
    }

    pub fn audio_check_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let st = &mut self.audio_check;
        // Tests never touch real audio devices (enumerating WASAPI devices from test threads crashes on Windows).
        st.list_devices();
        let (inputs, outputs) = st.devices.lock().unwrap().clone().unwrap_or_default();
        if st.listing && st.devices.lock().unwrap().is_none() {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        // Permission
        let (txt, col) = match st.permission() {
            MicPermission::Granted => ("✅ 마이크 권한 허용됨", GOOD),
            MicPermission::NotApplicable => ("● 마이크 권한: 운영체제 설정을 따릅니다", MUTED),
            MicPermission::NotDetermined => ("● 마이크 권한을 아직 묻지 않았습니다", WARN),
            MicPermission::Denied => ("⛔ 마이크 권한이 거부되어 있습니다", BAD),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(txt).color(col).strong());
            match st.permission() {
                MicPermission::NotDetermined => {
                    if ui.button("권한 요청").clicked() {
                        st.request_if_needed();
                    }
                }
                MicPermission::Denied => {
                    if ui.button("⚙ 마이크 설정 열기").clicked() {
                        mic_permission::open_settings();
                    }
                }
                _ => {}
            }
        });
        if st.permission() == MicPermission::Denied {
            ui.label(RichText::new(mic_permission::how_to_fix()).color(MUTED).small());
        }
        if st.builtin_mic_off() {
            ui.label(RichText::new(LID_MSG).color(WARN));
        }
        ui.add_space(4.0);

        // Devices
        let label = |name: &str| if name.is_empty() { "시스템 기본값".to_string() } else { name.to_string() };
        egui::Grid::new("audio_devices").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("🎤 마이크");
            egui::ComboBox::from_id_salt("mic_dev").width(320.0).selected_text(label(&self.store.settings.input_device)).show_ui(ui, |ui| {
                for name in std::iter::once(String::new()).chain(inputs.iter().cloned()) {
                    if ui.selectable_label(self.store.settings.input_device == name, label(&name)).clicked() {
                        self.store.settings.input_device = name.clone();
                        self.voice.recorder.device = name;
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("🔈 스피커");
            egui::ComboBox::from_id_salt("out_dev").width(320.0).selected_text(label(&self.store.settings.output_device)).show_ui(ui, |ui| {
                for name in std::iter::once(String::new()).chain(outputs.iter().cloned()) {
                    if ui.selectable_label(self.store.settings.output_device == name, label(&name)).clicked() {
                        self.store.settings.output_device = name.clone();
                        self.voice.player.set_device(name);
                        changed = true;
                    }
                }
            });
            ui.end_row();
        });
        if audio::is_headset_mic(&self.store.settings.input_device, &outputs) {
            ui.label(RichText::new("⚠ 블루투스 헤드폰의 마이크를 쓰면 헤드폰이 '통화 모드'로 바뀌어 원어민 음성이 전화처럼 뭉개져 들립니다. 소리는 헤드폰으로 듣고, 마이크는 노트북 내장 마이크를 쓰세요.").color(WARN));
            if let Some(m) = audio::non_headset_mic(&inputs, &outputs) {
                if ui.button(format!("🎤 마이크를 '{m}'(으)로 바꾸기")).clicked() {
                    self.store.settings.input_device = m.clone();
                    self.voice.recorder.device = m;
                    changed = true;
                }
            }
        }
        ui.horizontal(|ui| {
            if ui.small_button("↻ 장치 목록 새로고침").clicked() {
                *st.devices.lock().unwrap() = None;
                st.listing = false;
            }
            if st.devices.lock().unwrap().is_none() && !cfg!(test) {
                ui.spinner();
                ui.label(RichText::new("장치 찾는 중…").color(MUTED).small());
            }
        });
        ui.label(RichText::new("소리 내기(원어민 음성)는 운영체제의 기본 출력 장치를 사용합니다. 녹음·유튜브 재생은 위에서 고른 스피커로 나옵니다.").color(MUTED).small());
        ui.add_space(6.0);

        // Live test
        ui.horizontal(|ui| {
            if st.testing {
                let rec = &self.voice.recorder;
                st.peak = st.peak.max(rec.level());
                let t = rec.elapsed();
                ui.add(egui::ProgressBar::new((rec.level() * 3.0).min(1.0)).desired_width(220.0).desired_height(14.0).fill(GOOD).text(format!("녹음 중 {:.1}초", t)));
                ui.ctx().request_repaint();
                let failed = match rec.state() {
                    audio::RecState::Failed(e) => Some(e),
                    _ => None,
                };
                if let Some(e) = failed {
                    st.testing = false;
                    self.voice.recorder.reset();
                    st.result = Some(Err(e));
                } else if t >= TEST_SECS || rec.problem() == Some(MicProblem::NoData) && t > 2.0 {
                    let problem = rec.problem();
                    st.testing = false;
                    let samples = self.voice.recorder.stop();
                    st.result = Some(match problem {
                        Some(p) => Err(p.message(&self.voice.recorder.active_device())),
                        None if st.peak < 0.01 => Err("소리가 거의 들리지 않았어요. 마이크에 가까이 대고 말하거나 다른 마이크를 골라 보세요.".into()),
                        None => {
                            let dev = self.voice.recorder.active_device();
                            self.voice.last_recording = samples.clone();
                            self.voice.player.play_samples(samples, audio::STT_RATE, false);
                            Ok(format!("✅ 마이크 정상 ({dev}, 최대 음량 {:.0}%) — 녹음된 목소리를 재생합니다.", (st.peak * 100.0).min(100.0)))
                        }
                    });
                }
            } else if ui.button("🎤 마이크 테스트 (3초 말하기 → 다시 듣기)").clicked() {
                st.request_if_needed();
                st.result = None;
                st.peak = 0.0;
                st.testing = true;
                self.voice.speaker.stop();
                self.voice.player.stop();
                self.voice.recorder.start();
            }
            if ui.button("🔔 스피커 테스트").clicked() {
                self.voice.player.test_tone();
            }
            if ui.button("💬 원어민 음성 테스트").clicked() {
                self.voice.say("Hello! Can you hear me clearly?");
            }
        });
        if self.voice.player.output_failed() {
            error_label(ui, "출력 장치를 열 수 없습니다. 스피커/헤드폰 연결을 확인하거나 다른 장치를 고르세요.");
        }
        match &st.result {
            Some(Ok(m)) => {
                ui.label(RichText::new(m).color(GOOD));
            }
            Some(Err(e)) => error_label(ui, e),
            None => {}
        }
        changed
    }
}
