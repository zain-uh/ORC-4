use crate::app::action::InputEvent;
use crate::services::ServiceEvent;

/// Everything that can wake the UI thread. All producers (input sources,
/// the background worker) hold a clone of the same `Sender<Event>`.
#[derive(Debug)]
pub enum Event {
    Input(InputEvent),
    Service(ServiceEvent),
    /// Stop the main loop cleanly (window closed, power off requested).
    Shutdown,
}
