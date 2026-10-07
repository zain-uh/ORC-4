use crate::app::action::{InputEvent, TextEdit};
use crate::input::remote::server::RemoteEvent;
use crate::services::ServiceEvent;

/// Everything that can wake the UI thread. All producers (input sources,
/// phone connections, the background worker) hold a clone of the same
/// `Sender<Event>`.
#[derive(Debug)]
pub enum Event {
    Input(InputEvent),
    /// Typing from a physical keyboard (phone typing arrives via `Remote`).
    Text(TextEdit),
    Remote(RemoteEvent),
    Service(ServiceEvent),
    /// Stop the main loop cleanly (window closed, power off requested).
    Shutdown,
}
