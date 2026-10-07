//! Home screen: a horizontal carousel with one app panel per position.
//!
//! The focused panel is full size in the centre; its neighbours are smaller,
//! dimmed and peek in from the screen edges, so it is obvious that you can
//! move sideways (kept from the Python prototype). ←/→ slide, Enter opens.
//! Trying to move past either end gives a small bump instead of wrapping, so
//! you always know where you are.

use std::f32::consts::PI;
use std::time::Instant;

use crate::app::action::Action;
use crate::app::anim::{Ease, Tween};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, WIDTH, rgb, rgba};
use crate::ui::icons;
use crate::ui::nav::{Nav, Screen};
use crate::ui::text::Align;
use crate::ui::theme::{color, motion, space, typography};
use crate::ui::widgets::status_bar;

use super::registry::{AppEntry, apps};

const CARD_W: f32 = 440.0;
const CARD_H: f32 = 260.0;
const CARD_CENTRE_Y: f32 = 242.0;
/// Centre-to-centre distance between panels.
const SPACING: f32 = 520.0;
/// Neighbours (one step away) are drawn at this scale and opacity.
const NEIGHBOUR_SCALE: f32 = 0.8;
const NEIGHBOUR_ALPHA: f32 = 0.35;
const ICON_SIZE: f32 = 130.0;
const BUMP_DISTANCE: f32 = 24.0;
const DOT_Y: f32 = 476.0;
const DOT_RADIUS: f32 = 5.0;
const DOT_GAP: f32 = 22.0;

pub struct Launcher {
    apps: Vec<AppEntry>,
    index: usize,
    /// Scroll position in panels (0.0 = first app centred).
    scroll: Tween,
    /// Edge bump: start time and direction (-1 left, +1 right).
    bump: Option<(Instant, f32)>,
    // Sampled by `animate`, read by `draw`.
    pos: f32,
    bump_offset: f32,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            apps: apps(),
            index: 0,
            scroll: Tween::at(0.0),
            bump: None,
            pos: 0.0,
            bump_offset: 0.0,
        }
    }

    fn step(&mut self, dir: isize, now: Instant) {
        let target = self.index as isize + dir;
        if !(0..self.apps.len() as isize).contains(&target) {
            self.bump = Some((now, dir as f32));
            return;
        }
        self.index = target as usize;
        self.scroll
            .retarget(self.index as f32, now, motion::PANEL, Ease::OutCubic);
    }
}

impl Screen for Launcher {
    fn handle(&mut self, action: Action, now: Instant) -> Nav {
        match action {
            Action::Left => self.step(-1, now),
            Action::Right => self.step(1, now),
            Action::Select => {
                let app = &self.apps[self.index];
                log::info!("open {}", app.name);
                return Nav::Push((app.open)(app));
            }
            _ => return Nav::Unhandled,
        }
        Nav::Stay
    }

    fn animate(&mut self, now: Instant) -> bool {
        let pos = self.scroll.value(now);
        let bump_offset = match self.bump {
            Some((start, dir)) => {
                let t =
                    now.saturating_duration_since(start).as_secs_f32() / motion::BUMP.as_secs_f32();
                if t >= 1.0 {
                    self.bump = None;
                    0.0
                } else {
                    // Out and back: a half sine wave against the edge.
                    -dir * BUMP_DISTANCE * (PI * t).sin()
                }
            }
            None => 0.0,
        };
        let changed = pos != self.pos || bump_offset != self.bump_offset;
        self.pos = pos;
        self.bump_offset = bump_offset;
        changed
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        status_bar::draw(c, cx.theme, cx.status, None);

        // Farthest panels first so the focused one is drawn on top.
        let mut order: Vec<(usize, f32)> = (0..self.apps.len())
            .map(|i| (i, i as f32 - self.pos))
            .filter(|(_, d)| d.abs() < 1.8)
            .collect();
        order.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));

        for (i, d) in order {
            self.draw_panel(c, &self.apps[i], d);
        }
        self.draw_dots(c);
    }
}

