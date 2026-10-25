//! Natural Japanese speech with VOICEVOX (voicevox_core, offline neural TTS) and its "Nemo"
//! voices — plain adult voices made for reading, not characters. Downloaded once into the data
//! folder (~100 MB: VOICEVOX's ONNX Runtime, the Open JTalk dictionary and the Nemo voice
//! model) and reused. Credit: "VOICEVOX Nemo" (shown in 설정 › 음성 and 오픈소스 · 출처).
//!
//! Text is split into sentences; the first one starts playing as soon as it is ready while the
//! rest are synthesized, and synthesized sentences are cached so repeating a drill is instant.
//! The voice model (~600 MB in memory) is loaded on the first sentence and released after a few
//! idle minutes.

use crate::audio::PlayerHandle;
use anyhow::{Context, Result, anyhow};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use voicevox_core::StyleId;
use voicevox_core::blocking::{Onnxruntime, OpenJtalk, Synthesizer, VoiceModelFile};

pub const DIR: &str = "voicevox";
const DICT: &str = "open_jtalk_dic_utf_8-1.11";
const VVM: &str = "n0.vvm";
/// Written after a complete download; its presence means "never download again".
const DONE_MARK: &str = ".complete";

const ONNXRUNTIME_URL: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-1.17.3/voicevox_onnxruntime-osx-arm64-1.17.3.tgz")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-1.17.3/voicevox_onnxruntime-win-x64-1.17.3.tgz")
} else {
    None
};
/// Android: the runtime library is packed into the APK (scripts/build_android.sh) and opened by
/// name from the app's native library folder — a library downloaded into app data can't be loaded.
const BUNDLED_RUNTIME: Option<&str> = if cfg!(target_os = "android") { Some("libvoicevox_onnxruntime.so") } else { None };
const DICT_URL: &str = "https://github.com/r9y9/open_jtalk/releases/download/v1.11.1/open_jtalk_dic_utf_8-1.11.tar.gz";
const VVM_URL: &str = "https://github.com/VOICEVOX/voicevox_vvm/releases/download/0.17.0/n0.vvm";

/// The Nemo voices: (style id, name in the model, label). The first is the default.
pub const VOICES: &[(u32, &str, &str)] = &[
    (10005, "女声1", "여성 1"),
    (10007, "女声2", "여성 2"),
    (10004, "女声3", "여성 3"),
    (10003, "女声4", "여성 4"),
    (10008, "女声5", "여성 5"),
    (10006, "女声6", "여성 6"),
    (10001, "男声1", "남성 1"),
    (10000, "男声2", "남성 2"),
    (10002, "男声3", "남성 3"),
];

pub fn voice_label(id: u32) -> &'static str {
    VOICES.iter().find(|v| v.0 == id).map(|v| v.2).unwrap_or(VOICES[0].2)
}

/// This device can run the engine (VOICEVOX ships its runtime for macOS arm64 / Windows x64 /
/// Android arm64).
pub fn supported() -> bool {
    ONNXRUNTIME_URL.is_some() || BUNDLED_RUNTIME.is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Missing,
    Downloading,
    Ready,
    Error(String),
}

enum Job {
    Load(PathBuf),
    /// Speak now (cancels what is playing).
    Say { text: String, voice: u32, speed: f32, generation: u64 },
    /// Synthesize into the cache only, so a later `Say` is instant.
    Prefetch { text: String, voice: u32, speed: f32 },
}

pub struct VoiceVox {
    status: Arc<Mutex<Status>>,
    pub downloaded: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    tx: Sender<Job>,
    generation: Arc<AtomicU64>,
}

pub fn dir(models_dir: &Path) -> PathBuf {
    models_dir.join(DIR)
}

pub fn is_installed(models_dir: &Path) -> bool {
    dir(models_dir).join(DONE_MARK).exists()
}

impl VoiceVox {
    pub fn new(player: PlayerHandle) -> Self {
        let (tx, rx) = channel();
        let s = Self { status: Arc::new(Mutex::new(Status::Missing)), downloaded: Arc::default(), total: Arc::default(), tx, generation: Arc::default() };
        let (status, generation) = (s.status.clone(), s.generation.clone());
        std::thread::spawn(move || worker(rx, status, generation, player));
        s
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.status() == Status::Ready
    }

    pub fn load(&self, models_dir: &Path) {
        let _ = self.tx.send(Job::Load(dir(models_dir)));
    }

