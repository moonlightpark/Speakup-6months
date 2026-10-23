//! Microphone capture (cpal) and playback (rodio), each on its own thread: device lists and
//! mic diagnostics, learner recordings (resampled to 16 kHz for speech recognition), and
//! playback of recordings and the neural voice.

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const STT_RATE: u32 = 16_000;

/// Scripted speech for automated tests: (what the learner "says", seconds of speech).
/// The simulated recorder produces audio of that length; the scripted recognizer returns the text.
pub type Script = Arc<Mutex<std::collections::VecDeque<(String, f32)>>>;

// ---------------------------------------------------------------- devices

/// Names of the available microphones.
pub fn input_devices() -> Vec<String> {
    cpal::default_host().input_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

/// Names of the available speakers / headphones.
pub fn output_devices() -> Vec<String> {
    cpal::default_host().output_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

/// Name of the microphone a recording would use right now.
/// True when `mic` is the microphone of a wireless headset that is also a speaker (e.g.
/// "WH-1000XM3", AirPods, "Headset (… Hands-Free)"). Recording from it switches Bluetooth into
/// telephone mode, which makes every sound — including the native voice — muffled.
pub fn is_headset_mic(mic: &str, outputs: &[String]) -> bool {
    if mic.is_empty() {
        return false;
    }
    if is_builtin_mic(mic) {
        return false;
    }
    let l = mic.to_lowercase();
    l.contains("hands-free") || l.contains("airpods") || outputs.iter().any(|o| o == mic)
}

/// The laptop's own microphone ("MacBook Pro 마이크", "Microphone Array (Realtek…)").
pub fn is_builtin_mic(name: &str) -> bool {
    let l = name.to_lowercase();
    ["macbook", "built-in", "내장", "microphone array"].iter().any(|b| l.contains(b))
}

/// A microphone that is not part of a headset (the laptop's own mic first), if any.
pub fn non_headset_mic(inputs: &[String], outputs: &[String]) -> Option<String> {
    inputs.iter().find(|n| is_builtin_mic(n)).or_else(|| inputs.iter().find(|n| !is_headset_mic(n, outputs))).cloned()
}

/// Name of the microphone a recording would use. A chosen mic is taken by name; only the system
/// default is asked for. Listing every device takes ~0.3 s on macOS and loads coreaudiod.
pub fn current_input_name(preferred: &str) -> String {
    if !preferred.is_empty() {
        return preferred.to_string();
    }
    cpal::default_host().default_input_device().and_then(|d| d.name().ok()).unwrap_or_default()
}

/// The device called `name`, or the system default when `name` is empty or not connected.
fn pick_device(name: &str, input: bool) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let default = || if input { host.default_input_device() } else { host.default_output_device() };
    // Usually the chosen device is also the system default: skip listing every device then.
    if let Some(d) = default().filter(|d| name.is_empty() || d.name().is_ok_and(|n| n == name)) {
        return Some(d);
    }
    if !name.is_empty() {
        let found = if input { host.input_devices().ok() } else { host.output_devices().ok() }
            .and_then(|mut it| it.find(|d| d.name().is_ok_and(|n| n == name)));
        if found.is_some() {
            return found;
        }
    }
    default()
}

// ---------------------------------------------------------------- recording

#[derive(Clone, Debug, PartialEq)]
pub enum RecState {
    Idle,
    Starting,
    Recording,
    Failed(String),
}

/// Why a recording that is running produces nothing usable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MicProblem {
    /// The device delivers no audio at all.
    NoData,
    /// Every sample is exactly zero: the OS is blocking the microphone (privacy permission).
    Blocked,
}

impl MicProblem {
    /// What to tell the learner; `device` is the microphone actually in use.
    pub fn message(self, device: &str) -> String {
        match self {
            MicProblem::NoData => "마이크에서 소리가 들어오지 않습니다. 설정 > 오디오 장치에서 다른 마이크를 골라 보세요.".into(),
            // A separate (e.g. wireless) mic: the lid and the laptop's privacy switch don't matter.
            MicProblem::Blocked if !device.is_empty() && !is_builtin_mic(device) => format!(
                "선택한 마이크 '{device}'에서 소리가 들어오지 않습니다(입력 신호 0). 무선 마이크라면 송신기가 켜져 있고 수신기와 연결됐는지, 음소거가 아닌지 확인한 뒤 다시 녹음해 보세요."
            ),
            MicProblem::Blocked if crate::mic_permission::lid_closed() => {
                "노트북 덮개가 닫혀 있어 내장 마이크가 꺼져 있습니다(입력 신호 0). 덮개를 열거나, 헤드셋·이어폰 마이크를 연결한 뒤 설정 > 오디오 장치에서 그 마이크를 고르세요.".into()
            }
            MicProblem::Blocked => format!("마이크 소리가 차단되어 있습니다(입력 신호 0). 다른 마이크를 고르거나 권한을 확인하세요. {}", crate::mic_permission::how_to_fix()),
        }
    }
}

