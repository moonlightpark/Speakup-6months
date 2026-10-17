//! Offline speech recognition with whisper.cpp (whisper-rs), running on a worker thread.
//! The model file is downloaded once into the data directory.
//!
//! The model (~0.5 GB in memory) is only loaded when the learner presses a mic button — it
//! loads while they are still speaking — and is released again after a few idle minutes.

use crate::audio::{STT_RATE, rms};
use anyhow::{Result, anyhow};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Native model loading (whisper.cpp's Metal device setup, onnxruntime's first inference) is
/// not safe to run on several threads at once; every model load takes this lock.
pub static MODEL_LOAD: Mutex<()> = Mutex::new(());

/// Unused for this long, the model is released from memory (reloaded on the next mic press).
const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(180);

#[derive(Clone, Debug, PartialEq)]
pub enum SttStatus {
    Missing,
    Downloading,
    Ready,
    Error(String),
}

enum Job {
    /// Use this model file; it is loaded on first need.
    Load(PathBuf),
    /// Load now (the mic was just pressed) so recognition is instant afterwards.
    Warm,
    Transcribe(Vec<f32>, Sender<Result<String>>),
}

pub struct Stt {
    pub status: Arc<Mutex<SttStatus>>,
    /// The model is in memory right now (it can be Ready without being loaded).
    pub loaded: Arc<std::sync::atomic::AtomicBool>,
    pub downloaded: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    tx: Sender<Job>,
    script: Option<crate::audio::Script>,
}

pub fn model_path(models_dir: &Path, model: &str) -> PathBuf {
    models_dir.join(format!("ggml-{model}.bin"))
}

impl Stt {
    pub fn new(language: &'static str) -> Self {
        whisper_rs::install_logging_hooks(); // routes whisper.cpp's verbose logs away from stderr
        let (tx, rx) = channel::<Job>();
        let status = Arc::new(Mutex::new(SttStatus::Missing));
        let loaded: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let (st, ld) = (status.clone(), loaded.clone());
        std::thread::spawn(move || worker(rx, st, ld, language));
        Self { status, loaded, downloaded: Arc::default(), total: Arc::default(), tx, script: None }
    }

    /// A recognizer that returns scripted text instead of running whisper (automated tests).
    #[cfg(test)]
    pub fn scripted(script: crate::audio::Script) -> Self {
        let (tx, _rx) = channel::<Job>();
        Self {
            status: Arc::new(Mutex::new(SttStatus::Ready)),
            loaded: Arc::default(),
            downloaded: Arc::default(),
            total: Arc::default(),
            tx,
            script: Some(script),
        }
    }

    pub fn status(&self) -> SttStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.status() == SttStatus::Ready
    }

    /// Selects the model file. Nothing is loaded into memory until a mic button is pressed.
    pub fn load(&self, path: PathBuf) {
        *self.status.lock().unwrap() = if path.exists() { SttStatus::Ready } else { SttStatus::Missing };
        let _ = self.tx.send(Job::Load(path));
    }

    /// Starts loading the model in the background (called when recording starts).
    pub fn warm(&self) {
        if self.script.is_none() && self.is_ready() && !self.loaded.load(Ordering::Relaxed) {
            let _ = self.tx.send(Job::Warm);
        }
    }

    /// Downloads the model from Hugging Face (ggerganov/whisper.cpp), then loads it.
    pub fn download(&self, models_dir: PathBuf, model: String) {
        *self.status.lock().unwrap() = SttStatus::Downloading;
        let (status, done, total, tx) = (self.status.clone(), self.downloaded.clone(), self.total.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let dest = model_path(&models_dir, &model);
            match download_file(
                &format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{model}.bin"),
                &dest,
                &done,
                &total,
            ) {
                Ok(()) => {
                    *status.lock().unwrap() = SttStatus::Ready;
                    let _ = tx.send(Job::Load(dest));
                }
                Err(e) => *status.lock().unwrap() = SttStatus::Error(format!("모델 다운로드 실패: {e}")),
            }
        });
    }

    /// Queues 16 kHz mono audio for transcription; the result arrives on the returned channel.
    pub fn transcribe(&self, audio: Vec<f32>) -> Receiver<Result<String>> {
        let (tx, rx) = channel();
        if let Some(script) = &self.script {
            let text = script.lock().unwrap().pop_front().map(|s| s.0).unwrap_or_default();
            let _ = tx.send(Ok(text));
            return rx;
        }
        if rms(&audio) < 0.004 {
            let _ = tx.send(Err(anyhow!("목소리가 들리지 않았어요. 마이크에 조금 더 크게 말해 보세요.")));
        } else if !self.is_ready() {
            let _ = tx.send(Err(anyhow!("음성 인식 모델이 아직 준비되지 않았습니다 (설정 확인).")));
        } else {
            let _ = self.tx.send(Job::Transcribe(audio, tx));
        }
        rx
    }
}