    /// Downloads the engine's files once, then uses them.
    pub fn download(&self, models_dir: PathBuf) {
        *self.status.lock().unwrap() = Status::Downloading;
        let (status, done, total, tx) = (self.status.clone(), self.downloaded.clone(), self.total.clone(), self.tx.clone());
        std::thread::spawn(move || match download_all(&models_dir, &done, &total) {
            Ok(d) => {
                let _ = tx.send(Job::Load(d));
            }
            Err(e) => {
                crate::applog!("voicevox: 다운로드 실패 · {e:#}");
                *status.lock().unwrap() = Status::Error(format!("음성 엔진 다운로드 실패: {e:#}"));
            }
        });
    }

    pub fn say(&self, text: &str, voice: u32, speed: f32) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Job::Say { text: text.to_string(), voice, speed, generation });
    }

    pub fn prefetch(&self, text: &str, voice: u32, speed: f32) {
        let _ = self.tx.send(Job::Prefetch { text: text.to_string(), voice, speed });
    }

    /// Cancels speech that is still being synthesized.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

type Key = (String, u32, u32);

/// Synthesized clips, most recently used last; bounded so memory stays small.
struct Cache {
    map: HashMap<Key, Arc<Vec<f32>>>,
    order: VecDeque<Key>,
}

impl Cache {
    const MAX: usize = 150;
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

/// The voice model is released after this long without speech.
/// (Phones: longer, since loading it again is slow there.)
const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(if cfg!(target_os = "android") { 1800 } else { 300 });

/// The runtime library in the engine folder (its file name carries the version).
fn runtime_lib(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        n.contains("voicevox_onnxruntime") && (n.ends_with(".dylib") || n.ends_with(".dll") || n.contains(".so"))
    })
}

fn open_engine(dir: &Path) -> Result<Synthesizer<OpenJtalk>> {
    let lib = match BUNDLED_RUNTIME {
        Some(name) => std::ffi::OsString::from(name),
        None => runtime_lib(dir).ok_or_else(|| anyhow!("ONNX Runtime 파일이 없습니다"))?.into_os_string(),
    };
    let ort = Onnxruntime::load_once().filename(lib).perform().context("ONNX Runtime을 열 수 없습니다")?;
    let dict = dir.join(DICT);
    let ojt = OpenJtalk::new(dict.to_string_lossy().as_ref()).context("일본어 사전을 열 수 없습니다")?;
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(2, 8) as u16).unwrap_or(4);
    let synth = Synthesizer::builder(ort).text_analyzer(ojt).cpu_num_threads(threads).build().context("음성 엔진을 만들 수 없습니다")?;
    let model = VoiceModelFile::open(dir.join(VVM)).context("음성 모델을 열 수 없습니다")?;
    synth.load_voice_model(&model).perform().context("음성 모델을 불러올 수 없습니다")?;
    Ok(synth)
}

fn worker(rx: Receiver<Job>, status: Arc<Mutex<Status>>, generation: Arc<AtomicU64>, player: PlayerHandle) {
    let mut dir: Option<PathBuf> = None;
    let mut engine: Option<Synthesizer<OpenJtalk>> = None;
    let mut cache = Cache { map: HashMap::new(), order: VecDeque::new() };
    let mut backlog: VecDeque<Job> = VecDeque::new();
    loop {
        // Speech requests jump ahead of prefetching.
        let job = match rx.try_recv() {
            Ok(j) => j,
            Err(_) => match backlog.pop_front() {
                Some(j) => j,
                None => match rx.recv_timeout(IDLE_UNLOAD) {
                    Ok(j) => j,
                    Err(RecvTimeoutError::Timeout) => {
                        // Idle: give the model's memory back; it reloads on the next sentence.
                        if engine.take().is_some() {
                            crate::applog!("voicevox: 쉬는 동안 음성 엔진 해제");
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                },
            },
        };
        match job {
            // The engine itself is opened on the first sentence, not at startup — except on a
            // phone, where opening it takes long enough that the first sentence would wait.
            Job::Load(d) => {
                engine = None;
                if d.join(DONE_MARK).exists() && (BUNDLED_RUNTIME.is_some() || runtime_lib(&d).is_some()) {
                    if cfg!(target_os = "android") && !ensure_open(&mut engine, &d, &status) {
                        continue; // the status says why
                    }
                    dir = Some(d);
                    *status.lock().unwrap() = Status::Ready;
                } else {
                    *status.lock().unwrap() = Status::Error(format!("음성 엔진 파일이 없습니다: {}", d.display()));
                }
            }
            Job::Prefetch { text, voice, speed } => {
                let Some(d) = dir.as_deref() else { continue };
                for s in crate::voice::sentences(&text) {
                    // Stop prefetching as soon as the learner asks for something.
                    if let Ok(j) = rx.try_recv() {
                        backlog.push_front(j);
                        break;
                    }
                    let key = (s.clone(), voice, (speed * 100.0) as u32);
                    if cache.get(&key).is_none() {
                        if let Some(clip) = synth(&mut engine, d, &s, voice, speed, &status) {
                            cache.put(key, clip);
                        }
                    }
                }
            }
            Job::Say { text, voice, speed, generation: g } => {
                let Some(d) = dir.clone() else { continue };
                for (i, s) in crate::voice::sentences(&text).iter().enumerate() {
                    if generation.load(Ordering::SeqCst) != g {
                        break; // superseded or stopped
                    }
                    let key = (s.clone(), voice, (speed * 100.0) as u32);
                    let clip = match cache.get(&key) {
                        Some(c) => Some(c),
                        None => synth(&mut engine, &d, s, voice, speed, &status).inspect(|c| cache.put(key, c.clone())),
                    };
                    if generation.load(Ordering::SeqCst) != g {
                        break;
                    }
                    if let Some(c) = clip {
                        let mut samples = (*c).clone();
                        // A short natural pause between sentences.
                        samples.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize / 8));
                        if i == 0 { player.play(samples, SAMPLE_RATE) } else { player.append(samples, SAMPLE_RATE) }
                    }
                }
            }
        }
    }
}