/// Adds one input sample, averaging `step` input samples into each stored one (box-filter
/// decimation to 16 kHz); `step <= 1` stores samples as they are.
fn decimate_push(out: &mut Vec<f32>, dec: &mut (f32, f32, f64), v: f32, step: f64) {
    if step <= 1.0 {
        out.push(v);
        return;
    }
    dec.0 += v;
    dec.1 += 1.0;
    dec.2 += 1.0;
    if dec.2 >= step {
        out.push(dec.0 / dec.1);
        *dec = (0.0, 0.0, dec.2 - step);
    }
}

/// A stream that has delivered only zeros this long is reopened (up to twice).
const SILENT_REOPEN: Duration = Duration::from_millis(700);
/// Seconds of all-zero input before the learner is told the mic is blocked.
const BLOCKED_AFTER: f32 = 2.5;

/// Longest recording kept (4/3/2 speaking needs 4 minutes); later samples are dropped.
const MAX_REC_SECS: usize = 260;

enum RecCmd {
    Start(String),
    Stop,
}

/// Shared between the UI and the audio thread.
#[derive(Default)]
struct RecShared {
    /// Mono samples, already reduced to 16 kHz when the mic runs faster (6× less memory at 96 kHz).
    buf: Mutex<Vec<f32>>,
    /// Box-filter decimator state: (sum, count, position in the current output sample).
    dec: Mutex<(f32, f32, f64)>,
    level: Mutex<f32>,
    rate: AtomicU32,
    callbacks: AtomicUsize,
    /// Any sample that isn't exactly zero (real microphones always have a little noise).
    signal: AtomicBool,
    device: Mutex<String>,
}

/// Microphone recorder. The cpal stream lives on its own thread so a slow or stuck audio
/// device can never freeze the UI.
pub struct Recorder {
    tx: std::sync::mpsc::Sender<RecCmd>,
    shared: Arc<RecShared>,
    state: Arc<Mutex<RecState>>,
    started: Option<Instant>,
    /// Microphone name; empty = system default.
    pub device: String,
    sim: Option<Script>,
}

