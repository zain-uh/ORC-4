//! ORC-4: smart-glasses operating environment.
//!
//! Wiring only: load config, pick the display backend, run the app loop.
//! See `docs/phase2-architecture.md` for the module layout.

mod app;
mod config;
mod event;
mod input;
mod logging;
mod platform;
mod runtime;
mod screens;
mod services;
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

    prepare_files_folder(&paths);

    let mut display = platform::open(&settings)?;
    let (tx, rx) = mpsc::channel();
    runtime::run(display.as_mut(), &tx, &rx, settings, &paths, started)
}

/// Makes sure the Files app has a folder. In the simulator, a new folder
/// gets a welcome note so there is something to open.
fn prepare_files_folder(paths: &config::Paths) {
    let dir = paths.files();
    if dir.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::error!("cannot create {}: {e}", dir.display());
        return;
    }
    if cfg!(feature = "simulator") {
        let note = "Welcome to ORC-4.\n\nThis is the Files app. Put your own notes, \
                    pictures and documents in the dev-data/files folder to see them here.\n\n\
                    Up and Down turn pages. Esc goes back.";
        let _ = std::fs::write(dir.join("Welcome.txt"), note);
    }
}
