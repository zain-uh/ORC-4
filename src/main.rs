//! ORC-4: smart-glasses operating environment.
//!
//! Wiring only: load config, pick the display backend, run the app loop.
//! See `docs/phase2-architecture.md` for the module layout.

mod app;
mod config;
mod input;
mod logging;
mod platform;
mod runtime;
mod screens;
mod ui;

use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Instant;

fn main() -> ExitCode {
    let started = Instant::now();
    logging::init();
    log::info!("ORC-4 {} starting", env!("CARGO_PKG_VERSION"));

    match run(started) {
        Ok(()) => {
            log::info!("exited cleanly");
            ExitCode::SUCCESS
        }
        Err(err) => {
            log::error!("fatal: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(started: Instant) -> anyhow::Result<()> {
    let paths = config::Paths::from_env();
    let settings = config::load(&paths);
    logging::apply_level(&settings.log_level);

    let mut display = platform::open(&settings)?;
    let (tx, rx) = mpsc::channel();
    runtime::run(display.as_mut(), &tx, &rx, started)
}