impl Launcher {
    /// Draws one panel `d` positions from the centre (fractional while sliding).
    fn draw_panel(&self, c: &mut Canvas, app: &AppEntry, d: f32) {
        let k = d.abs().min(1.0);
        let scale = 1.0 - (1.0 - NEIGHBOUR_SCALE) * k;
        let alpha = 1.0 - (1.0 - NEIGHBOUR_ALPHA) * k;
        // Names fade out by the halfway point: only the centred app is named
        // at rest (neighbours would be clipped at the edges), and names
        // cross-fade while sliding.
        let label_alpha = (1.0 - 2.0 * k).max(0.0);

        let centre_x = WIDTH as f32 / 2.0 + d * SPACING + self.bump_offset;
        let (w, h) = (CARD_W * scale, CARD_H * scale);
        c.fill_rounded_rect(
            centre_x - w / 2.0,
            CARD_CENTRE_Y - h / 2.0,
            w,
            h,
            space::RADIUS * 1.5 * scale,
            rgba(app.accent, alpha),
        );
        icons::draw(
            c,
            app.icon,
            centre_x,
            CARD_CENTRE_Y,
            ICON_SIZE * scale,
            rgba(color::TEXT, alpha),
        );
        c.text(
            typography::TITLE,
            app.name,
            centre_x,
            CARD_CENTRE_Y + CARD_H / 2.0 + space::M,
            Align::Center,
            rgba(color::TEXT, label_alpha),
        );
    }

    fn draw_dots(&self, c: &mut Canvas) {
        let n = self.apps.len();
        let first = WIDTH as f32 / 2.0 - (n - 1) as f32 * DOT_GAP / 2.0;
        for i in 0..n {
            let active = i == self.index;
            let fill = if active {
                color::TEXT
            } else {
                color::TEXT_DISABLED
            };
            c.fill_circle(first + i as f32 * DOT_GAP, DOT_Y, DOT_RADIUS, rgb(fill));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    fn render(l: &Launcher) -> Canvas {
        let mut c = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
        };
        l.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        c
    }

    #[test]
    fn slides_between_apps_and_stops_at_the_ends() {
        let mut l = Launcher::new();
        let t0 = Instant::now();
        assert!(matches!(l.handle(Action::Left, t0), Nav::Stay));
        assert_eq!(l.index, 0, "no wrap at the start");
        assert!(l.bump.is_some(), "edge bump instead");

        l.handle(Action::Right, t0);
        assert_eq!(l.index, 1);
        assert!(l.animate(t0 + motion::PANEL / 2));
        assert!(l.pos > 0.0 && l.pos < 1.0, "sliding");
        l.animate(t0 + motion::PANEL);
        assert_eq!(l.pos, 1.0);

        for _ in 0..10 {
            l.handle(Action::Right, t0);
        }
        assert_eq!(l.index, l.apps.len() - 1, "no wrap at the end");
    }

    #[test]
    fn bump_returns_to_rest() {
        let mut l = Launcher::new();
        let t0 = Instant::now();
        l.handle(Action::Left, t0);
        l.animate(t0 + motion::BUMP / 2);
        assert!(l.bump_offset > 0.0, "nudged right when pushing left");
        assert!(l.animate(t0 + motion::BUMP), "final frame");
        assert_eq!(l.bump_offset, 0.0);
        assert!(!l.animate(t0 + motion::BUMP * 2), "at rest");
    }

    #[test]
    fn select_opens_the_focused_app_and_back_is_left_to_the_navigator() {
        let mut l = Launcher::new();
        let t0 = Instant::now();
        assert!(matches!(l.handle(Action::Select, t0), Nav::Push(_)));
        assert!(matches!(l.handle(Action::Back, t0), Nav::Unhandled));
    }

    #[test]
    fn snapshot_launcher_at_rest() {
        let mut l = Launcher::new();
        let t0 = Instant::now();
        l.handle(Action::Right, t0);
        l.animate(t0 + Duration::from_secs(1));
        assert_snapshot("launcher", &render(&l));
    }

    #[test]
    fn snapshot_launcher_mid_slide() {
        let mut l = Launcher::new();
        let t0 = Instant::now();
        l.handle(Action::Right, t0);
        l.animate(t0 + motion::PANEL / 3);
        assert_snapshot("launcher-sliding", &render(&l));
    }
}
