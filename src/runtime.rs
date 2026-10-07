//! The main loop: drains events, routes them to the active screen, and
//! presents a new frame only when something changed.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use crate::app::event::Event;
use crate::config::Settings;
use crate::platform::Display;
use crate::screens::launcher::Launcher;
use crate::services::clock;
use crate::ui::DrawCx;
use crate::ui::canvas::Canvas;
use crate::ui::nav::Navigator;
use crate::ui::theme::Theme;
use crate::ui::widgets::status_bar::Status;

/// Runs until shutdown. The UI thread sleeps whenever nothing changes: it
/// redraws only after an event, a running animation or a clock minute change.
pub fn run(
    display: &mut dyn Display,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    settings: &Settings,
    started: Instant,
) -> anyhow::Result<()> {
    let theme = Theme::default();
    let utc_offset = settings.clock.utc_offset_minutes;
    let mut status = Status {
        clock: clock::local_hhmm(utc_offset),
    };
    let mut screens = Navigator::new(Box::new(Launcher::new()));
    let mut canvas = Canvas::new();
    let mut dirty = true;
    let mut first_frame = true;

    loop {
        display.pump(tx)?;

        // Block only when idle and the backend does not need pumping; wake
        // at the next minute so the clock stays right.
        let first = if dirty || display.needs_polling() {
            rx.try_recv().ok()
        } else {
            rx.recv_timeout(clock::until_next_minute()).ok()
        };

        for event in first
            .into_iter()
            .chain(std::iter::from_fn(|| rx.try_recv().ok()))
        {
            match event {
                Event::Shutdown => {
                    log::info!("shutdown requested");
                    return Ok(());
                }
                Event::Input(input) => {
                    log::debug!(
                        "input {:?} repeat={} from {:?}",
                        input.action,
                        input.repeat,
                        input.source
                    );
                    if screens.handle(input.action, Instant::now()) {
                        dirty = true;
                    }
                }
            }
        }

        let now_clock = clock::local_hhmm(utc_offset);
        if now_clock != status.clock {
            status.clock = now_clock;
            dirty = true;
        }
        if screens.animate(Instant::now()) {
            dirty = true;
        }

        if dirty {
            let cx = DrawCx {
                theme: &theme,
                status: &status,
            };
            screens.draw(&mut canvas, &cx);
            display.present(&canvas)?;
            dirty = false;
            if first_frame {
                first_frame = false;
                log::info!("first frame after {} ms", started.elapsed().as_millis());
            }
        }
    }
}
