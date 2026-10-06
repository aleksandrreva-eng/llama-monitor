//! Local file logger for diagnostics.
//!
//! Writes log lines to `llama-monitor.log` next to the executable, and mirrors
//! them to stderr (handy during `tauri:dev`). Debug/Trace are gated by the
//! `verbose_logging` setting. Callers must never log secrets — this module
//! only formats what it is given.
//!
//! The file is capped at [`MAX_LOG_BYTES`]: a widget that runs for weeks must
//! not grow a log until the disk fills.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::sync::lock;

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Hard cap on the log file size. On exceeding it the file is truncated and a
/// marker line is written, so the newest entries always survive.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Whether writing `next_len` more bytes would push the file past the cap.
fn should_truncate(current_bytes: u64, next_len: usize) -> bool {
    current_bytes.saturating_add(next_len as u64) > MAX_LOG_BYTES
}

/// Enable/disable Debug+Trace logging at runtime.
pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::SeqCst);
}

/// Path to the log file.
///
/// The MSI installs into `Program Files`, which is **not writable** by a
/// non-elevated process (writing there fails with os error 5 / "Отказано в
/// доступе", observed live right after installing). So the log lives under
/// `%LOCALAPPDATA%\llama-monitor\` — the canonical per-user writable location.
///
/// For a portable (unzip-and-run) copy we keep a fallback: if the directory
/// beside the executable *is* writable, use it, which keeps the "log next to
/// the exe" behaviour that is convenient during development.
pub fn log_path() -> PathBuf {
    let file_name = "llama-monitor.log";

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    // Portable case: an adjacent writable directory wins.
    if let Some(dir) = exe_dir.clone() {
        let candidate = dir.join(file_name);
        if candidate.exists() || dir_writable(&dir) {
            return candidate;
        }
    }

    if let Some(data_dir) = data_dir() {
        return data_dir.join(file_name);
    }

    // Last resort: current working directory.
    PathBuf::from(file_name)
}

/// `%LOCALAPPDATA%\llama-monitor`, created on demand.
pub(crate) fn data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    let dir = base.join("llama-monitor");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Cheap writability probe: can we create a temp file in `dir`?
fn dir_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".llama-monitor-write-probe");
    match File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Open (creating if needed) the log file in append mode.
pub fn open() -> std::io::Result<File> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    OpenOptions::new().create(true).append(true).open(path)
}

struct FileLogger {
    file: Mutex<File>,
    /// Approximate number of bytes written so far. Tracked in memory because
    /// asking the filesystem on every line would be far more expensive than the
    /// write itself.
    bytes: AtomicU64,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if metadata.level() <= Level::Info {
            true
        } else {
            VERBOSE.load(Ordering::SeqCst)
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
        let module = record.module_path().unwrap_or("");
        let line = format!(
            "[{}] {} {}: {}\n",
            ts,
            record.level(),
            module,
            record.args()
        );

        let mut file = lock(&self.file);
        let written = self.bytes.load(Ordering::Relaxed);
        if should_truncate(written, line.len()) {
            // Truncate instead of rotating to `.1`: the file is opened in append
            // mode, so a zero-length file simply starts over, while a rotation
            // would have to swap the handle out from under other threads.
            if file.set_len(0).is_ok() {
                self.bytes.store(0, Ordering::Relaxed);
                let marker = format!("[... log truncated after {written} bytes ...]\n");
                let _ = file.write_all(marker.as_bytes());
            }
        }
        if file.write_all(line.as_bytes()).is_ok() {
            self.bytes.fetch_add(line.len() as u64, Ordering::Relaxed);
        }
        let _ = file.flush();
        drop(file);

        // Mirror to stderr for development convenience.
        eprint!("{}", line);
    }

    fn flush(&self) {
        let _ = lock(&self.file).flush();
    }
}

/// Install the global file logger. Call once at startup.
pub fn init_file_logger(verbose: bool) {
    set_verbose(verbose);
    match open() {
        Ok(file) => {
            let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = log::set_boxed_logger(Box::new(FileLogger {
                file: Mutex::new(file),
                bytes: AtomicU64::new(bytes),
            }));
            log::set_max_level(LevelFilter::Trace);
        }
        Err(e) => {
            // Fall back to stderr-only logging if the file can't be opened.
            eprintln!("failed to open log file: {e}");
            let _ = log::set_logger(&StderrLogger);
            log::set_max_level(LevelFilter::Trace);
        }
    }
}

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        eprintln!(
            "[{}] {} {}: {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.module_path().unwrap_or(""),
            record.args()
        );
    }
    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log must never be written into a read-only install directory
    /// (Program Files → os error 5). `log_path()` must therefore resolve to a
    /// writable location and `open()` must succeed.
    #[test]
    fn log_path_is_writable() {
        let path = log_path();
        println!("resolved log path: {path:?}");

        // Whatever we pick, we must be able to actually create/append to it.
        let file = open().expect("log file must be openable");
        drop(file);

        if let Some(parent) = path.parent() {
            assert!(
                parent.as_os_str().is_empty() || parent.exists(),
                "log directory must exist: {parent:?}"
            );
        }
    }

    /// A non-empty filename, so a directory is never used as the log file.
    #[test]
    fn log_file_name_is_sane() {
        let name = log_path()
            .file_name()
            .expect("log path must have a file name")
            .to_string_lossy()
            .to_string();
        assert_eq!(name, "llama-monitor.log");
    }

    /// The log must be capped, or a widget left running for weeks fills the disk.
    #[test]
    fn log_size_is_capped() {
        assert!(!should_truncate(0, 100));
        assert!(!should_truncate(MAX_LOG_BYTES - 10, 10));
        assert!(should_truncate(MAX_LOG_BYTES - 10, 11));
        assert!(should_truncate(MAX_LOG_BYTES, 1));
        // A pathologically long line must not overflow the arithmetic.
        assert!(should_truncate(u64::MAX, 1));
    }
}
