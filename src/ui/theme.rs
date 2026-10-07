//! Design tokens. These encode the composite-video rules from
//! docs/phase2-architecture.md §1.1 and §6:
//! - black background, luma-only contrast for text (colour smears)
//! - text never below 24 px, strokes never below 2 px (interlace flicker)
//! - colour accents only on large areas
//!
//! The full token set is the design spec, so some tokens are defined before
//! any screen uses them.
#![allow(dead_code)]

use std::time::Duration;

use super::text::{FontId, TextStyle};

pub mod color {
    pub const BACKGROUND: u32 = 0x000000;
    pub const SURFACE: u32 = 0x1A1A1A;
    pub const SURFACE_RAISED: u32 = 0x2A2A2A;
    pub const TEXT: u32 = 0xF2F2F2;
    pub const TEXT_SECONDARY: u32 = 0xA6A6A6;
    pub const TEXT_DISABLED: u32 = 0x5C5C5C;
    pub const FOCUS: u32 = 0xF2F2F2;

    // App accents: large areas only, never text.
    pub const YOUTUBE: u32 = 0xC4302B;
    pub const SEARCH: u32 = 0x3B6FD8;
    pub const FILES: u32 = 0xD99A2B;
    pub const SETTINGS: u32 = 0x6E6E73;
}

pub mod typography {
    use super::{FontId, TextStyle};

    pub const DISPLAY: TextStyle = TextStyle::new(FontId::Sans, 72.0);
    pub const TITLE: TextStyle = TextStyle::new(FontId::Sans, 44.0);
    pub const BODY: TextStyle = TextStyle::new(FontId::Sans, 32.0);
    pub const CAPTION: TextStyle = TextStyle::new(FontId::Sans, 26.0);
    /// Clock, counters, sizes: tabular figures that don't jitter.
    pub const NUMERIC: TextStyle = TextStyle::new(FontId::Mono, 26.0);

    /// Composite video cannot resolve anything smaller.
    pub const MIN_SIZE: f32 = 24.0;
    /// Line spacing as a multiple of the font size.
    pub const LINE_HEIGHT: f32 = 1.3;
}

pub mod space {
    pub const XS: f32 = 8.0;
    pub const S: f32 = 16.0;
    pub const M: f32 = 24.0;
    pub const L: f32 = 40.0;
    pub const XL: f32 = 64.0;

    pub const RADIUS: f32 = 20.0;
    /// Focus ring: stroke width and gap from the focused element.
    pub const FOCUS_WIDTH: f32 = 4.0;
    pub const FOCUS_GAP: f32 = 6.0;
    pub const STATUS_BAR_HEIGHT: f32 = 44.0;
}

pub mod motion {
    use super::Duration;

    pub const PANEL: Duration = Duration::from_millis(220);
    pub const FOCUS: Duration = Duration::from_millis(120);
    /// Whole-screen dip to black between screens (push/pop).
    pub const SCREEN: Duration = Duration::from_millis(200);
    /// Nudge when trying to scroll past the first or last panel.
    pub const BUMP: Duration = Duration::from_millis(180);
}

/// Edge insets the board's analog overscan may crop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// Per-device values (calibrated from Settings in M4). Constant tokens live
/// in the modules above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub safe: Insets,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            safe: Insets {
                left: 56.0,
                right: 56.0,
                top: 40.0,
                bottom: 40.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::typography::*;

    #[test]
    fn no_text_style_below_composite_minimum() {
        for s in [DISPLAY, TITLE, BODY, CAPTION, NUMERIC] {
            assert!(s.size >= MIN_SIZE, "{s:?} is too small for composite video");
        }
    }
}
