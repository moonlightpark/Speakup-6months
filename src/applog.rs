//! Debug log for the development period: `logs/speakup.log` in the data folder.
//! Records app start, screen changes, mic/speaker streams, speech recognition, Claude calls,
//! errors, and the app's memory every minute (to catch memory growth early).
//! The file is rotated to `speakup.old.log` at 5 MB, so it never grows without bound.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const MAX_BYTES: u64 = 5 * 1024 * 1024;

struct Log {
    path: PathBuf,
    file: File,
    bytes: u64,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

fn open(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Opens the log in `dir/logs` and starts the once-a-minute memory line. Without this call
/// (tests) `write` only goes to stderr.
pub fn init(dir: &Path) {
    let logs = dir.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let path = logs.join("speakup.log");
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if bytes > MAX_BYTES {
        let _ = std::fs::rename(&path, logs.join("speakup.old.log"));
    }
    let Some(file) = open(&path) else { return };
    let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    if LOG.set(Mutex::new(Log { path, file, bytes })).is_err() {
        return;
    }
    write(&format!("==== SpeakUp {} 시작 ({} {}) · 데이터 {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, dir.display()));
    let _ = std::thread::Builder::new().name("applog-mem".into()).spawn(|| loop {
        if let Some(mb) = rss_mb() {
            write(&format!("memory: app {mb} MB"));
        }
        std::thread::sleep(Duration::from_secs(60));
    });
}

/// Appends one timestamped line (also printed to stderr).
pub fn write(msg: &str) {
    eprintln!("{msg}");
    let Some(log) = LOG.get() else { return };
    let mut l = log.lock().unwrap_or_else(|e| e.into_inner());
    let line = format!("{} {msg}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
    if l.bytes + line.len() as u64 > MAX_BYTES {
        let old = l.path.with_file_name("speakup.old.log");
        let _ = std::fs::rename(&l.path, old);
        if let Some(f) = open(&l.path) {
            l.file = f;
            l.bytes = 0;
        }
    }
    if l.file.write_all(line.as_bytes()).is_ok() {
        l.bytes += line.len() as u64;
    }
}

/// Resident memory of this process in MB.
#[cfg(unix)]
fn rss_mb() -> Option<u64> {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

/// Asked of Windows directly: running `tasklist` every minute flashed a console window over the
/// app (a windowed program that starts a console program gets a new console).
#[cfg(windows)]
fn rss_mb() -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(process: *mut std::ffi::c_void, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
    }
    let mut c = ProcessMemoryCounters { cb: std::mem::size_of::<ProcessMemoryCounters>() as u32, ..Default::default() };
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    (ok != 0).then(|| c.working_set_size as u64 / (1024 * 1024))
}

/// `applog!("...", args)` — formats and appends a line to the debug log.
#[macro_export]
macro_rules! applog {
    ($($t:tt)*) => { $crate::applog::write(&format!($($t)*)) };
}

#[cfg(test)]
mod tests {
    /// The memory line works without starting another program (on Windows that flashed a console).
    #[test]
    fn reads_own_memory() {
        let mb = super::rss_mb().expect("memory of this process");
        assert!(mb > 0 && mb < 64 * 1024, "{mb} MB");
    }
}
