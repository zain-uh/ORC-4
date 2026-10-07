//! Vertical list with a focus ring, used by Files and Settings.
//!
//! Five 72 px rows are visible: large enough for composite video, few enough
//! to read at a glance. Focus and scrolling animate; a scrollbar appears for
//! longer lists. The list only tracks focus — what a row *does* is up to the
//! screen.
//!
//! Draw the list *before* the status bar and hints: rows sliding out of view
//! are cut off by painting the background over the areas above and below it.

use std::time::{Duration, Instant};

use crate::app::action::Action;
use crate::app::anim::{Ease, Tween};
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};
use crate::ui::icons::{self, Icon};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, space, typography};

pub const VISIBLE_ROWS: usize = 5;
pub const ROW_HEIGHT: f32 = 72.0;
const ICON_SIZE: f32 = 40.0;
const SCROLLBAR_WIDTH: f32 = 6.0;

pub struct Row<'a> {
    pub icon: Option<Icon>,
    pub label: &'a str,
    /// Right-aligned secondary text ("2.4 MB", "On", "›"); empty for none.
    pub detail: String,
}

pub struct ListView {
    len: usize,
    focus: usize,
    /// Index of the first fully visible row.
    first: usize,
    focus_anim: Tween,
    scroll_anim: Tween,
    // Sampled by `animate`, read by `draw`.
    focus_pos: f32,
    scroll_pos: f32,
}