pub const SAMPLE_RATE: u32 = 24_000;

/// Opens the engine if it isn't open. False (and the status says why) when it can't be.
fn ensure_open(engine: &mut Option<Synthesizer<OpenJtalk>>, dir: &Path, status: &Mutex<Status>) -> bool {
    if engine.is_some() {
        return true;
    }
    let t0 = std::time::Instant::now();
    let _guard = crate::stt::MODEL_LOAD.lock().unwrap_or_else(|e| e.into_inner());
    match open_engine(dir) {
        Ok(e) => {
            crate::applog!("voicevox: 음성 엔진 로드 {:.1}초", t0.elapsed().as_secs_f32());
            *engine = Some(e);
            true
        }
        Err(e) => {
            crate::applog!("voicevox: {e:#}");
            *status.lock().unwrap() = Status::Error(format!("{e:#}"));
            false
        }
    }
}

fn synth(engine: &mut Option<Synthesizer<OpenJtalk>>, dir: &Path, text: &str, voice: u32, speed: f32, status: &Mutex<Status>) -> Option<Arc<Vec<f32>>> {
    if !ensure_open(engine, dir, status) {
        return None;
    }
    let e = engine.as_ref()?;
    let style = StyleId(voice);
    let result = (|| -> Result<Vec<u8>> {
        let mut q = e.create_audio_query(text, style)?;
        q.speed_scale = typed_floats::PositiveFinite::<f32>::new(speed.clamp(0.5, 2.0)).map_err(|_| anyhow!("speed"))?;
        Ok(e.synthesis(&q, style).perform()?)
    })();
    match result {
        Ok(wav) => Some(Arc::new(fade_edges(wav_samples(&wav)))),
        Err(e) => {
            crate::applog!("voicevox: \"{text}\" 합성 실패 · {e:#}");
            None
        }
    }
}

/// 16-bit mono PCM samples of a WAV file (VOICEVOX writes 24 kHz mono).
fn wav_samples(wav: &[u8]) -> Vec<f32> {
    let data = wav.windows(4).position(|w| w == b"data").map(|i| i + 8).unwrap_or(44).min(wav.len());
    wav[data..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
}

/// 5 ms fade in/out so clips joined back to back never click.
fn fade_edges(mut s: Vec<f32>) -> Vec<f32> {
    let n = ((SAMPLE_RATE / 200) as usize).min(s.len() / 2);
    let len = s.len();
    for i in 0..n {
        let g = i as f32 / n as f32;
        s[i] *= g;
        s[len - 1 - i] *= g;
    }
    s
}

/// Downloads one file, counting bytes into `done`.
fn fetch(client: &reqwest::blocking::Client, url: &str, done: &AtomicU64) -> Result<Vec<u8>> {
    let mut resp = client.get(url).send()?.error_for_status()?;
    let mut out = Vec::with_capacity(resp.content_length().unwrap_or(0) as usize);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(out)
}

/// Unpacks a .tar.gz into `into`, keeping only entries `keep` accepts (path → where to write).
fn untar(bytes: &[u8], mut keep: impl FnMut(&Path) -> Option<PathBuf>) -> Result<()> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in ar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        if let Some(dest) = keep(&path) {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(&dest)?;
        }
    }
    Ok(())
}

