//! Rendering toolkit. Knows pixels, not hardware.

pub mod canvas;
pub mod icons;
pub mod nav;
#[cfg(test)]
pub mod snapshot;
pub mod text;
pub mod theme;
pub mod widgets;

use theme::Theme;
use widgets::status_bar::Status;

/// Read-only context every screen gets when drawing.
pub struct DrawCx<'a> {
    pub theme: &'a Theme,
    pub status: &'a Status,
}
