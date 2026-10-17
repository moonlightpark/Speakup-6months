//! Microphone capture (cpal) and playback (rodio): learner recordings, YouTube segments with
//! A-B looping and pitch-preserving slow-down.

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
pub fn current_input_name(preferred: &str) -> String {
    pick_device(preferred, true).and_then(|d| d.name().ok()).unwrap_or_default()
}

/// The device called `name`, or the system default when `name` is empty or not connected.
fn pick_device(name: &str, input: bool) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if !name.is_empty() {
        let found = if input { host.input_devices().ok() } else { host.output_devices().ok() }
            .and_then(|mut it| it.find(|d| d.name().is_ok_and(|n| n == name)));
        if found.is_some() {
            return found;
        }
    }
    if input { host.default_input_device() } else { host.default_output_device() }
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
    pub fn message(self) -> String {
        match self {
            MicProblem::NoData => "마이크에서 소리가 들어오지 않습니다. 설정 > 오디오 장치에서 다른 마이크를 골라 보세요.".into(),
            MicProblem::Blocked if crate::mic_permission::lid_closed() => {
                "노트북 덮개가 닫혀 있어 내장 마이크가 꺼져 있습니다(입력 신호 0). 덮개를 열거나, 헤드셋·이어폰 마이크를 연결한 뒤 설정 > 오디오 장치에서 그 마이크를 고르세요.".into()
            }
            MicProblem::Blocked => format!("마이크 소리가 차단되어 있습니다(입력 신호 0). 다른 마이크를 고르거나 권한을 확인하세요. {}", crate::mic_permission::how_to_fix()),
        }
    }
}

enum RecCmd {
    Start(String),
    Stop,
}

/// Shared between the UI and the audio thread.
#[derive(Default)]
struct RecShared {
    buf: Mutex<Vec<f32>>,
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
            for cmd in rx {
                match cmd {
                    RecCmd::Start(name) => {
                        stream = None;
                        match open_input(&shared, &state, &name) {
                            Ok(s) => {
                                stream = Some(s);
                                let mut st = state.lock().unwrap();
                                // A Stop may already have arrived while the device was opening.
                                if *st == RecState::Starting {
                                    *st = RecState::Recording;
                                }
                            }
                            Err(e) => *state.lock().unwrap() = RecState::Failed(format!("{e:#}")),
                        }
                    }
                    RecCmd::Stop => stream = None,
                }
            }
            drop(stream);
        });
        r
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
        eprintln!("mic stream error: {e}");
        let mut s = st.lock().unwrap();
        if *s == RecState::Recording {
            *s = RecState::Failed(format!("마이크 연결이 끊겼습니다: {e}"));
        }
    };

    fn push<T: Copy>(data: &[T], ch: usize, conv: impl Fn(T) -> f32, sh: &RecShared) {
        sh.callbacks.fetch_add(1, Ordering::Relaxed);
        let mut b = sh.buf.lock().unwrap();
        let mut peak = 0.0f32;
        for frame in data.chunks(ch) {
            let v = frame.iter().map(|s| conv(*s)).sum::<f32>() / ch as f32;
            peak = peak.max(v.abs());
            b.push(v);
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
        } else if !self.shared.signal.load(Ordering::Relaxed) {
            Some(MicProblem::Blocked)
        } else {
            None
        }
    }

    /// Asks the audio thread to open the mic. Poll `state()` for Recording / Failed.
    pub fn start(&mut self) {
        self.shared.buf.lock().unwrap().clear();
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
        resample(&raw, self.shared.rate.load(Ordering::Relaxed), STT_RATE)
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

// ---------------------------------------------------------------- time stretching

/// WSOLA time-stretch: changes speed without changing pitch. `speed` < 1 slows down.
pub fn time_stretch(input: &[f32], rate: u32, speed: f32) -> Vec<f32> {
    if (speed - 1.0).abs() < 0.01 || input.len() < 4096 {
        return input.to_vec();
    }
    let win = (rate as f32 * 0.040) as usize; // 40 ms window
    let hop_out = win / 2;
    let hop_in = (hop_out as f32 * speed) as usize;
    let tol = (rate as f32 * 0.010) as usize; // ±10 ms search
    let window: Vec<f32> = (0..win)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / win as f32).cos())
        .collect();
    let n_frames = input.len().saturating_sub(win + 2 * tol) / hop_in.max(1);
    let mut out = vec![0.0f32; n_frames * hop_out + win];
    let mut norm = vec![0.0f32; out.len()];
    let mut prev_pos = 0usize;
    for f in 0..n_frames {
        let nominal = f * hop_in + tol;
        // Pick the offset whose frame best continues the previous frame's natural successor.
        let pos = if f == 0 {
            nominal
        } else {
            let natural = prev_pos + hop_out;
            let mut best = nominal;
            let mut best_corr = f32::MIN;
            let lo = nominal.saturating_sub(tol);
            let hi = (nominal + tol).min(input.len().saturating_sub(win));
            let cmp_len = hop_out.min(input.len().saturating_sub(natural));
            for cand in (lo..=hi).step_by(2) {
                let mut c = 0.0;
                for k in (0..cmp_len).step_by(4) {
                    c += input[cand + k] * input[natural + k];
                }
                if c > best_corr {
                    best_corr = c;
                    best = cand;
                }
            }
            best
        };
        let o = f * hop_out;
        for k in 0..win {
            if pos + k < input.len() {
                out[o + k] += input[pos + k] * window[k];
                norm[o + k] += window[k];
            }
        }
        prev_pos = pos;
    }
    for (s, n) in out.iter_mut().zip(&norm) {
        if *n > 1e-3 {
            *s /= n;
        }
    }
    out
}