fn download_all(models_dir: &Path, done: &AtomicU64, total: &AtomicU64) -> Result<PathBuf> {
    let dest = dir(models_dir);
    if dest.join(DONE_MARK).exists() {
        return Ok(dest);
    }
    if !supported() {
        return Err(anyhow!("이 기기에서는 쓸 수 없습니다 (macOS Apple Silicon · Windows 64비트 · Android 전용)"));
    }
    let tmp = models_dir.join(format!("{DIR}.part"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let client = reqwest::blocking::Client::builder().timeout(None).build()?;
    // For the progress bar: GitHub's redirects don't say the sizes up front (about 104 MB together).
    total.store(104_000_000, Ordering::Relaxed);
    done.store(0, Ordering::Relaxed);
    if let Some(ort_url) = ONNXRUNTIME_URL {
        let ort = fetch(&client, ort_url, done).context("ONNX Runtime")?;
        untar(&ort, |p| {
            let name = p.file_name()?.to_string_lossy().to_string();
            (name.contains("voicevox_onnxruntime") && (name.ends_with(".dylib") || name.ends_with(".dll") || name.contains(".so")) || name == "TERMS.txt").then(|| tmp.join(if name == "TERMS.txt" { "ONNXRUNTIME_TERMS.txt".to_string() } else { name }))
        })?;
    }
    let dict = fetch(&client, DICT_URL, done).context("일본어 사전")?;
    untar(&dict, |p| {
        let rel = p.strip_prefix(DICT).ok()?;
        Some(tmp.join(DICT).join(rel))
    })?;
    let vvm = fetch(&client, VVM_URL, done).context("음성 모델")?;
    std::fs::write(tmp.join(VVM), vvm)?;
    if (BUNDLED_RUNTIME.is_none() && runtime_lib(&tmp).is_none()) || !tmp.join(DICT).join("sys.dic").exists() {
        return Err(anyhow!("받은 파일이 올바르지 않습니다"));
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest)?;
    std::fs::write(dest.join(DONE_MARK), VVM_URL)?;
    crate::applog!("voicevox: 음성 엔진 받기 완료 ({} MB)", done.load(Ordering::Relaxed) / 1_000_000);
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wav_samples_after_the_header() {
        let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        wav.extend([0u8; 20]);
        wav.extend(b"data");
        wav.extend(4u32.to_le_bytes());
        wav.extend(16384i16.to_le_bytes());
        wav.extend((-32768i16).to_le_bytes());
        assert_eq!(wav_samples(&wav), vec![0.5, -1.0]);
        assert_eq!(voice_label(10001), "남성 1");
        assert_eq!(voice_label(1), VOICES[0].2, "unknown → default");
    }

    /// Downloads the engine's files for real (about 100 MB):
    /// SPEAKUP_TEST_VOICEVOX_DL=<empty dir> cargo test voicevox_download -- --ignored --nocapture
    #[test]
    #[ignore]
    fn voicevox_download_and_speak() {
        let models = PathBuf::from(std::env::var("SPEAKUP_TEST_VOICEVOX_DL").expect("SPEAKUP_TEST_VOICEVOX_DL"));
        let (done, total) = (AtomicU64::new(0), AtomicU64::new(0));
        let d = download_all(&models, &done, &total).expect("download");
        println!("{} / {} bytes → {}", done.load(Ordering::Relaxed), total.load(Ordering::Relaxed), d.display());
        assert!(is_installed(&models));
        let mut engine = None;
        let clip = synth(&mut engine, &d, "こんにちは。", VOICES[6].0, 1.0, &Mutex::new(Status::Ready)).expect("clip");
        assert!(clip.len() > SAMPLE_RATE as usize / 2);
    }

    /// Speaks with the real engine (needs the downloaded files):
    /// SPEAKUP_TEST_VOICEVOX=<models dir> cargo test voicevox_real -- --ignored --nocapture
    #[test]
    #[ignore]
    fn voicevox_real_synthesis() {
        let models = PathBuf::from(std::env::var("SPEAKUP_TEST_VOICEVOX").expect("SPEAKUP_TEST_VOICEVOX"));
        let d = dir(&models);
        let status = Mutex::new(Status::Ready);
        let mut engine = None;
        let t0 = std::time::Instant::now();
        let clip = synth(&mut engine, &d, "すみません、お水を一杯いただけますか。", VOICES[0].0, 1.0, &status).expect("clip");
        println!("{} samples ({:.1}s audio) in {:?}", clip.len(), clip.len() as f32 / SAMPLE_RATE as f32, t0.elapsed());
        assert!(clip.len() > SAMPLE_RATE as usize);
        let slow = synth(&mut engine, &d, "すみません、お水を一杯いただけますか。", VOICES[0].0, 0.7, &status).expect("slow");
        assert!(slow.len() > clip.len(), "slower speech is longer");
    }
}