impl Recorder {
    /// A recorder that never touches the audio device (automated tests).
    #[cfg(test)]
    pub fn simulated(script: Script) -> Self {
        let mut r = Self::new(String::new());
        r.sim = Some(script);
        r
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Recorder {
    pub fn new(device: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let r = Self {
            tx,
            shared: Arc::new(RecShared { rate: AtomicU32::new(STT_RATE), ..Default::default() }),
            state: Arc::new(Mutex::new(RecState::Idle)),
            started: None,
            device,
            sim: None,
        };
        let (shared, state) = (r.shared.clone(), r.state.clone());
        std::thread::spawn(move || {
            let mut stream: Option<cpal::Stream> = None;
            // (device name, when the stream opened, reopens so far) for the silent-stream check.
            let mut current: Option<(String, Instant, u32)> = None;
            loop {
                let cmd = match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(c) => c,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // A wireless mic woken from sleep (e.g. DJI Mic 2) can open a stream that
                        // only ever delivers zeros, while opening it again right away works.
                        if let Some((name, opened, reopens)) = &mut current {
                            let silent = shared.callbacks.load(Ordering::Relaxed) > 0 && !shared.signal.load(Ordering::Relaxed);
                            if stream.is_some() && *state.lock().unwrap() == RecState::Recording && silent && opened.elapsed() > SILENT_REOPEN && *reopens < 2 {
                                *reopens += 1;
                                crate::applog!("mic: 입력 신호 0 → 다시 열기 ({}번째) · {name}", reopens);
                                close_input(&mut stream);
                                shared.buf.lock().unwrap().clear();
                                *shared.dec.lock().unwrap() = (0.0, 0.0, 0.0);
                                match open_input(&shared, &state, name) {
                                    Ok(s) => stream = Some(s),
                                    Err(e) => crate::applog!("mic: 다시 열기 실패 · {e:#}"),
                                }
                                *opened = Instant::now();
                            }
                        }
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };
                match cmd {
                    RecCmd::Start(name) => {
                        close_input(&mut stream);
                        current = Some((name.clone(), Instant::now(), 0));
                        match open_input(&shared, &state, &name) {
                            Ok(s) => {
                                stream = Some(s);
                                crate::applog!("mic: 녹음 시작 · {} · {} Hz", shared.device.lock().unwrap(), shared.rate.load(Ordering::Relaxed));
                                let mut st = state.lock().unwrap();
                                // A Stop may already have arrived while the device was opening.
                                if *st == RecState::Starting {
                                    *st = RecState::Recording;
                                }
                            }
                            Err(e) => {
                                crate::applog!("mic: 열기 실패 · {e:#}");
                                *state.lock().unwrap() = RecState::Failed(format!("{e:#}"));
                            }
                        }
                    }
                    RecCmd::Stop => {
                        current = None;
                        if stream.is_some() {
                            crate::applog!("mic: 녹음 정지 · 콜백 {}회", shared.callbacks.load(Ordering::Relaxed));
                        }
                        close_input(&mut stream);
                    }
                }
            }
            close_input(&mut stream);
        });
        r
    }
}

/// Stops the mic stream before dropping it. On macOS, cpal 0.15.3 keeps a clone of the stream
/// inside its own device-disconnect listener, so dropping alone never stops it: every old
/// recording kept running and mixing into the next one (3rd try → unrecognizable audio).
fn close_input(stream: &mut Option<cpal::Stream>) {
    if let Some(s) = stream.take() {
        let _ = s.pause();
    }
}

fn open_input(shared: &Arc<RecShared>, state: &Arc<Mutex<RecState>>, name: &str) -> Result<cpal::Stream> {
    let device = pick_device(name, true).ok_or_else(|| anyhow!("마이크를 찾을 수 없습니다. 마이크가 연결되어 있는지 확인하세요."))?;
    *shared.device.lock().unwrap() = device.name().unwrap_or_default();
    let config = device.default_input_config().context("마이크 설정을 읽을 수 없습니다")?;
    let channels = config.channels() as usize;
    shared.rate.store(config.sample_rate().0, Ordering::Relaxed);
    let st = state.clone();
    let err_fn = move |e: cpal::StreamError| {
        crate::applog!("mic: 스트림 오류 · {e}");
        let mut s = st.lock().unwrap();
        if *s == RecState::Recording {
            *s = RecState::Failed(format!("마이크 연결이 끊겼습니다: {e}"));
        }
    };

    fn push<T: Copy>(data: &[T], ch: usize, conv: impl Fn(T) -> f32, sh: &RecShared) {
        sh.callbacks.fetch_add(1, Ordering::Relaxed);
        let mut b = sh.buf.lock().unwrap();
        let rate = sh.rate.load(Ordering::Relaxed);
        // Faster mics are averaged down to 16 kHz as samples arrive; slower ones are kept as is.
        let step = rate as f64 / STT_RATE as f64;
        let cap = rate.min(STT_RATE) as usize * MAX_REC_SECS;
        let mut dec = sh.dec.lock().unwrap();
        let mut peak = 0.0f32;
        for frame in data.chunks(ch) {
            let v = frame.iter().map(|s| conv(*s)).sum::<f32>() / ch as f32;
            peak = peak.max(v.abs());
            if b.len() >= cap {
                continue;
            }
            decimate_push(&mut b, &mut dec, v, step);
        }
        if peak > 0.0 {
            sh.signal.store(true, Ordering::Relaxed);
        }
        let mut l = sh.level.lock().unwrap();
        *l = (*l * 0.6).max(peak);
    }

    let cfg: cpal::StreamConfig = config.clone().into();
    let sh = shared.clone();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(&cfg, move |d: &[f32], _: &_| push(d, channels, |s| s, &sh), err_fn, None)?,
        cpal::SampleFormat::I16 => device.build_input_stream(&cfg, move |d: &[i16], _: &_| push(d, channels, |s| s as f32 / 32768.0, &sh), err_fn, None)?,
        cpal::SampleFormat::U16 => {
            device.build_input_stream(&cfg, move |d: &[u16], _: &_| push(d, channels, |s| (s as f32 - 32768.0) / 32768.0, &sh), err_fn, None)?
        }
        cpal::SampleFormat::I32 => {
            device.build_input_stream(&cfg, move |d: &[i32], _: &_| push(d, channels, |s| s as f32 / 2_147_483_648.0, &sh), err_fn, None)?
        }
        f => return Err(anyhow!("지원하지 않는 마이크 형식: {f:?}")),
    };
    stream.play().with_context(|| format!("마이크를 시작할 수 없습니다. {}", crate::mic_permission::how_to_fix()))?;
    Ok(stream)
}

