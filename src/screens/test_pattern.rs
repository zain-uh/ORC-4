//! M0 placeholder screen: proves window → keymap → Action → redraw works.
//! Arrow keys flash the matching triangle, Enter flashes the centre square,
//! Esc quits. Replaced by the launcher in milestone M2.

use std::time::{Duration, Instant};

use tiny_skia::{Color, Path, PathBuilder};

use crate::app::action::Action;
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};

const BACKGROUND: u32 = 0x000000;
const SURFACE: u32 = 0x1A1A1A;
const DIM: u32 = 0x5C5C5C;
const BRIGHT: u32 = 0xF2F2F2;

/// Default safe-area insets (composite overscan); calibratable from M4.
const SAFE_X: f32 = 56.0;
const SAFE_Y: f32 = 40.0;

/// A press stays fully lit for `HOLD`, then fades out over `FADE`. Key
/// auto-repeat (first repeat after 350 ms) re-triggers it, so a held key
/// stays lit and fades once released.
const HOLD: Duration = Duration::from_millis(150);
const FADE: Duration = Duration::from_millis(350);

pub enum Flow {
    Redraw,
    Quit,
}

#[derive(Default)]
pub struct TestPattern {
    last: Option<Action>,
    lit_at: Option<Instant>,
    /// 1.0 = fully lit, 0.0 = idle. Updated by `animate`, read by `draw`.
    level: f32,
}

impl TestPattern {
    pub fn handle(&mut self, action: Action, now: Instant) -> Flow {
        if action == Action::Back {
            return Flow::Quit;
        }
        self.last = Some(action);
        self.lit_at = Some(now);
        self.level = 1.0;
        Flow::Redraw
    }

    /// Advances the fade. Returns true while a new frame is needed.
    pub fn animate(&mut self, now: Instant) -> bool {
        let Some(start) = self.lit_at else {
            return false;
        };
        let elapsed = now.saturating_duration_since(start);
        if elapsed < HOLD {
            self.level = 1.0;
        } else if elapsed < HOLD + FADE {
            let t = (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32();
            self.level = 1.0 - t * t * (3.0 - 2.0 * t); // smoothstep
        } else {
            // Final frame at rest; after this the loop can sleep again.
            self.level = 0.0;
            self.lit_at = None;
        }
        true
    }

    pub fn draw(&self, c: &mut Canvas) {
        c.clear(rgb(BACKGROUND));

        let (w, h) = (WIDTH as f32, HEIGHT as f32);
        c.stroke_rect(
            SAFE_X,
            SAFE_Y,
            w - 2.0 * SAFE_X,
            h - 2.0 * SAFE_Y,
            2.0,
            rgb(DIM),
        );

        let level_for = |a: Action| {
            if self.last == Some(a) {
                self.level
            } else {
                0.0
            }
        };

        let (cx, cy) = (w / 2.0, h / 2.0);
        let centre = mix(SURFACE, BRIGHT, level_for(Action::Select));
        c.fill_rect(cx - 60.0, cy - 60.0, 120.0, 120.0, centre);

        // Triangles point outward, 110 px from the centre.
        let (gap, size) = (110.0, 50.0);
        for (action, (dx, dy)) in [
            (Action::Up, (0.0, -1.0)),
            (Action::Down, (0.0, 1.0)),
            (Action::Left, (-1.0, 0.0)),
            (Action::Right, (1.0, 0.0)),
        ] {
            if let Some(tri) = triangle(cx + dx * gap, cy + dy * gap, dx, dy, size) {
                c.fill_path(&tri, mix(DIM, BRIGHT, level_for(action)));
            }
        }
    }
}

/// Linear blend between two `0xRRGGBB` colours (t = 0 → `a`, t = 1 → `b`).
fn mix(a: u32, b: u32, t: f32) -> Color {
    let ch = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        (x + (y - x) * t).round() as u8
    };
    Color::from_rgba8(ch(16), ch(8), ch(0), 0xFF)
}

/// Isosceles triangle with its base centred on (x, y), pointing along (dx, dy).
fn triangle(x: f32, y: f32, dx: f32, dy: f32, size: f32) -> Option<Path> {
    let (px, py) = (-dy, dx); // perpendicular
    let mut pb = PathBuilder::new();
    pb.move_to(x + px * size, y + py * size);
    pb.line_to(x - px * size, y - py * size);
    pb.line_to(x + dx * size, y + dy * size);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RGB of a point inside the right-hand triangle.
    fn right_triangle_rgb(s: &TestPattern) -> (u8, u8, u8) {
        let mut c = Canvas::new();
        s.draw(&mut c);
        let px = c.pixmap().pixel(960 / 2 + 120, 540 / 2).unwrap();
        (px.red(), px.green(), px.blue())
    }

    #[test]
    fn back_quits_and_other_actions_redraw() {
        let mut s = TestPattern::default();
        let now = Instant::now();
        assert!(matches!(s.handle(Action::Left, now), Flow::Redraw));
        assert_eq!(s.last, Some(Action::Left));
        assert!(matches!(s.handle(Action::Back, now), Flow::Quit));
    }

    #[test]
    fn press_lights_then_fades_back_to_dim() {
        let mut s = TestPattern::default();
        let t0 = Instant::now();
        s.handle(Action::Right, t0);
        assert_eq!(right_triangle_rgb(&s), (0xF2, 0xF2, 0xF2));

        assert!(s.animate(t0 + HOLD + FADE / 2), "mid-fade needs frames");
        let (r, _, _) = right_triangle_rgb(&s);
        assert!(r > 0x5C && r < 0xF2, "mid-fade is between dim and bright");

        assert!(s.animate(t0 + HOLD + FADE), "one final frame at rest");
        assert_eq!(right_triangle_rgb(&s), (0x5C, 0x5C, 0x5C));
        assert!(!s.animate(t0 + HOLD + FADE * 2), "idle: no more frames");
    }

    #[test]
    fn repeat_press_restarts_the_hold() {
        let mut s = TestPattern::default();
        let t0 = Instant::now();
        s.handle(Action::Right, t0);
        let t1 = t0 + HOLD + FADE / 2;
        s.handle(Action::Right, t1);
        s.animate(t1 + HOLD / 2);
        assert_eq!(s.level, 1.0);
    }
}
