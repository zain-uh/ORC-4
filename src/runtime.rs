//! The main loop: drains events, routes them to the screens, saves settings
//! shortly after they change, and presents a new frame only when something
//! changed.

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::app::action::{Action, Notice, TextEdit};
use crate::config::{self, Paths, Settings};
use crate::event::Event;
use crate::input::remote::link::{LinkOutput, PhoneLink};
use crate::input::remote::server;
use crate::platform::Display;
use crate::screens::launcher::Launcher;
use crate::screens::nav::{Cx, Navigator};
use crate::services::player::PlayerEvent;
use crate::services::{ServiceEvent, Services, clock};
use crate::ui::DrawCx;
use crate::ui::canvas::Canvas;
use crate::ui::theme::Theme;
use crate::ui::widgets::status_bar::Status;

/// Settings are written this long after they first change, so holding an
/// arrow key in the calibration screen doesn't hammer the SD card.
const SAVE_DELAY: Duration = Duration::from_secs(1);

/// One unit of work for the screens, whatever its source.
enum Step {
    Action(Action),
    Text(TextEdit),
    Notice(Notice),
    Service(ServiceEvent),
    Player(PlayerEvent),
}

/// Runs until shutdown. The UI thread sleeps whenever nothing changes: it
/// redraws only after an event, a running animation or a clock minute change.
pub fn run(
    display: &mut dyn Display,
    tx: &Sender<Event>,
    rx: &Receiver<Event>,
    mut settings: Settings,
    paths: &Paths,
    started: Instant,
) -> anyhow::Result<()> {
    let services = Services::start(tx.clone());
    let mut link = PhoneLink::new();
    // The glasses must work without a phone: a failure here is only logged.
    match server::start(&settings.phone.listen, tx.clone()) {
        Ok(addr) => log::info!("phone link listening on {addr}"),
        Err(e) => log::error!("phone link unavailable on {}: {e}", settings.phone.listen),
    }
    let mut saved = settings.clone();
    let mut save_due: Option<Instant> = None;
    let mut status = Status {
        clock: clock::local_hhmm(settings.clock.utc_offset_minutes),
    };
    let mut screens = Navigator::new(Box::new(Launcher::new()));
    screens.start(&mut Cx::new(
        Instant::now(),
        &mut settings,
        &services,
        paths,
        &mut link,
    ));
    let mut canvas = Canvas::new();
    let mut dirty = true;
    let mut first_frame = true;
    // Video owns the display (device only; the simulator plays in a window).
    let mut video_mode = false;
    let mut handed_over = false;

    loop {
        display.pump(tx)?;

        // Block only when idle and the backend does not need pumping; wake
        // for the next clock minute or a pending settings save.
        let first = if dirty || display.needs_polling() {
            rx.try_recv().ok()
        } else {
            let mut wait = clock::until_next_minute();
            if let Some(due) = save_due {
                wait = wait.min(due.saturating_duration_since(Instant::now()));
            }
            rx.recv_timeout(wait).ok()
        };

        for event in first
            .into_iter()
            .chain(std::iter::from_fn(|| rx.try_recv().ok()))
        {
            let now = Instant::now();
            let steps = match event {
                Event::Shutdown => {
                    log::info!("shutdown requested");
                    save_if_changed(paths, &settings, &mut saved);
                    return Ok(());
                }
                Event::Input(input) => {
                    log::debug!(
                        "input {:?} repeat={} from {:?}",
                        input.action,
                        input.repeat,
                        input.source
                    );
                    vec![Step::Action(input.action)]
                }
                Event::Text(edit) => vec![Step::Text(edit)],
                Event::Service(result) => vec![Step::Service(result)],
                Event::Player(event) => vec![Step::Player(event)],
                // Phone traffic becomes the same steps a keyboard produces.
                Event::Remote(remote) => {
                    let outputs = link.handle(remote, &mut settings, now);
                    outputs
                        .into_iter()
                        .filter_map(|output| match output {
                            LinkOutput::Input(action) => Some(Step::Action(action)),
                            LinkOutput::Text(edit) => Some(Step::Text(edit)),
                            LinkOutput::Notice(notice) => Some(Step::Notice(notice)),
                            LinkOutput::Time {
                                unix,
                                utc_offset_minutes,
                            } => {
                                clock::set_time(unix);
                                settings.clock.utc_offset_minutes = utc_offset_minutes;
                                None
                            }
                        })
                        .collect()
                }
            };

            let mut cx = Cx::new(now, &mut settings, &services, paths, &mut link);
            for step in steps {
                dirty |= match step {
                    Step::Action(action) => screens.handle(action, &mut cx),
                    Step::Text(edit) => screens.text(&edit, &mut cx),
                    Step::Notice(notice) => screens.notify(&notice, &mut cx),
                    Step::Service(result) => screens.dispatch(result, &mut cx),
                    Step::Player(event) => screens.player(&event, &mut cx),
                };
            }
            if cx.exit_requested() {
                log::info!("exit requested");
                save_if_changed(paths, &settings, &mut saved);
                return Ok(());
            }
        }

        // Keep phones in step: drop connections settings no longer allow, and
        // tell them whether a text field is open.
        link.enforce(&settings);
        link.set_text_request(screens.text_request());

        let now = Instant::now();
        if settings != saved {
            // Changes come from input, which already marked the frame dirty.
            let due = *save_due.get_or_insert(now + SAVE_DELAY);
            if now >= due {
                save_if_changed(paths, &settings, &mut saved);
                save_due = None;
            }
        }

        let now_clock = clock::local_hhmm(settings.clock.utc_offset_minutes);
        if now_clock != status.clock {
            status.clock = now_clock;
            dirty = true;
        }
        dirty |= screens.animate(now);

        let want_video = screens.takes_display();
        if want_video != video_mode {
            video_mode = want_video;
            if want_video {
                handed_over = display.release_for_video()?;
            } else if handed_over {
                display.reclaim()?;
                handed_over = false;
                dirty = true;
            }
        }

        if dirty && !handed_over {
            let theme = Theme::with_edges(settings.display.edge_x, settings.display.edge_y);
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

fn save_if_changed(paths: &Paths, settings: &Settings, saved: &mut Settings) {
    if settings == saved {
        return;
    }
    match config::save_user(paths, settings) {
        Ok(()) => {
            log::info!("settings saved");
            *saved = settings.clone();
        }
        // Keep running with the in-memory values; the next change retries.
        Err(e) => log::error!("saving settings failed: {e:#}"),
    }
}