fn worker(rx: Receiver<Job>, status: Arc<Mutex<SttStatus>>, loaded: Arc<std::sync::atomic::AtomicBool>, language: &'static str) {
    let mut path: Option<PathBuf> = None;
    let mut ctx: Option<WhisperContext> = None;
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4);
    // Loads the model if it isn't in memory yet.
    let ensure = |ctx: &mut Option<WhisperContext>, path: &Option<PathBuf>| -> Result<()> {
        if ctx.is_some() {
            return Ok(());
        }
        let p = path.as_ref().ok_or_else(|| anyhow!("모델이 선택되지 않았습니다"))?.to_string_lossy().to_string();
        let _guard = MODEL_LOAD.lock().unwrap_or_else(|e| e.into_inner());
        let c = WhisperContext::new_with_params(&p, WhisperContextParameters::default()).map_err(|e| anyhow!("모델 로드 실패: {e}"))?;
        *ctx = Some(c);
        loaded.store(true, Ordering::Relaxed);
        Ok(())
    };
    loop {
        let job = match rx.recv_timeout(IDLE_UNLOAD) {
            Ok(j) => j,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Idle: give the memory back.
                ctx = None;
                loaded.store(false, Ordering::Relaxed);
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match job {
            Job::Load(p) => {
                if path.as_ref() != Some(&p) {
                    ctx = None;
                    loaded.store(false, Ordering::Relaxed);
                }
                path = Some(p);
            }
            Job::Warm => {
                if let Err(e) = ensure(&mut ctx, &path) {
                    *status.lock().unwrap() = SttStatus::Error(format!("{e:#}"));
                }
            }
            Job::Transcribe(mut audio, reply) => {
                let result = (|| -> Result<String> {
                    ensure(&mut ctx, &path)?;
                    let ctx = ctx.as_ref().ok_or_else(|| anyhow!("모델이 로드되지 않았습니다"))?;
                    // whisper needs at least ~1 s of audio; pad short clips with silence.
                    let min = STT_RATE as usize * 11 / 10;
                    if audio.len() < min {
                        audio.resize(min, 0.0);
                    }
                    let mut state = ctx.create_state()?;
                    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                    params.set_language(Some(language));
                    params.set_n_threads(threads);
                    params.set_no_context(true);
                    params.set_suppress_blank(true);
                    params.set_print_progress(false);
                    params.set_print_realtime(false);
                    params.set_print_special(false);
                    params.set_print_timestamps(false);
                    state.full(params, &audio)?;
                    let n = state.full_n_segments()?;
                    let mut text = String::new();
                    for i in 0..n {
                        text.push_str(&state.full_get_segment_text(i)?);
                    }
                    Ok(clean_transcript(&text))
                })();
                let _ = reply.send(result);
            }
        }
    }
}

/// Removes whisper's non-speech annotations such as "[BLANK_AUDIO]" or "(music)".
pub fn clean_transcript(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn download_file(url: &str, dest: &Path, done: &AtomicU64, total: &AtomicU64) -> Result<()> {
    let client = reqwest::blocking::Client::builder().timeout(None).build()?;
    let mut resp = client.get(url).send()?.error_for_status()?;
    total.store(resp.content_length().unwrap_or(0), Ordering::Relaxed);
    done.store(0, Ordering::Relaxed);
    let tmp = dest.with_extension("part");
    let mut f = std::fs::File::create(&tmp)?;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
        done.fetch_add(n as u64, Ordering::Relaxed);
    }
    f.flush()?;
    drop(f);
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end check with a real whisper model and real speech:
    /// SPEAKUP_TEST_MODEL=.../ggml-base.en.bin SPEAKUP_TEST_AUDIO=dir cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn transcribes_and_scores_real_speech() {
        let model = std::env::var("SPEAKUP_TEST_MODEL").expect("SPEAKUP_TEST_MODEL");
        let dir = PathBuf::from(std::env::var("SPEAKUP_TEST_AUDIO").expect("SPEAKUP_TEST_AUDIO"));
        let stt = Stt::new("en");
        stt.load(model.into());
        let t0 = std::time::Instant::now();
        while stt.status() != SttStatus::Ready {
            assert!(!matches!(stt.status(), SttStatus::Error(_)), "{:?}", stt.status());
            assert!(t0.elapsed().as_secs() < 60, "model load timeout");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let read = |name: &str| -> Vec<f32> {
            let mut r = hound::WavReader::open(dir.join(name)).unwrap();
            let spec = r.spec();
            let s: Vec<f32> = r.samples::<i16>().map(|x| x.unwrap() as f32 / 32768.0).collect();
            crate::audio::resample(&s, spec.sample_rate, STT_RATE)
        };
        let target = "Could I get a glass of water, please?";
        let cases = [("good.wav", target, true), ("partial.wav", target, false)];
        for (file, target, should_pass) in cases {
            let audio = read(file);
            let t = std::time::Instant::now();
            let heard = stt.transcribe(audio).recv().unwrap().unwrap();
            let r = crate::scoring::score(target, &heard);
            println!("{file}: heard {heard:?} -> {:.0} ({} ms)", r.score, t.elapsed().as_millis());
            assert_eq!(r.score >= 80.0, should_pass, "{file}");
        }
        // 1.5x speed: recognition should still work, and WPM must come out higher.
        let normal = read("good.wav");
        let fast = read("fast.wav");
        let (s_norm, s_fast) = (crate::audio::voiced_seconds(&normal, STT_RATE), crate::audio::voiced_seconds(&fast, STT_RATE));
        let heard = stt.transcribe(fast).recv().unwrap().unwrap();
        let r = crate::scoring::score(target, &heard);
        let ok = r.marks.iter().filter(|m| m.ok).count();
        println!("fast.wav: {heard:?} -> {:.0}%, {:.0} WPM ({s_fast:.2}s) vs normal {:.0} WPM ({s_norm:.2}s)", r.score, crate::audio::wpm(ok, s_fast), crate::audio::wpm(8, s_norm));
        assert!(r.score >= 80.0);
        assert!(s_fast < s_norm * 0.8);
        // silence is rejected before reaching whisper
        assert!(stt.transcribe(vec![0.0; 16000]).recv().unwrap().is_err());
    }

    #[test]
    fn strips_annotations() {
        assert_eq!(clean_transcript(" [BLANK_AUDIO] Hello (laughs) there "), "Hello there");
    }
}
