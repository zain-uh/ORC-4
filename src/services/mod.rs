//! Background work and OS facts. Never touches the UI.
//!
//! Slow work runs as a job on a worker thread, so the UI thread never
//! blocks. Each job gets a `Ticket`; its result comes back as
//! `Event::Service`. A screen remembers the ticket it is waiting for and
//! ignores any other, so results that arrive after the user moved on are
//! dropped instead of corrupting state.
//!
//! Two pools: one thread for disk and CPU work (in order), and three for
//! network work, so a slow search never holds up thumbnails or files.

pub mod clock;
pub mod http;
pub mod storage;
pub mod system;
pub mod youtube;

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::event::Event;
use storage::{DirEntry, Image};
use youtube::Video;

const NETWORK_THREADS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

#[derive(Debug)]
pub enum Reply {
    Dir(Vec<DirEntry>),
    Lines(Vec<String>),
    Image(Image),
    Videos(Vec<Video>),
    Done,
}

/// Failure categories with display-ready wording. Details go to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserError {
    NotFound,
    NotEmpty,
    TooLarge,
    Unsupported,
    Storage,
    Network,
    /// A helper program (yt-dlp) isn't installed.
    MissingTool,
    Internal,
}

impl UserError {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotFound => "It's no longer there",
            Self::NotEmpty => "Folder isn't empty",
            Self::TooLarge => "Too large to open",
            Self::Unsupported => "Can't open this type of file",
            Self::Storage => "Storage error",
            Self::Network => "Can't reach the internet",
            Self::MissingTool => "YouTube support (yt-dlp) isn't installed",
            Self::Internal => "Something went wrong",
        }
    }
}

pub type JobResult = Result<Reply, UserError>;

#[derive(Debug)]
pub struct ServiceEvent {
    pub ticket: Ticket,
    pub result: JobResult,
}

type Job = Box<dyn FnOnce() -> JobResult + Send>;
type Queue = Sender<(Ticket, Job)>;

/// Handle used by screens (UI thread only) to queue background work.
pub struct Services {
    disk: Queue,
    net: Queue,
    next: Cell<u64>,
}

impl Services {
    pub fn start(events: Sender<Event>) -> Self {
        Self {
            disk: spawn_pool("worker", 1, events.clone()),
            net: spawn_pool("net", NETWORK_THREADS, events),
            next: Cell::new(1),
        }
    }

    /// Disk or CPU work (file listing, decoding, text layout). Runs in order.
    pub fn run(&self, job: impl FnOnce() -> JobResult + Send + 'static) -> Ticket {
        self.queue(&self.disk, Box::new(job))
    }

    /// Network work (searches, downloads). Several run at once.
    pub fn run_net(&self, job: impl FnOnce() -> JobResult + Send + 'static) -> Ticket {
        self.queue(&self.net, Box::new(job))
    }

    fn queue(&self, queue: &Queue, job: Job) -> Ticket {
        let ticket = Ticket(self.next.get());
        self.next.set(ticket.0 + 1);
        if queue.send((ticket, job)).is_err() {
            log::error!("worker threads are gone; job {ticket:?} dropped");
        }
        ticket
    }
}

fn spawn_pool(name: &str, threads: usize, events: Sender<Event>) -> Queue {
    let (queue, inbox) = mpsc::channel::<(Ticket, Job)>();
    let inbox: Arc<Mutex<Receiver<(Ticket, Job)>>> = Arc::new(Mutex::new(inbox));
    for i in 0..threads {
        let (inbox, events) = (Arc::clone(&inbox), events.clone());
        thread::Builder::new()
            .name(format!("{name}-{i}"))
            .spawn(move || {
                loop {
                    // Hold the lock only while taking the next job.
                    let next = inbox.lock().ok().and_then(|rx| rx.recv().ok());
                    let Some((ticket, job)) = next else { break };
                    // A panicking job must not kill the worker for everyone.
                    let result = catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| {
                        log::error!("job {ticket:?} panicked");
                        Err(UserError::Internal)
                    });
                    if events
                        .send(Event::Service(ServiceEvent { ticket, result }))
                        .is_err()
                    {
                        break; // UI is gone
                    }
                }
            })
            .expect("spawn worker thread");
    }
    queue
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn next_service_event(rx: &mpsc::Receiver<Event>) -> ServiceEvent {
        match rx.recv_timeout(Duration::from_secs(5)).expect("event") {
            Event::Service(e) => e,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn jobs_run_in_order_and_report_their_ticket() {
        let (tx, rx) = mpsc::channel();
        let services = Services::start(tx);
        let a = services.run(|| Ok(Reply::Lines(vec!["a".into()])));
        let b = services.run(|| Err(UserError::NotFound));
        let first = next_service_event(&rx);
        assert_eq!(first.ticket, a);
        assert!(matches!(first.result, Ok(Reply::Lines(_))));
        let second = next_service_event(&rx);
        assert_eq!(second.ticket, b);
        assert_eq!(second.result.unwrap_err(), UserError::NotFound);
    }

    #[test]
    fn a_panicking_job_does_not_kill_the_worker() {
        let (tx, rx) = mpsc::channel();
        let services = Services::start(tx);
        services.run(|| panic!("boom"));
        let ok = services.run(|| Ok(Reply::Done));
        assert_eq!(
            next_service_event(&rx).result.unwrap_err(),
            UserError::Internal
        );
        let after = next_service_event(&rx);
        assert_eq!(after.ticket, ok);
        assert!(matches!(after.result, Ok(Reply::Done)));
    }
}