impl Recorder {
    pub fn state(&self) -> RecState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.state(), RecState::Starting | RecState::Recording)
    }

    pub fn elapsed(&self) -> f32 {
        if self.state() != RecState::Recording {
            return 0.0;
        }
        self.started.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0)
    }

    /// Current input loudness 0..1 for the level meter.
    pub fn level(&self) -> f32 {
        *self.shared.level.lock().unwrap()
    }

    /// Name of the microphone actually in use (after the first recording).
    pub fn active_device(&self) -> String {
        self.shared.device.lock().unwrap().clone()
    }

    /// After a moment of recording: does the microphone deliver real sound?
    pub fn problem(&self) -> Option<MicProblem> {
        if self.sim.is_some() || self.state() != RecState::Recording || self.elapsed() < 1.0 {
            return None;
        }
        if self.shared.callbacks.load(Ordering::Relaxed) == 0 {
            Some(MicProblem::NoData)
        } else if !self.shared.signal.load(Ordering::Relaxed) && self.elapsed() >= BLOCKED_AFTER {
            // Only after the silent stream has been reopened (see SILENT_REOPEN) without success.
            Some(MicProblem::Blocked)
        } else {
            None
        }
    }

    /// Asks the audio thread to open the mic. Poll `state()` for Recording / Failed.
    pub fn start(&mut self) {
        self.shared.buf.lock().unwrap().clear();
        *self.shared.dec.lock().unwrap() = (0.0, 0.0, 0.0);
        *self.shared.level.lock().unwrap() = 0.0;
        self.shared.callbacks.store(0, Ordering::Relaxed);
        self.shared.signal.store(false, Ordering::Relaxed);
        *self.state.lock().unwrap() = RecState::Starting;
        self.started = Some(Instant::now());
        if self.sim.is_some() {
            *self.state.lock().unwrap() = RecState::Recording;
            return;
        }
        let _ = self.tx.send(RecCmd::Start(self.device.clone()));
    }

    /// Stops recording and returns mono 16 kHz audio ready for speech recognition.
    pub fn stop(&mut self) -> Vec<f32> {
        if let Some(script) = &self.sim {
            *self.state.lock().unwrap() = RecState::Idle;
            self.started = None;
            let secs = script.lock().unwrap().front().map(|s| s.1).unwrap_or(1.0);
            let mut v = vec![0.0f32; STT_RATE as usize / 4];
            v.extend((0..(secs * STT_RATE as f32) as usize).map(|i| (i as f32 * 0.07).sin() * 0.3));
            v.extend(vec![0.0f32; STT_RATE as usize / 4]);
            return v;
        }
        let _ = self.tx.send(RecCmd::Stop);
        if matches!(self.state(), RecState::Starting | RecState::Recording) {
            *self.state.lock().unwrap() = RecState::Idle;
        }
        self.started = None;
        *self.shared.level.lock().unwrap() = 0.0;
        let raw = std::mem::take(&mut *self.shared.buf.lock().unwrap());
        // Already 16 kHz unless the mic itself runs slower (e.g. a Bluetooth headset at 8 kHz).
        let rate = self.shared.rate.load(Ordering::Relaxed);
        let audio = if rate > STT_RATE { raw } else { resample(&raw, rate, STT_RATE) };
        crate::applog!("mic: 녹음 결과 {} 샘플 ({:.1}초) · rms {:.4}", audio.len(), audio.len() as f32 / STT_RATE as f32, rms(&audio));
        audio
    }

    /// Clears a failed state after the error has been shown.
    pub fn reset(&mut self) {
        *self.state.lock().unwrap() = RecState::Idle;
    }
}