impl ListView {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            focus: 0,
            first: 0,
            focus_anim: Tween::at(0.0),
            scroll_anim: Tween::at(0.0),
            focus_pos: 0.0,
            scroll_pos: 0.0,
        }
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    /// Changes the row count (e.g. after a reload), keeping focus as close
    /// to where it was as possible. No animation.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.jump_to(self.focus.min(len.saturating_sub(1)));
    }

    /// Focuses `index` instantly, scrolling it into view.
    pub fn jump_to(&mut self, index: usize) {
        self.focus = index.min(self.len.saturating_sub(1));
        self.first = Self::first_for(self.focus, self.first, self.len);
        self.focus_anim = Tween::at(self.focus as f32);
        self.scroll_anim = Tween::at(self.first as f32);
        self.focus_pos = self.focus as f32;
        self.scroll_pos = self.first as f32;
    }

    /// Up/Down move the focus. Returns true if the action was used (also at
    /// the ends, so the screen doesn't treat it as something else).
    pub fn handle(&mut self, action: Action, now: Instant, duration: Duration) -> bool {
        let target = match action {
            Action::Up => self.focus.saturating_sub(1),
            Action::Down => (self.focus + 1).min(self.len.saturating_sub(1)),
            _ => return false,
        };
        if target != self.focus {
            self.focus = target;
            self.first = Self::first_for(target, self.first, self.len);
            self.focus_anim
                .retarget(target as f32, now, duration, Ease::OutCubic);
            self.scroll_anim
                .retarget(self.first as f32, now, duration, Ease::OutCubic);
        }
        true
    }

    pub fn animate(&mut self, now: Instant) -> bool {
        let (f, s) = (self.focus_anim.value(now), self.scroll_anim.value(now));
        let changed = f != self.focus_pos || s != self.scroll_pos;
        self.focus_pos = f;
        self.scroll_pos = s;
        changed
    }

    /// Top and bottom of the list area for this theme.
    pub fn area(theme: &Theme) -> (f32, f32) {
        let top = theme.safe.top + space::STATUS_BAR_HEIGHT + space::XS;
        (top, top + VISIBLE_ROWS as f32 * ROW_HEIGHT)
    }

    pub fn draw(&self, c: &mut Canvas, theme: &Theme, rows: &[Row]) {
        let (top, bottom) = Self::area(theme);
        let left = theme.safe.left;
        let scrollable = self.len > VISIBLE_ROWS;
        let right = WIDTH as f32
            - theme.safe.right
            - if scrollable {
                SCROLLBAR_WIDTH + space::S
            } else {
                0.0
            };

        // Focus ring behind the focused row's content.
        if !rows.is_empty() {
            let y = top + (self.focus_pos - self.scroll_pos) * ROW_HEIGHT;
            let inset = space::FOCUS_WIDTH;
            c.fill_rounded_rect(
                left,
                y + inset,
                right - left,
                ROW_HEIGHT - 2.0 * inset,
                space::RADIUS,
                rgb(color::SURFACE),
            );
            c.stroke_rounded_rect(
                left + inset / 2.0,
                y + inset * 1.5,
                right - left - inset,
                ROW_HEIGHT - 3.0 * inset,
                space::RADIUS,
                space::FOCUS_WIDTH,
                rgb(color::FOCUS),
            );
        }

        let first = self.scroll_pos.floor().max(0.0) as usize;
        for (i, row) in rows.iter().enumerate().skip(first).take(VISIBLE_ROWS + 1) {
            let y = top + (i as f32 - self.scroll_pos) * ROW_HEIGHT;
            Self::draw_row(c, row, left, right, y);
        }

        // Clip rows that slid outside the list area.
        c.fill_rect(0.0, 0.0, WIDTH as f32, top, rgb(color::BACKGROUND));
        c.fill_rect(
            0.0,
            bottom,
            WIDTH as f32,
            HEIGHT as f32 - bottom,
            rgb(color::BACKGROUND),
        );

        if scrollable {
            let track_h = bottom - top;
            let thumb_h = (track_h * VISIBLE_ROWS as f32 / self.len as f32).max(24.0);
            let max_first = (self.len - VISIBLE_ROWS) as f32;
            let thumb_y = top + (track_h - thumb_h) * (self.scroll_pos / max_first).clamp(0.0, 1.0);
            let x = WIDTH as f32 - theme.safe.right - SCROLLBAR_WIDTH;
            c.fill_rounded_rect(x, top, SCROLLBAR_WIDTH, track_h, 3.0, rgb(color::SURFACE));
            c.fill_rounded_rect(
                x,
                thumb_y,
                SCROLLBAR_WIDTH,
                thumb_h,
                3.0,
                rgb(color::TEXT_SECONDARY),
            );
        }
    }

    fn draw_row(c: &mut Canvas, row: &Row, left: f32, right: f32, y: f32) {
        let pad = space::M;
        let mut x = left + pad;
        let mid = y + ROW_HEIGHT / 2.0;
        if let Some(icon) = row.icon {
            icons::draw(
                c,
                icon,
                x + ICON_SIZE / 2.0,
                mid,
                ICON_SIZE,
                rgb(color::TEXT),
            );
            x += ICON_SIZE + space::S;
        }

        let detail_style = typography::CAPTION;
        let mut detail_w = 0.0;
        if !row.detail.is_empty() {
            let dy = mid - fonts().line_metrics(detail_style).height / 2.0;
            detail_w = c.text(
                detail_style,
                &row.detail,
                right - pad,
                dy,
                Align::Right,
                rgb(color::TEXT_SECONDARY),
            ) + space::M;
        }

        let style = typography::BODY;
        let label = fonts().ellipsize(style, row.label, right - pad - detail_w - x);
        let ly = mid - fonts().line_metrics(style).height / 2.0;
        c.text(style, &label, x, ly, Align::Left, rgb(color::TEXT));
    }

    /// Smallest scroll change that keeps `focus` visible.
    fn first_for(focus: usize, first: usize, len: usize) -> usize {
        let max_first = len.saturating_sub(VISIBLE_ROWS);
        let first = if focus < first {
            focus
        } else if focus >= first + VISIBLE_ROWS {
            focus + 1 - VISIBLE_ROWS
        } else {
            first
        };
        first.min(max_first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Duration = Duration::from_millis(100);

    #[test]
    fn focus_moves_and_scrolls_only_when_leaving_the_window() {
        let mut l = ListView::new(12);
        let t = Instant::now();
        for _ in 0..4 {
            l.handle(Action::Down, t, D);
        }
        assert_eq!((l.focus, l.first), (4, 0), "still inside the first window");
        l.handle(Action::Down, t, D);
        assert_eq!((l.focus, l.first), (5, 1), "scrolled by one");
        for _ in 0..20 {
            l.handle(Action::Down, t, D);
        }
        assert_eq!((l.focus, l.first), (11, 7), "clamped at the end");
        l.handle(Action::Up, t, D);
        assert_eq!(l.first, 7, "moving up inside the window doesn't scroll");
    }

    #[test]
    fn ends_consume_the_action_and_other_actions_pass_through() {
        let mut l = ListView::new(3);
        let t = Instant::now();
        assert!(l.handle(Action::Up, t, D));
        assert_eq!(l.focus, 0);
        assert!(!l.handle(Action::Select, t, D));
    }

    #[test]
    fn set_len_keeps_focus_in_range() {
        let mut l = ListView::new(10);
        l.jump_to(9);
        l.set_len(4);
        assert_eq!(l.focus, 3);
        l.set_len(0);
        assert_eq!(l.focus, 0);
    }
}
