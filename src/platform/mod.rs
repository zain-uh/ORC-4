//! Display backends. The rest of the app only sees the `Display` trait.

#[cfg(feature = "simulator")]
mod composite;
#[cfg(feature = "simulator")]
mod window;

use std::sync::mpsc::Sender;

use crate::config::Settings;
use crate::event::Event;
use crate::ui::canvas::Canvas;

pub trait Display {
    /// Shows a finished frame. Paced to the output's refresh rate.
    fn present(&mut self, canvas: &Canvas) -> anyhow::Result<()>;

    /// Desktop windows must be serviced from the UI thread. They also deliver
    /// their keyboard input and close events through here.
    fn pump(&mut self, _events: &Sender<Event>) -> anyhow::Result<()> {
        Ok(())
    }

    /// True if `pump` must be called continuously (it then paces the loop).
    /// False for backends where the loop can block until an event arrives.
    fn needs_polling(&self) -> bool {
        false
    }

    /// Hands the physical display to the video player (mpv draws to it
    /// directly). Returns true if it was handed over, in which case the UI
    /// must not present until `reclaim`. Backends where video plays in its
    /// own window (the simulator) keep the UI and return false.
    fn release_for_video(&mut self) -> anyhow::Result<bool> {
        Ok(false)
    }

    /// Takes the display back after video; the next frame repaints it.
    fn reclaim(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn open(settings: &Settings) -> anyhow::Result<Box<dyn Display>> {
    #[cfg(feature = "simulator")]
    {
        Ok(Box::new(window::WindowDisplay::new(settings)?))
    }
    #[cfg(not(feature = "simulator"))]
    {
        let _ = settings;
        anyhow::bail!("no display backend compiled in (the KMS backend arrives in milestone M3)")
    }
}