/// Linear-interpolation resampler (adequate for speech recognition input).
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx.min(input.len() - 1)];
            let b = input[(idx + 1).min(input.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// Band-limited (windowed-sinc) resampler for playback. Linear interpolation, which rodio
/// uses internally, leaves metallic aliasing when 24 kHz speech is played on a 48/96 kHz
/// speaker; this keeps the voice clean.
pub fn resample_hq(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 || to == 0 {
        return input.to_vec();
    }
    use std::f64::consts::PI;
    let ratio = to as f64 / from as f64;
    // Cutoff relative to the input rate's Nyquist; below 1 when downsampling.
    let cutoff = ratio.min(1.0) * 0.95;
    let half = 16.0 / cutoff; // window half-width in input samples (16 zero crossings)
    // The kernel is tabulated once (512 steps per input sample) so each tap is a lookup.
    const STEPS: f64 = 512.0;
    let table: Vec<f32> = (0..=(half * STEPS) as usize + 1)
        .map(|k| {
            let x = k as f64 / STEPS;
            let sinc = if x < 1e-9 { 1.0 } else { (PI * cutoff * x).sin() / (PI * cutoff * x) };
            let w = if x >= half { 0.0 } else { 0.42 + 0.5 * (PI * x / half).cos() + 0.08 * (2.0 * PI * x / half).cos() };
            (cutoff * sinc * w) as f32
        })
        .collect();
    let out_len = (input.len() as f64 * ratio).round() as usize;
    let last = input.len() as i64 - 1;
    (0..out_len)
        .map(|j| {
            let t = j as f64 / ratio;
            let (lo, hi) = (((t - half).ceil() as i64).max(0), ((t + half).floor() as i64).min(last));
            let mut acc = 0.0f32;
            for i in lo..=hi {
                let k = ((t - i as f64).abs() * STEPS) as usize;
                acc += input[i as usize] * table[k.min(table.len() - 1)];
            }
            acc
        })
        .collect()
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Seconds between the first and last clearly voiced moment (ignores leading/trailing silence).
pub fn voiced_seconds(samples: &[f32], rate: u32) -> f32 {
    let win = (rate / 50) as usize; // 20 ms frames
    if samples.len() < win {
        return 0.0;
    }
    let frames: Vec<f32> = samples.chunks(win).map(rms).collect();
    let peak = frames.iter().cloned().fold(0.0f32, f32::max);
    let thr = (peak * 0.15).max(0.01);
    let first = frames.iter().position(|f| *f > thr);
    let last = frames.iter().rposition(|f| *f > thr);
    match (first, last) {
        (Some(a), Some(b)) => (b + 1 - a) as f32 * win as f32 / rate as f32,
        _ => 0.0,
    }
}

/// Words per minute over the voiced part of a recording.
pub fn wpm(words: usize, voiced_secs: f32) -> f32 {
    if voiced_secs < 0.3 { 0.0 } else { words as f32 / voiced_secs * 60.0 }
}

// ---------------------------------------------------------------- playback

enum PlayCmd {
    Play(Vec<f32>, u32),
    /// Queue after whatever is playing (sentence-by-sentence speech).
    Append(Vec<f32>, u32),
    Stop,
    Device(String),
}

/// The open output device and the name it was opened for.
struct Output {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    name: String,
    /// The device's own sample rate; clips are converted to it with `resample_hq`.
    rate: u32,
}

impl Output {
    fn clip(&self, samples: Vec<f32>, rate: u32) -> rodio::buffer::SamplesBuffer<f32> {
        if rate == self.rate || self.rate == 0 {
            return rodio::buffer::SamplesBuffer::new(1, rate, samples);
        }
        rodio::buffer::SamplesBuffer::new(1, self.rate, resample_hq(&samples, rate, self.rate))
    }
}

/// Opens the chosen speaker (or the system default).
fn open_output(preferred: &str) -> Option<Output> {
    let device = pick_device(preferred, false)?;
    let name = device.name().unwrap_or_default();
    let rate = device.default_output_config().map(|c| c.sample_rate().0).unwrap_or(0);
    match OutputStream::try_from_device(&device).or_else(|_| OutputStream::try_default()) {
        Ok((stream, handle)) => {
            crate::applog!("speaker: 출력 열기 · {name} · {rate} Hz");
            Some(Output { _stream: stream, handle, name, rate })
        }
        Err(e) => {
            crate::applog!("speaker: 출력 열기 실패 · {e}");
            None
        }
    }
}

/// Audio output on a dedicated thread (rodio's OutputStream must stay on the thread that
/// created it, and opening the device can be slow).
/// Before each clip the output is reopened if the speaker was changed or unplugged, so sound
/// keeps working after switching to headphones.
pub struct Player {
    tx: std::sync::mpsc::Sender<PlayCmd>,
    /// When the output device started opening, while it hasn't finished yet.
    opening_since: Arc<Mutex<Option<Instant>>>,
    playing: Arc<AtomicBool>,
    /// The device could not be opened at all.
    failed: Arc<AtomicBool>,
    pub clip_len: f32,
}

impl Player {
    /// No audio output at all (automated tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        let (tx, _rx) = std::sync::mpsc::channel();
        Self { tx, opening_since: Arc::default(), playing: Arc::default(), failed: Arc::default(), clip_len: 0.0 }
    }

    pub fn new(device: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<PlayCmd>();
        let playing = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let opening_since: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(Some(Instant::now())));
        let (pl, fl, os) = (playing.clone(), failed.clone(), opening_since.clone());
        std::thread::spawn(move || {
            let mut preferred = device;
            // Open the device up front so a broken audio system is detected before the
            // learner presses play.
            let mut out = open_output(&preferred);
            fl.store(out.is_none(), Ordering::Relaxed);
            *os.lock().unwrap() = None;
            let mut sink: Option<Sink> = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(40)) {
                    Ok(PlayCmd::Play(samples, rate)) => {
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                        // Follow the system default when it changes (headphones plugged in…). Only
                        // the default is checked: listing every device can take seconds.
                        let default_changed = preferred.is_empty()
                            && cpal::default_host().default_output_device().and_then(|d| d.name().ok()).is_some_and(|w| out.as_ref().is_some_and(|o| o.name != w));
                        if out.is_none() || default_changed {
                            drop(out.take()); // release the old device first
                            *os.lock().unwrap() = Some(Instant::now());
                            out = open_output(&preferred);
                            *os.lock().unwrap() = None;
                        }
                        fl.store(out.is_none(), Ordering::Relaxed);
                        if let Some(o) = &out {
                            if let Ok(s) = Sink::try_new(&o.handle) {
                                s.append(o.clip(samples, rate));
                                sink = Some(s);
                            }
                        }
                    }
                    Ok(PlayCmd::Append(samples, rate)) => {
                        match (&sink, &out) {
                            (Some(s), Some(o)) if !s.empty() => s.append(o.clip(samples, rate)),
                            _ => {
                                if out.is_none() {
                                    out = open_output(&preferred);
                                    fl.store(out.is_none(), Ordering::Relaxed);
                                }
                                if let Some(o) = &out {
                                    if let Ok(s) = Sink::try_new(&o.handle) {
                                        s.append(o.clip(samples, rate));
                                        sink = Some(s);
                                    }
                                }
                            }
                        }
                    }
                    Ok(PlayCmd::Stop) => {
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                    }
                    Ok(PlayCmd::Device(name)) => {
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                        preferred = name;
                        drop(out.take());
                        out = open_output(&preferred);
                        fl.store(out.is_none(), Ordering::Relaxed);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
                pl.store(sink.as_ref().map(|s| !s.empty()).unwrap_or(false), Ordering::Relaxed);
            }
        });
        Self { tx, opening_since, playing, failed, clip_len: 0.0 }
    }

    /// A cloneable sender for other threads (the speech synthesizer) to play through.
    pub fn handle(&self) -> PlayerHandle {
        PlayerHandle { tx: self.tx.clone(), playing: self.playing.clone() }
    }

    /// True when the OS audio output has not responded for several seconds (e.g. a hung
    /// coreaudiod on macOS) — nothing can be heard until it's fixed.
    pub fn output_stuck(&self) -> bool {
        self.opening_since.lock().unwrap().map(|t| t.elapsed() > Duration::from_secs(4)).unwrap_or(false)
    }

    /// The output device could not be opened.
    pub fn output_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Switches to another speaker (empty = system default).
    pub fn set_device(&mut self, name: String) {
        let _ = self.tx.send(PlayCmd::Device(name));
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(PlayCmd::Stop);
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }


    pub fn play_samples(&mut self, samples: Vec<f32>, rate: u32) {
        self.clip_len = samples.len() as f32 / rate.max(1) as f32;
        // Report "playing" right away; the audio thread confirms within a frame or two.
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(PlayCmd::Play(samples, rate));
    }

    /// A short two-note chime for checking the speakers.
    pub fn test_tone(&mut self) {
        let rate = 44_100;
        let note = |f: f32, secs: f32| {
            let n = (rate as f32 * secs) as usize;
            (0..n).map(move |i| {
                let t = i as f32 / rate as f32;
                let env = (t * 40.0).min(1.0) * (1.0 - i as f32 / n as f32);
                (t * f * std::f32::consts::TAU).sin() * 0.3 * env
            })
        };
        let samples: Vec<f32> = note(660.0, 0.25).chain(note(880.0, 0.35)).collect();
        self.play_samples(samples, rate);
    }
}

#[derive(Clone)]
pub struct PlayerHandle {
    tx: std::sync::mpsc::Sender<PlayCmd>,
    playing: Arc<AtomicBool>,
}

impl PlayerHandle {
    pub fn play(&self, samples: Vec<f32>, rate: u32) {
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(PlayCmd::Play(samples, rate));
    }
    pub fn append(&self, samples: Vec<f32>, rate: u32) {
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(PlayCmd::Append(samples, rate));
    }
}

/// Decodes `[start, end)` seconds of an audio file into mono samples.
/// Saves mono audio as 16-bit PCM WAV (weekly recordings; ~1.9 MB per minute at 16 kHz).
pub fn write_wav(path: &Path, samples: &[f32], rate: u32) -> Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out).context("녹음 파일을 저장할 수 없습니다")?;
    Ok(())
}

