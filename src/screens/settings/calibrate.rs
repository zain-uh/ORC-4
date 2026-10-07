//! Settings → Display → Screen edges.
//!
//! The AV driver board crops an unknown amount of the picture. This screen
//! draws a frame at the current safe area; the user moves its edges until all
//! four are just visible. Changes apply live (every screen uses the same
//! safe area); Enter keeps them, Back restores the previous values.

use crate::app::action::Action;
use crate::config::DisplaySettings;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};

use crate::screens::nav::{Cx, Nav, Screen};

const STEP: f32 = 4.0;
const CORNER: f32 = 36.0;
const LINE: f32 = 4.0;

pub struct Calibrate {
    original: (f32, f32),
    edge_x: f32,
    edge_y: f32,
}

impl Calibrate {
    pub fn new(cx: &Cx) -> Self {
        let d = &cx.settings.display;
        Self {
            original: (d.edge_x, d.edge_y),
            edge_x: d.edge_x,
            edge_y: d.edge_y,
        }
    }
}

impl Screen for Calibrate {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        let clamp = |v: f32| v.clamp(0.0, DisplaySettings::EDGE_MAX);
        match action {
            // Arrows move the frame's edges outward/inward: → wider, ← narrower,
            // ↑ taller, ↓ shorter.
            Action::Right => self.edge_x = clamp(self.edge_x - STEP),
            Action::Left => self.edge_x = clamp(self.edge_x + STEP),
            Action::Up => self.edge_y = clamp(self.edge_y - STEP),
            Action::Down => self.edge_y = clamp(self.edge_y + STEP),
            Action::Select => {
                cx.toast("Screen edges saved");
                return Nav::Pop;
            }
            Action::Back => {
                (self.edge_x, self.edge_y) = self.original;
                cx.settings.display.edge_x = self.edge_x;
                cx.settings.display.edge_y = self.edge_y;
                return Nav::Pop;
            }
            _ => return Nav::Unhandled,
        }
        cx.settings.display.edge_x = self.edge_x;
        cx.settings.display.edge_y = self.edge_y;
        Nav::Stay
    }

    fn draw(&self, c: &mut Canvas, _cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let (x, y) = (self.edge_x, self.edge_y);
        let (w, h) = (WIDTH as f32 - 2.0 * x, HEIGHT as f32 - 2.0 * y);
        let white = rgb(color::TEXT);

        // Thin frame plus heavy corner marks: the corners are what to look for.
        c.stroke_rect(x, y, w, h, 2.0, rgb(color::TEXT_SECONDARY));
        for (cx_, cy_, dx, dy) in [
            (x, y, 1.0, 1.0),
            (x + w, y, -1.0, 1.0),
            (x, y + h, 1.0, -1.0),
            (x + w, y + h, -1.0, -1.0),
        ] {
            let (hx, vy) = (
                if dx > 0.0 { cx_ } else { cx_ - CORNER },
                if dy > 0.0 { cy_ } else { cy_ - LINE },
            );
            c.fill_rect(hx, vy, CORNER, LINE, white);
            let (vx, hy) = (
                if dx > 0.0 { cx_ } else { cx_ - LINE },
                if dy > 0.0 { cy_ } else { cy_ - CORNER },
            );
            c.fill_rect(vx, hy, LINE, CORNER, white);
        }

        let mid = WIDTH as f32 / 2.0;
        let mut ty = HEIGHT as f32 / 2.0 - 110.0;
        c.text(
            typography::TITLE,
            "Screen edges",
            mid,
            ty,
            Align::Center,
            white,
        );
        ty += fonts().line_metrics(typography::TITLE).height + space::S;
        ty += c.paragraph(
            typography::BODY,
            "Adjust until all four corners are visible.",
            x + space::L,
            ty,
            w - 2.0 * space::L,
            2,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
        ty += space::S;
        c.text(
            typography::NUMERIC,
            &format!("{x:.0} × {y:.0}"),
            mid,
            ty,
            Align::Center,
            white,
        );
        let hint = typography::CAPTION;
        c.text(
            hint,
            "Arrows move edges · Enter save · Esc cancel",
            mid,
            y + h - space::M - fonts().line_metrics(hint).height,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;

    #[test]
    fn arrows_adjust_live_and_back_restores() {
        let mut h = Harness::new();
        let mut cal = Calibrate::new(&h.cx(Instant::now()));
        let mut cx = h.cx(Instant::now());
        cal.handle(Action::Right, &mut cx);
        cal.handle(Action::Up, &mut cx);
        assert_eq!(cx.settings.display.edge_x, 52.0, "wider = smaller inset");
        assert_eq!(cx.settings.display.edge_y, 36.0);
        assert!(matches!(cal.handle(Action::Back, &mut cx), Nav::Pop));
        assert_eq!(cx.settings.display.edge_x, 56.0, "restored");
    }

    #[test]
    fn edges_are_clamped() {
        let mut h = Harness::new();
        let mut cal = Calibrate::new(&h.cx(Instant::now()));
        let mut cx = h.cx(Instant::now());
        for _ in 0..100 {
            cal.handle(Action::Right, &mut cx);
            cal.handle(Action::Down, &mut cx);
        }
        assert_eq!(cx.settings.display.edge_x, 0.0);
        assert_eq!(cx.settings.display.edge_y, DisplaySettings::EDGE_MAX);
    }
}
