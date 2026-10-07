use super::action::InputEvent;

/// Everything that can wake the UI thread. All producers (input sources,
/// background services) hold a clone of the same `Sender<Event>`.
#[derive(Debug)]
pub enum Event {
    Input(InputEvent),
    /// Stop the main loop cleanly (window closed, power off requested).
    Shutdown,
}