// ---------------------------------------------------------------- playback

enum PlayCmd {
    Play(Vec<f32>, u32, bool),
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
}

/// Opens the chosen speaker (or the system default).
fn open_output(preferred: &str) -> Option<Output> {
    let device = pick_device(preferred, false)?;
    let name = device.name().unwrap_or_default();
    match OutputStream::try_from_device(&device).or_else(|_| OutputStream::try_default()) {
        Ok((stream, handle)) => Some(Output { _stream: stream, handle, name }),
        Err(e) => {
            eprintln!("audio output unavailable: {e}");
            None
        }
    }
}

/// Audio output on a dedicated thread (rodio's OutputStream must stay on the thread that
/// created it, and opening the device can be slow). Looping clips restart with a short pause.
/// Before each clip the output is reopened if the speaker was changed or unplugged, so sound
/// keeps working after switching to headphones.
pub struct Player {
    tx: std::sync::mpsc::Sender<PlayCmd>,
    /// When the output device started opening, while it hasn't finished yet.
    opening_since: Arc<Mutex<Option<Instant>>>,
    playing: Arc<AtomicBool>,
    /// The device could not be opened at all.
    failed: Arc<AtomicBool>,
    clip_start: Arc<Mutex<Option<Instant>>>,
    pub clip_len: f32,
}

impl Player {
    /// No audio output at all (automated tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        let (tx, _rx) = std::sync::mpsc::channel();
        Self { tx, opening_since: Arc::default(), playing: Arc::default(), failed: Arc::default(), clip_start: Arc::default(), clip_len: 0.0 }
    }

    pub fn new(device: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<PlayCmd>();
        let playing = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let clip_start: Arc<Mutex<Option<Instant>>> = Arc::default();
        let opening_since: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(Some(Instant::now())));
        let (pl, fl, cs, os) = (playing.clone(), failed.clone(), clip_start.clone(), opening_since.clone());
        std::thread::spawn(move || {
            let mut preferred = device;
            // Open the device up front so a broken audio system is detected before the
            // learner presses play.
            let mut out = open_output(&preferred);
            fl.store(out.is_none(), Ordering::Relaxed);
            *os.lock().unwrap() = None;
            let mut sink: Option<Sink> = None;
            let mut looping: Option<(Vec<f32>, u32)> = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(40)) {
                    Ok(PlayCmd::Play(samples, rate, looped)) => {
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
                                s.append(rodio::buffer::SamplesBuffer::new(1, rate, samples.clone()));
                                sink = Some(s);
                                *cs.lock().unwrap() = Some(Instant::now());
                            }
                        }
                        looping = looped.then_some((samples, rate));
                    }
                    Ok(PlayCmd::Append(samples, rate)) => {
                        let source = rodio::buffer::SamplesBuffer::new(1, rate, samples);
                        match &sink {
                            Some(s) if !s.empty() => s.append(source),
                            _ => {
                                if out.is_none() {
                                    out = open_output(&preferred);
                                    fl.store(out.is_none(), Ordering::Relaxed);
                                }
                                if let Some(o) = &out {
                                    if let Ok(s) = Sink::try_new(&o.handle) {
                                        s.append(source);
                                        sink = Some(s);
                                        *cs.lock().unwrap() = Some(Instant::now());
                                    }
                                }
                            }
                        }
                        looping = None;
                    }
                    Ok(PlayCmd::Stop) => {
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                        looping = None;
                        *cs.lock().unwrap() = None;
                    }
                    Ok(PlayCmd::Device(name)) => {
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                        looping = None;
                        preferred = name;
                        drop(out.take());
                        out = open_output(&preferred);
                        fl.store(out.is_none(), Ordering::Relaxed);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if let (Some(s), Some((samples, rate))) = (&sink, &looping) {
                    if s.empty() {
                        let mut clip = vec![0.0; (*rate as f32 * 0.6) as usize];
                        clip.extend_from_slice(samples);
                        s.append(rodio::buffer::SamplesBuffer::new(1, *rate, clip));
                        *cs.lock().unwrap() = Some(Instant::now() - Duration::from_millis(600));
                    }
                }
                pl.store(sink.as_ref().map(|s| !s.empty()).unwrap_or(false), Ordering::Relaxed);
            }
        });
        Self { tx, opening_since, playing, failed, clip_start, clip_len: 0.0 }
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

    /// Seconds into the current clip.
    pub fn position(&self) -> f32 {
        self.clip_start.lock().unwrap().map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0)
    }

    pub fn play_samples(&mut self, samples: Vec<f32>, rate: u32, looped: bool) {
        self.clip_len = samples.len() as f32 / rate.max(1) as f32;
        // Report "playing" right away; the audio thread confirms within a frame or two.
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(PlayCmd::Play(samples, rate, looped));
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
        self.play_samples(samples, rate, false);
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
        let _ = self.tx.send(PlayCmd::Play(samples, rate, false));
    }
    pub fn append(&self, samples: Vec<f32>, rate: u32) {
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(PlayCmd::Append(samples, rate));
    }
}

/// Decodes `[start, end)` seconds of an audio file into mono samples.
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

    #[test]
    fn resample_halves_length() {
        let v: Vec<f32> = (0..32000).map(|i| (i as f32).sin()).collect();
        assert_eq!(resample(&v, 32000, 16000).len(), 16000);
    }

    #[test]
    fn stretch_slows_down() {
        let rate = 16000;
        let v: Vec<f32> = (0..rate * 2).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = time_stretch(&v, rate as u32, 0.75);
        let ratio = out.len() as f32 / v.len() as f32;
        assert!((ratio - 1.333).abs() < 0.1, "ratio {ratio}");
    }
}
