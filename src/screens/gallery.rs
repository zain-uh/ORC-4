//! M1 component gallery: shows the toolkit (status bar, type scale, wrapped
//! text, cards, focus ring) so it can be judged on the PC and, in M3, on the
//! real composite display. ←/→ move focus, Enter selects, Esc quits.
//! Replaced by the launcher in milestone M2.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::anim::{Ease, Tween};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, motion, space, typography};
use crate::ui::widgets::status_bar;

const CARD_W: f32 = 248.0;
const CARD_H: f32 = 150.0;
const CARD_Y: f32 = 290.0;

struct Card {
    label: &'static str,
    fill: u32,
}

const CARDS: [Card; 3] = [
    Card {
        label: "Surface",
        fill: color::SURFACE,
    },
    Card {
        label: "Raised surface",
        fill: color::SURFACE_RAISED,
    },
    Card {
        label: "Accent colour on large areas only",
        fill: color::YOUTUBE,
    },
];

pub enum Flow {
    Stay,
    Redraw,
    Quit,
}

pub struct Gallery {
    focus: usize,
    selected: Option<usize>,
    /// Focus ring x position (left edge of the focused card).
    ring: Tween,
    /// `ring` sampled at the last `animate`; what `draw` uses.
    ring_x: f32,
}

impl Gallery {
    pub fn new() -> Self {
        let x = card_x(0);
        Self {
            focus: 0,
            selected: None,
            ring: Tween::at(x),
            ring_x: x,
        }
    }

    pub fn handle(&mut self, action: Action, now: Instant) -> Flow {
        let target = match action {
            Action::Back => return Flow::Quit,
            Action::Select => {
                self.selected = Some(self.focus);
                return Flow::Redraw;
            }
            Action::Left => self.focus.saturating_sub(1),
            Action::Right => (self.focus + 1).min(CARDS.len() - 1),
            _ => return Flow::Stay,
        };
        if target == self.focus {
            return Flow::Stay;
        }
        self.focus = target;
        self.ring
            .retarget(card_x(target), now, motion::FOCUS, Ease::OutCubic);
        Flow::Redraw
    }

    /// Advances animations. Returns true if the picture changed.
    pub fn animate(&mut self, now: Instant) -> bool {
        let x = self.ring.value(now);
        let changed = x != self.ring_x;
        self.ring_x = x;
        changed
    }

    pub fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        status_bar::draw(c, t, cx.status, Some("UI gallery"));

        let left = t.safe.left;
        let width = WIDTH as f32 - t.safe.left - t.safe.right;
        let mut y = t.safe.top + space::STATUS_BAR_HEIGHT + space::S;

        c.text(
            typography::TITLE,
            "Readable at a glance",
            left,
            y,
            Align::Left,
            rgb(color::TEXT),
        );
        y += fonts().line_metrics(typography::TITLE).height + space::S;

        c.paragraph(
            typography::BODY,
            "White text on black survives composite video. Colour is kept for large \
             areas, and nothing is thinner than two pixels.",
            left,
            y,
            width,
            2,
            Align::Left,
            rgb(color::TEXT_SECONDARY),
        );

        for (i, card) in CARDS.iter().enumerate() {
            let x = card_x(i);
            c.fill_rounded_rect(x, CARD_Y, CARD_W, CARD_H, space::RADIUS, rgb(card.fill));

            let style = typography::CAPTION;
            let inner = CARD_W - 2.0 * space::M;
            let label = fonts().ellipsize(style, card.label, inner);
            let lh = fonts().line_metrics(style).height;
            c.text(
                style,
                &label,
                x + space::M,
                CARD_Y + CARD_H - space::M - lh,
                Align::Left,
                rgb(color::TEXT),
            );
            if self.selected == Some(i) {
                c.text(
                    style,
                    "Selected",
                    x + space::M,
                    CARD_Y + space::M,
                    Align::Left,
                    rgb(color::TEXT),
                );
            }
        }

        // Focus ring: centred on a path outside the card by FOCUS_GAP.
        let grow = space::FOCUS_GAP + space::FOCUS_WIDTH / 2.0;
        c.stroke_rounded_rect(
            self.ring_x - grow,
            CARD_Y - grow,
            CARD_W + 2.0 * grow,
            CARD_H + 2.0 * grow,
            space::RADIUS + grow,
            space::FOCUS_WIDTH,
            rgb(color::FOCUS),
        );

        let hint_style = typography::CAPTION;
        let hint_y = HEIGHT as f32 - t.safe.bottom - fonts().line_metrics(hint_style).height;
        c.text(
            hint_style,
            "← → move    Enter select    Esc quit",
            WIDTH as f32 / 2.0,
            hint_y,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
    }
}

/// Left edge of card `i`; the row is centred on the screen.
fn card_x(i: usize) -> f32 {
    let row = CARDS.len() as f32 * CARD_W + (CARDS.len() - 1) as f32 * space::M;
    (WIDTH as f32 - row) / 2.0 + i as f32 * (CARD_W + space::M)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn focus_moves_within_bounds_and_animates() {
        let mut g = Gallery::new();
        let t0 = Instant::now();
        assert!(
            matches!(g.handle(Action::Left, t0), Flow::Stay),
            "already at first card"
        );
        assert!(matches!(g.handle(Action::Right, t0), Flow::Redraw));
        assert_eq!(g.focus, 1);

        assert!(g.animate(t0 + motion::FOCUS / 2), "ring moving");
        assert!(g.ring_x > card_x(0) && g.ring_x < card_x(1));
        assert!(g.animate(t0 + motion::FOCUS), "final frame");
        assert_eq!(g.ring_x, card_x(1));
        assert!(!g.animate(t0 + motion::FOCUS * 2), "at rest");
    }

    #[test]
    fn back_quits() {
        assert!(matches!(
            Gallery::new().handle(Action::Back, Instant::now()),
            Flow::Quit
        ));
    }

    #[test]
    fn snapshot_gallery() {
        let mut g = Gallery::new();
        let t0 = Instant::now();
        g.handle(Action::Right, t0);
        g.handle(Action::Select, t0);
        g.animate(t0 + Duration::from_secs(1));

        let mut c = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
        };
        g.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("gallery", &c);
    }
}