pub fn decode_segment(path: &Path, start: f32, end: f32) -> Result<(Vec<f32>, u32)> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut dec = rodio::Decoder::new(file).context("오디오 파일을 열 수 없습니다")?;
    let rate = dec.sample_rate();
    let ch = dec.channels() as usize;
    let start = start.max(0.0);
    // Seek when supported; otherwise skip samples manually.
    let skip_frames = if dec.try_seek(Duration::from_secs_f32(start)).is_ok() { 0 } else { (start * rate as f32) as usize };
    let take_frames = ((end - start).max(0.1) * rate as f32) as usize;
    let mut out = Vec::with_capacity(take_frames);
    let mut frame_acc = 0.0f32;
    let mut i = 0usize;
    let mut frame = 0usize;
    for s in dec.by_ref() {
        frame_acc += s as f32 / 32768.0;
        i += 1;
        if i == ch {
            if frame >= skip_frames {
                out.push(frame_acc / ch as f32);
                if out.len() >= take_frames {
                    break;
                }
            }
            frame += 1;
            frame_acc = 0.0;
            i = 0;
        }
    }
    Ok((out, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_input_is_reduced_to_16k_as_it_arrives() {
        for rate in [44_100u32, 48_000, 96_000] {
            let (mut out, mut dec) = (Vec::new(), (0.0f32, 0.0f32, 0.0f64));
            let step = rate as f64 / STT_RATE as f64;
            // 1 s of a 440 Hz tone.
            for i in 0..rate {
                decimate_push(&mut out, &mut dec, (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5, step);
            }
            assert!((out.len() as i64 - STT_RATE as i64).abs() <= 1, "{rate}: {} samples", out.len());
            let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.45 && peak <= 0.5, "{rate}: speech-band tone kept ({peak})");
        }
        // Slower mics (Bluetooth 8 kHz) are stored untouched.
        let (mut out, mut dec) = (Vec::new(), (0.0f32, 0.0f32, 0.0f64));
        decimate_push(&mut out, &mut dec, 0.3, 0.5);
        assert_eq!(out, vec![0.3]);
    }

    #[test]
    fn voiced_span_ignores_silence() {
        let rate = 16000;
        let mut v = vec![0.0f32; rate]; // 1 s silence
        v.extend((0..rate * 2).map(|i| (i as f32 * 0.1).sin() * 0.5)); // 2 s "speech"
        v.extend(vec![0.0f32; rate]);
        let s = voiced_seconds(&v, rate as u32);
        assert!((s - 2.0).abs() < 0.1, "{s}");
        assert_eq!(wpm(30, 12.0), 150.0);
    }

    /// Real microphone: records 1.5 s and reports what the recorder detected.
    #[test]
    #[ignore]
    fn real_mic_reports_problem() {
        let mut r = Recorder::new(std::env::var("SPEAKUP_TEST_MIC").unwrap_or_default());
        r.start();
        std::thread::sleep(Duration::from_millis(1500));
        let problem = r.problem();
        let audio = r.stop();
        println!("device={} state_problem={problem:?} samples={} rms={:.5}", r.active_device(), audio.len(), rms(&audio));
        assert!(!audio.is_empty(), "the stream delivered audio");
    }

    /// Real microphone: opens and closes the mic several times in a row, like repeated drill tries.
    #[test]
    #[ignore]
    fn real_mic_repeated_recordings() {
        let mut r = Recorder::new(std::env::var("SPEAKUP_TEST_MIC").unwrap_or_default());
        let rss = || {
            let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0)
        };
        let n: usize = std::env::var("SPEAKUP_TEST_TRIES").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let rss0 = rss();
        for i in 1..=n {
            r.start();
            let t0 = Instant::now();
            while r.state() == RecState::Starting && t0.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(20));
            }
            let open = t0.elapsed().as_secs_f32();
            let state = r.state();
            std::thread::sleep(Duration::from_millis(1500));
            let problem = r.problem();
            let audio = r.stop();
            let cb = r.shared.callbacks.load(Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(300));
            let after = r.shared.callbacks.load(Ordering::Relaxed) - cb;
            println!("try {i}: open {open:.2}s state={state:?} problem={problem:?} rate={} samples={} rms={:.5} callbacks_after_stop={after}", r.shared.rate.load(Ordering::Relaxed), audio.len(), rms(&audio));
            assert!(after <= 2, "the previous stream must stop");
            assert!(audio.len() < 16_000 * 2, "one recording's worth of audio");
        }
        println!("rss growth over {n} recordings: {} KB", rss() as i64 - rss0 as i64);
    }

    /// How long a (wireless) mic sends silence after opening, depending on how long it was idle.
    #[test]
    #[ignore]
    fn real_mic_wakeup() {
        let mut r = Recorder::new(std::env::var("SPEAKUP_TEST_MIC").unwrap_or_default());
        for idle in [0u64, 0, 5, 15, 40] {
            std::thread::sleep(Duration::from_secs(idle));
            r.start();
            std::thread::sleep(Duration::from_millis(3500));
            let audio = r.stop();
            let first = audio.iter().position(|s| s.abs() > 1e-6).map(|i| format!("{:.2}s", i as f32 / STT_RATE as f32)).unwrap_or("never".into());
            println!("idle {idle:>2}s → first sound at {first} of {:.1}s", audio.len() as f32 / STT_RATE as f32);
        }
    }

    /// Device lookups like the 3-second mic check, many times over (watch coreaudiod meanwhile).
    #[test]
    #[ignore]
    fn real_device_lookups() {
        let n: usize = std::env::var("SPEAKUP_TEST_TRIES").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        let t = Instant::now();
        for _ in 0..n {
            let _ = current_input_name(&std::env::var("SPEAKUP_TEST_MIC").unwrap_or_default());
        }
        println!("{n} lookups in {:?}", t.elapsed());
    }

    #[test]
    fn resample_halves_length() {
        let v: Vec<f32> = (0..32000).map(|i| (i as f32).sin()).collect();
        assert_eq!(resample(&v, 32000, 16000).len(), 16000);
    }

    #[test]
    fn detects_headset_mics() {
        let outs = vec!["MacBook Pro 스피커".to_string(), "WH-1000XM3".to_string()];
        assert!(is_headset_mic("WH-1000XM3", &outs));
        assert!(is_headset_mic("Headset (WH-1000XM3 Hands-Free)", &[]));
        assert!(!is_headset_mic("MacBook Pro 마이크", &outs));
        assert!(!is_headset_mic("", &outs));
        let ins = vec!["WH-1000XM3".to_string(), "MacBook Pro 마이크".to_string()];
        assert_eq!(non_headset_mic(&ins, &outs).as_deref(), Some("MacBook Pro 마이크"));
    }

    #[test]
    fn hq_resample_keeps_tone_and_length() {
        // A 1 kHz tone at 24 kHz → 96 kHz: same length in seconds, same amplitude, no aliasing.
        let v: Vec<f32> = (0..24000).map(|i| (i as f32 * 1000.0 * std::f32::consts::TAU / 24000.0).sin() * 0.5).collect();
        let out = resample_hq(&v, 24000, 96000);
        assert_eq!(out.len(), 96000);
        let ideal: Vec<f32> = (0..96000).map(|i| (i as f32 * 1000.0 * std::f32::consts::TAU / 96000.0).sin() * 0.5).collect();
        let err = out[1000..95000].iter().zip(&ideal[1000..95000]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(err < 0.01, "max error {err}");
        // Downsampling halves the length.
        assert_eq!(resample_hq(&v, 24000, 12000).len(), 12000);
    }
}
