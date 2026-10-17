//! Natural-sounding speech with Kokoro-82M, a neural text-to-speech model, run offline through
//! sherpa-onnx. The model (about 330 MB) is downloaded once into the data folder and reused.
//!
//! Text is split into sentences; the first sentence starts playing as soon as it is ready
//! while the rest are synthesized, and every synthesized sentence is cached so repeating a
//! drill sentence is instant.

use crate::audio::PlayerHandle;
use anyhow::{Context, Result, anyhow};
use sherpa_rs::tts::{KokoroTts, KokoroTtsConfig};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

pub const MODEL_DIR: &str = "kokoro-multi-lang-v1_0";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2";
/// Written after a complete download + extraction; its presence means "never download again".
const DONE_MARK: &str = ".complete";

/// English voices of Kokoro v1.0: (speaker id, name, label). The first is the default.
pub const VOICES: &[(i32, &str, &str)] = &[
    (3, "af_heart", "Heart · 미국 여성 (추천)"),
    (2, "af_bella", "Bella · 미국 여성"),
    (6, "af_nicole", "Nicole · 미국 여성 (차분함)"),
    (9, "af_sarah", "Sarah · 미국 여성"),
    (16, "am_michael", "Michael · 미국 남성"),
    (14, "am_fenrir", "Fenrir · 미국 남성"),
    (18, "am_puck", "Puck · 미국 남성 (밝음)"),
    (21, "bf_emma", "Emma · 영국 여성"),
    (22, "bf_isabella", "Isabella · 영국 여성"),
    (26, "bm_george", "George · 영국 남성"),
    (25, "bm_fable", "Fable · 영국 남성"),
];

pub fn voice_label(name: &str) -> &'static str {
    VOICES.iter().find(|v| v.1 == name).map(|v| v.2).unwrap_or(VOICES[0].2)
}

