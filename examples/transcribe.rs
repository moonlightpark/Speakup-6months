//! Transcribes 16-bit WAV files with a whisper model: `cargo run --example transcribe -- model.bin a.wav ...`
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ctx = WhisperContext::new_with_params(&args[0], WhisperContextParameters::default()).unwrap();
    for path in &args[1..] {
        let mut r = hound::WavReader::open(path).unwrap();
        let rate = r.spec().sample_rate;
        let raw: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        let rms = (raw.iter().map(|x| x * x).sum::<f32>() / raw.len() as f32).sqrt();
        // Linear resample to 16 kHz.
        let ratio = rate as f64 / 16000.0;
        let audio: Vec<f32> = (0..(raw.len() as f64 / ratio) as usize).map(|i| raw[((i as f64 * ratio) as usize).min(raw.len() - 1)]).collect();
        let mut st = ctx.create_state().unwrap();
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let lang = std::env::var("LANG_STT").unwrap_or_else(|_| "en".into());
        p.set_language(Some(&lang));
        p.set_print_progress(false);
        st.full(p, &audio).unwrap();
        let text: String = (0..st.full_n_segments().unwrap()).map(|i| st.full_get_segment_text(i).unwrap()).collect();
        println!("{path}: rms {rms:.3} -> {}", text.trim());
    }
}
