//! Audio diagnostics: microphone permission and the peak level of every input device.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
fn permission() -> String {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let s = unsafe { AVCaptureDevice::authorizationStatusForMediaType(AVMediaTypeAudio.unwrap()) };
    format!("{} (0 미결정, 1 제한, 2 거부, 3 허용)", s.0)
}
#[cfg(not(target_os = "macos"))]
fn permission() -> String {
    "n/a".into()
}

fn main() {
    println!("mic permission: {}", permission());
    let host = cpal::default_host();
    for dev in host.input_devices().unwrap() {
        let name = dev.name().unwrap_or_default();
        let Ok(cfg) = dev.default_input_config() else { continue };
        let n = Arc::new(Mutex::new((0usize, 0f32)));
        let n2 = n.clone();
        let Ok(s) = dev.build_input_stream(&cfg.into(), move |d: &[f32], _: &_| {
            let mut g = n2.lock().unwrap();
            g.0 += d.len();
            for v in d {
                g.1 = g.1.max(v.abs());
            }
        }, |e| eprintln!("err {e}"), None) else { continue };
        let _ = s.play();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(s);
        let g = n.lock().unwrap();
        println!("{name}: samples {} peak {:.6}", g.0, g.1);
    }
}