fn voice_id(name: &str) -> i32 {
    VOICES.iter().find(|v| v.1 == name).map(|v| v.0).unwrap_or(VOICES[0].0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum NeuralStatus {
    Missing,
    Downloading,
    Loading,
    Ready,
    Error(String),
}

enum Job {
    Load(PathBuf),
    /// Speak now (cancels what is playing).
    Say { text: String, voice: String, speed: f32, generation: u64 },
    /// Synthesize into the cache only, so a later `Say` is instant.
    Prefetch { text: String, voice: String, speed: f32 },
}

pub struct NeuralTts {
    status: Arc<Mutex<NeuralStatus>>,
    pub downloaded: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    tx: Sender<Job>,
    generation: Arc<AtomicU64>,
}

pub fn model_dir(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_DIR)
}

pub fn is_installed(models_dir: &Path) -> bool {
    model_dir(models_dir).join(DONE_MARK).exists()
}

impl NeuralTts {
    pub fn new(player: PlayerHandle) -> Self {
        let (tx, rx) = channel();
        let s = Self {
            status: Arc::new(Mutex::new(NeuralStatus::Missing)),
            downloaded: Arc::default(),
            total: Arc::default(),
            tx,
            generation: Arc::default(),
        };
        let (status, generation) = (s.status.clone(), s.generation.clone());
        std::thread::spawn(move || worker(rx, status, generation, player));
        s
    }

    pub fn status(&self) -> NeuralStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.status() == NeuralStatus::Ready
    }

    pub fn load(&self, models_dir: &Path) {
        *self.status.lock().unwrap() = NeuralStatus::Loading;
        let _ = self.tx.send(Job::Load(model_dir(models_dir)));
    }

    /// Downloads and unpacks the model once, then loads it.
    pub fn download(&self, models_dir: PathBuf) {
        *self.status.lock().unwrap() = NeuralStatus::Downloading;
        let (status, done, total, tx) = (self.status.clone(), self.downloaded.clone(), self.total.clone(), self.tx.clone());
        std::thread::spawn(move || match download_and_unpack(&models_dir, &done, &total) {
            Ok(dir) => {
                *status.lock().unwrap() = NeuralStatus::Loading;
                let _ = tx.send(Job::Load(dir));
            }
            Err(e) => *status.lock().unwrap() = NeuralStatus::Error(format!("음성 모델 다운로드 실패: {e:#}")),
        });
    }

    pub fn say(&self, text: &str, voice: &str, speed: f32) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Job::Say { text: text.to_string(), voice: voice.to_string(), speed, generation });
    }

    pub fn prefetch(&self, text: &str, voice: &str, speed: f32) {
        let _ = self.tx.send(Job::Prefetch { text: text.to_string(), voice: voice.to_string(), speed });
    }

    /// Cancels speech that is still being synthesized.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// Splits text into sentences so long passages start playing quickly.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        cur.push(*c);
        let end = matches!(c, '.' | '!' | '?' | ';' | '\n') && chars.get(i + 1).is_none_or(|n| n.is_whitespace() || *n == '"' || *n == '\'');
        if end && cur.trim().split_whitespace().count() >= 2 {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

type Key = (String, String, u32);

/// Synthesized clips, most recently used last; bounded so memory stays small (~1 MB per sentence).
struct Cache {
    map: HashMap<Key, Arc<Vec<f32>>>,
    order: VecDeque<Key>,
}

impl Cache {
    const MAX: usize = 300;
    fn get(&mut self, k: &Key) -> Option<Arc<Vec<f32>>> {
        let v = self.map.get(k).cloned()?;
        self.order.retain(|x| x != k);
        self.order.push_back(k.clone());
        Some(v)
    }
    fn put(&mut self, k: Key, v: Arc<Vec<f32>>) {
        if self.map.insert(k.clone(), v).is_none() {
            self.order.push_back(k);
        }
        while self.order.len() > Self::MAX {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

fn open_engine(dir: &Path, british: bool) -> Result<KokoroTts> {
    let p = |f: &str| dir.join(f).to_string_lossy().into_owned();
    let model = dir.join("model.onnx");
    if !model.exists() {
        return Err(anyhow!("모델 파일이 없습니다: {}", model.display()));
    }
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(2, 8) as i32).unwrap_or(4);
    let lexicon = if british { "lexicon-gb-en.txt" } else { "lexicon-us-en.txt" };
    Ok(KokoroTts::new(KokoroTtsConfig {
        model: p("model.onnx"),
        voices: p("voices.bin"),
        tokens: p("tokens.txt"),
        data_dir: p("espeak-ng-data"),
        dict_dir: p("dict"),
        lexicon: format!("{},{}", p(lexicon), p("lexicon-zh.txt")),
        length_scale: 1.0,
        // "en-gb" makes sherpa-onnx throw (and abort the process); the British lexicon with
        // the "en-us" fallback gives British pronunciation without that.
        lang: "en-us".into(),
        onnx_config: sherpa_rs::OnnxConfig { num_threads: threads, provider: "cpu".into(), ..Default::default() },
        ..Default::default()
    }))
}

fn worker(rx: Receiver<Job>, status: Arc<Mutex<NeuralStatus>>, generation: Arc<AtomicU64>, player: PlayerHandle) {
    // One engine per accent (the lexicon differs); the British one is opened on first use.
    let mut dir: Option<PathBuf> = None;
    let mut engines: HashMap<bool, KokoroTts> = HashMap::new();
    let mut cache = Cache { map: HashMap::new(), order: VecDeque::new() };
    let mut backlog: VecDeque<Job> = VecDeque::new();
    loop {
        // Speech requests jump ahead of prefetching.
        let job = match rx.try_recv() {
            Ok(j) => j,
            Err(_) => match backlog.pop_front() {
                Some(j) => j,
                None => match rx.recv() {
                    Ok(j) => j,
                    Err(_) => break,
                },
            },
        };
        match job {
            Job::Load(d) => match open_engine(&d, false) {
                Ok(mut e) => {
                    // The first inference is several times slower than the rest; pay it now.
                    let _ = e.create("Hello there.", VOICES[0].0, 1.0);
                    engines.clear();
                    engines.insert(false, e);
                    dir = Some(d);
                    *status.lock().unwrap() = NeuralStatus::Ready;
                }
                Err(e) => *status.lock().unwrap() = NeuralStatus::Error(format!("{e:#}")),
            },
            Job::Prefetch { text, voice, speed } => {
                if dir.is_none() {
                    continue;
                }
                for s in sentences(&text) {
                    // Stop prefetching as soon as the learner asks for something.
                    if let Ok(j) = rx.try_recv() {
                        backlog.push_front(j);
                        break;
                    }
                    let key = (s.clone(), voice.clone(), (speed * 100.0) as u32);
                    if cache.get(&key).is_none() {
                        if let Some(clip) = synth(&mut engines, dir.as_deref(), &s, &voice, speed) {
                            cache.put(key, clip);
                        }
                    }
                }
            }
            Job::Say { text, voice, speed, generation: g } => {
                if dir.is_none() {
                    continue;
                }
                let parts = sentences(&text);
                for (i, s) in parts.iter().enumerate() {
                    if generation.load(Ordering::SeqCst) != g {
                        break; // superseded or stopped
                    }
                    let key = (s.clone(), voice.clone(), (speed * 100.0) as u32);
                    let clip = match cache.get(&key) {
                        Some(c) => Some(c),
                        None => synth(&mut engines, dir.as_deref(), s, &voice, speed).inspect(|c| cache.put(key, c.clone())),
                    };
                    if generation.load(Ordering::SeqCst) != g {
                        break;
                    }
                    if let Some(c) = clip {
                        let mut samples = (*c).clone();
                        // A short natural pause between sentences.
                        samples.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize / 6));
                        if i == 0 { player.play(samples, SAMPLE_RATE) } else { player.append(samples, SAMPLE_RATE) }
                    }
                }
            }
        }
    }
}

pub const SAMPLE_RATE: u32 = 24_000;

fn synth(engines: &mut HashMap<bool, KokoroTts>, dir: Option<&Path>, text: &str, voice: &str, speed: f32) -> Option<Arc<Vec<f32>>> {
    let british = voice.starts_with('b');
    if !engines.contains_key(&british) {
        let e = open_engine(dir?, british).map_err(|e| eprintln!("tts: {e:#}")).ok()?;
        engines.insert(british, e);
    }
    let engine = engines.get_mut(&british)?;
    match engine.create(text, voice_id(voice), speed.clamp(0.5, 2.0)) {
        Ok(a) => Some(Arc::new(fade_edges(a.samples, a.sample_rate))),
        Err(e) => {
            eprintln!("tts: {e:#}");
            None
        }
    }
}

/// 5 ms fade in/out so clips joined back to back never click.
fn fade_edges(mut s: Vec<f32>, rate: u32) -> Vec<f32> {
    let n = ((rate / 200) as usize).min(s.len() / 2);
    let len = s.len();
    for i in 0..n {
        let g = i as f32 / n as f32;
        s[i] *= g;
        s[len - 1 - i] *= g;
    }
    s
}

fn download_and_unpack(models_dir: &Path, done: &AtomicU64, total: &AtomicU64) -> Result<PathBuf> {
    let dest = model_dir(models_dir);
    if dest.join(DONE_MARK).exists() {
        return Ok(dest);
    }
    let client = reqwest::blocking::Client::builder().timeout(None).build()?;
    let resp = client.get(MODEL_URL).send()?.error_for_status()?;
    total.store(resp.content_length().unwrap_or(0), Ordering::Relaxed);
    done.store(0, Ordering::Relaxed);
    struct Counting<'a, R: Read>(R, &'a AtomicU64);
    impl<R: Read> Read for Counting<'_, R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.read(buf)?;
            self.1.fetch_add(n as u64, Ordering::Relaxed);
            Ok(n)
        }
    }
    // Stream straight from the network into the unpacker (no 330 MB temp file).
    let tmp = models_dir.join(format!("{MODEL_DIR}.part"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let bz = bzip2::read::BzDecoder::new(Counting(resp, done));
    tar::Archive::new(bz).unpack(&tmp).context("압축 해제 실패")?;
    let inner = tmp.join(MODEL_DIR);
    let unpacked = if inner.exists() { inner } else { tmp.clone() };
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&unpacked, &dest)?;
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::write(dest.join(DONE_MARK), MODEL_URL)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences() {
        assert_eq!(sentences("Hello there. How are you? I'm fine!"), vec!["Hello there.", "How are you?", "I'm fine!"]);
        // Abbreviation-like one-word pieces stay attached; decimals don't split.
        assert_eq!(sentences("Mr. Smith paid 3.5 dollars."), vec!["Mr. Smith paid 3.5 dollars."]);
        assert_eq!(sentences("No punctuation"), vec!["No punctuation"]);
    }

    /// Downloads and installs the model into the app's real data folder (what the button does).
    /// cargo test install_kokoro -- --ignored --nocapture
    #[test]
    #[ignore]
    fn install_kokoro_into_data_dir() {
        let models = crate::storage::data_dir().join("models");
        let (done, total) = (AtomicU64::new(0), AtomicU64::new(0));
        let t = std::time::Instant::now();
        let dir = download_and_unpack(&models, &done, &total).unwrap();
        println!("installed {} ({} MB) in {:?}", dir.display(), done.load(Ordering::Relaxed) / 1_000_000, t.elapsed());
        assert!(is_installed(&models));
        let mut engines = HashMap::new();
        assert!(synth(&mut engines, Some(&dir), "Installed and ready.", "af_heart", 1.0).is_some());
    }

    /// SPEAKUP_TEST_KOKORO=<model dir> cargo test kokoro -- --ignored --nocapture
    #[test]
    #[ignore]
    fn kokoro_synthesizes_real_speech() {
        let dir = PathBuf::from(std::env::var("SPEAKUP_TEST_KOKORO").expect("SPEAKUP_TEST_KOKORO"));
        let mut engines = HashMap::new();
        for voice in ["af_heart", "bm_george"] {
            let t = std::time::Instant::now();
            let clip = synth(&mut engines, Some(&dir), "Could I get a glass of water, please?", voice, 1.0).expect("audio");
            println!("{voice}: {:.2}s audio in {:?}", clip.len() as f32 / SAMPLE_RATE as f32, t.elapsed());
            assert!(clip.len() > SAMPLE_RATE as usize, "at least a second of speech");
            if let Ok(out) = std::env::var("SPEAKUP_TEST_WAV_DIR") {
                let mut w = hound::WavWriter::create(format!("{out}/{voice}.wav"), hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
                for s in clip.iter() {
                    w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
                }
            }
        }
    }

    /// The whole path: model load → sentence synthesis → audio player.
    #[test]
    #[ignore]
    fn speaks_through_the_player() {
        let models = crate::storage::data_dir().join("models");
        let player = crate::audio::Player::new(String::new());
        let tts = NeuralTts::new(player.handle());
        tts.load(&models);
        let t = std::time::Instant::now();
        while !tts.is_ready() {
            assert!(!matches!(tts.status(), NeuralStatus::Error(_)), "{:?}", tts.status());
            assert!(t.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let t = std::time::Instant::now();
        tts.say("Good morning. This is your first lesson. Let's begin!", "af_heart", 1.0);
        while !player.is_playing() {
            assert!(t.elapsed().as_secs() < 10, "nothing played");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        println!("first sound after {:?}", t.elapsed());
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(player.is_playing());
    }
}

#[cfg(test)]
mod playback_quality {
    use super::*;

    /// Converts real speech to a 96 kHz speaker rate both ways and writes WAVs for comparison.
    /// SPEAKUP_TEST_KOKORO=<model dir> cargo test --release alias -- --ignored --nocapture
    #[test]
    #[ignore]
    fn alias_energy_hq_vs_linear() {
        let dir = PathBuf::from(std::env::var("SPEAKUP_TEST_KOKORO").expect("SPEAKUP_TEST_KOKORO"));
        let mut engines = HashMap::new();
        let clip = synth(&mut engines, Some(&dir), "Could I get a glass of water, please? Thanks so much, that's really kind of you.", "af_heart", 1.0).unwrap();
        let peak = clip.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let t = std::time::Instant::now();
        let hq = crate::audio::resample_hq(&clip, SAMPLE_RATE, 96_000);
        println!("{:.2}s clip, peak {peak:.2}, hq resample took {:?}", clip.len() as f32 / SAMPLE_RATE as f32, t.elapsed());
        let lin = crate::audio::resample(&clip, SAMPLE_RATE, 96_000);
        assert!(t.elapsed().as_millis() < 300, "fast enough to run before every clip");
        // Spectrum check (numpy FFT on the WAVs): linear leaves aliasing at about -27 dB above
        // 12 kHz, the band-limited resampler about -58 dB.
        if let Ok(out) = std::env::var("SPEAKUP_TEST_WAV_DIR") {
            for (name, v) in [("linear96k", &lin), ("hq96k", &hq)] {
                let mut w = hound::WavWriter::create(format!("{out}/{name}.wav"), hound::WavSpec { channels: 1, sample_rate: 96_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
                for s in v.iter() {
                    w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
                }
            }
        }
    }
}
