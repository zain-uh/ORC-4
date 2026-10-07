//! The main loop: drains events, routes them to the active screen, and
//! presents a new frame only when something changed.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use crate::app::event::Event;
use crate::platform::Display;
use crate::screens::test_pattern::{Flow, TestPattern};
use crate::ui::canvas::Canvas;

/// Runs until shutdown. The UI thread sleeps whenever nothing changes:
/// it redraws only after an event or a running animation marks it dirty.
pub fn run(
    display: &mut dyn Display,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    started: Instant,
) -> anyhow::Result<()> {
    let mut screen = TestPattern::default();
    let mut canvas = Canvas::new();
    let mut dirty = true;
    let mut first_frame = true;

    loop {
        display.pump(tx)?;

        // Block only when nothing is pending and the backend does not need
        // pumping. `recv` cannot fail: we hold a sender ourselves.
        let first = if dirty || display.needs_polling() {
            rx.try_recv().ok()
        } else {
            rx.recv().ok()
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
                    match screen.handle(input.action, Instant::now()) {
                        Flow::Redraw => dirty = true,
                        Flow::Quit => return Ok(()),
                    }
                }
            }
        }

        if screen.animate(Instant::now()) {
            dirty = true;
        }

        if dirty {
            screen.draw(&mut canvas);
            display.present(&canvas)?;
            dirty = false;
            if first_frame {
                first_frame = false;
                log::info!("first frame after {} ms", started.elapsed().as_millis());
            }
        }
    }
}
