//! Minimal logger: timestamped lines to stderr (journald picks them up on the
//! device) plus an in-memory ring buffer of recent lines, which the Settings
//! app will show so problems can be diagnosed without SSH.

use std::collections::VecDeque;
use std::io::Write;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};

const RING_CAPACITY: usize = 200;

struct Logger {
    start: Instant,
    ring: Mutex<VecDeque<String>>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Installs the logger. The level comes from `ORC4_LOG` if set, otherwise
/// `info` until the config is loaded and `apply_level` is called.
pub fn init() {
    let logger = LOGGER.get_or_init(|| Logger {
        start: Instant::now(),
        ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(env_level().unwrap_or(LevelFilter::Info));
        // Route panics through the logger so they reach journald and the
        // in-app log viewer; systemd then restarts the process.
        std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    }
}

/// Applies the configured level unless `ORC4_LOG` overrides it.
pub fn apply_level(configured: &str) {
    if env_level().is_some() {
        return;
    }
    match LevelFilter::from_str(configured) {
        Ok(level) => log::set_max_level(level),
        Err(_) => log::warn!(
            "unknown log_level {configured:?}, keeping {}",
            log::max_level()
        ),
    }
}

/// Most recent log lines, oldest first.
#[allow(dead_code)] // first caller: Settings → Device → Logs (M4)
pub fn recent() -> Vec<String> {
    LOGGER
        .get()
        .map(|l| {
            l.ring
                .lock()
                .map(|r| r.iter().cloned().collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

fn env_level() -> Option<LevelFilter> {
    std::env::var("ORC4_LOG")
        .ok()
        .and_then(|v| LevelFilter::from_str(&v).ok())
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let t = self.start.elapsed();
        let line = format!(
            "{:>4}.{:03} {:<5} {}: {}",
            t.as_secs(),
            t.subsec_millis(),
            record.level(),
            record.target(),
            record.args()
        );
        let _ = writeln!(std::io::stderr().lock(), "{line}");
        if let Ok(mut ring) = self.ring.lock() {
            if ring.len() == RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line);
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}
