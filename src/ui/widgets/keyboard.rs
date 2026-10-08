//! On-screen keyboard for typing with only arrows and Enter (no phone, no
//! physical keyboard), e.g. a Wi-Fi password. Two pages: letters and
//! symbols. Up/Down move to the nearest key in the next row.

use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};

pub const ROW_HEIGHT: f32 = 46.0;
pub const ROWS: usize = 5;
const GAP: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Char(char),
    Shift,
    Symbols,
    Letters,
    Space,
    Backspace,
    Done,
}

/// What pressing a key means for the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Type(char),
    Backspace,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Letters,
    Symbols,
}

/// One row: keys with relative widths (each row adds up to 10).
type Row = Vec<(Key, f32)>;

fn chars(s: &str) -> Row {
    s.chars().map(|c| (Key::Char(c), 1.0)).collect()
}

fn rows(page: Page) -> [Row; ROWS] {
    let bottom = |switch| {
        vec![
            (switch, 2.0),
            (Key::Char(','), 1.0),
            (Key::Space, 4.0),
            (Key::Char('.'), 1.0),
            (Key::Done, 2.0),
        ]
    };
    match page {
        Page::Letters => [
            chars("1234567890"),
            chars("qwertyuiop"),
            chars("asdfghjkl'"),
            [(Key::Shift, 1.5)]
                .into_iter()
                .chain(chars("zxcvbnm"))
                .chain([(Key::Backspace, 1.5)])
                .collect(),
            bottom(Key::Symbols),
        ],
        Page::Symbols => [
            chars("!@#$%^&*()"),
            chars("-_=+[]{};:"),
            chars("\"'/\\|<>?~`"),
            chars("€£¥°§×÷¬")
                .into_iter()
                .chain([(Key::Backspace, 2.0)])
                .collect(),
            bottom(Key::Letters),
        ],
    }
}

/// Centre of each key in a row, as a fraction of the row width.
fn centres(row: &Row) -> Vec<f32> {
    let total: f32 = row.iter().map(|(_, w)| w).sum();
    let mut x = 0.0;
    row.iter()
        .map(|(_, w)| {
            let c = (x + w / 2.0) / total;
            x += w;
            c
        })
        .collect()
}

pub struct Keyboard {
    page: Page,
    shift: bool,
    row: usize,
    col: usize,
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            page: Page::Letters,
            shift: false,
            row: 0,
            col: 0,
        }
    }

    /// Moves focus. Returns false when moving up out of the top row (the
    /// caller then focuses the text field).
    pub fn move_by(&mut self, dx: isize, dy: isize) -> bool {
        let rows = rows(self.page);
        if dy != 0 {
            let target = self.row as isize + dy;
            if target < 0 {
                return false;
            }
            let target = (target as usize).min(ROWS - 1);
            let here = centres(&rows[self.row])[self.col];
            self.col = nearest(&centres(&rows[target]), here);
            self.row = target;
        }
        let len = rows[self.row].len() as isize;
        self.col = (self.col as isize + dx).clamp(0, len - 1) as usize;
        true
    }

    pub fn press(&mut self) -> Option<Press> {
        let key = rows(self.page)[self.row][self.col].0;
        match key {
            Key::Char(c) => {
                let c = if self.shift {
                    self.shift = false;
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c
                };
                Some(Press::Type(c))
            }
            Key::Space => Some(Press::Type(' ')),
            Key::Backspace => Some(Press::Backspace),
            Key::Done => Some(Press::Done),
            Key::Shift => {
                self.shift = !self.shift;
                None
            }
            Key::Symbols | Key::Letters => {
                self.page = if key == Key::Symbols {
                    Page::Symbols
                } else {
                    Page::Letters
                };
                self.col = self.col.min(rows(self.page)[self.row].len() - 1);
                None
            }
        }
    }

    /// Draws the keys in the given box; `focused` shows the focus ring.
    pub fn draw(&self, c: &mut Canvas, x: f32, y: f32, width: f32, focused: bool) {
        let style = typography::CAPTION;
        let text_h = fonts().line_metrics(style).height;
        for (r, row) in rows(self.page).iter().enumerate() {
            let total: f32 = row.iter().map(|(_, w)| w).sum();
            let mut kx = x;
            let ky = y + r as f32 * ROW_HEIGHT;
            for (i, (key, weight)) in row.iter().enumerate() {
                let kw = width * weight / total;
                let (bx, bw, bh) = (kx + GAP / 2.0, kw - GAP, ROW_HEIGHT - GAP);
                let active = matches!(key, Key::Shift) && self.shift;
                let fill = if active {
                    color::TEXT_SECONDARY
                } else {
                    color::SURFACE_RAISED
                };
                c.fill_rounded_rect(bx, ky, bw, bh, 8.0, rgb(fill));
                if focused && r == self.row && i == self.col {
                    c.stroke_rounded_rect(
                        bx - 1.0,
                        ky - 1.0,
                        bw + 2.0,
                        bh + 2.0,
                        9.0,
                        space::FOCUS_WIDTH,
                        rgb(color::FOCUS),
                    );
                }
                let label = self.label(*key);
                c.text(
                    style,
                    &label,
                    bx + bw / 2.0,
                    ky + (bh - text_h) / 2.0,
                    Align::Center,
                    rgb(if active {
                        color::BACKGROUND
                    } else {
                        color::TEXT
                    }),
                );
                kx += kw;
            }
        }
    }

    fn label(&self, key: Key) -> String {
        match key {
            Key::Char(ch) if self.shift => ch.to_uppercase().collect(),
            Key::Char(ch) => ch.to_string(),
            Key::Shift => "Shift".into(),
            Key::Symbols => "#+=".into(),
            Key::Letters => "abc".into(),
            Key::Space => "space".into(),
            Key::Backspace => "Del".into(),
            Key::Done => "Done".into(),
        }
    }
}

fn nearest(centres: &[f32], x: f32) -> usize {
    centres
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map_or(0, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_ten_units_wide() {
        for page in [Page::Letters, Page::Symbols] {
            for row in rows(page) {
                let total: f32 = row.iter().map(|(_, w)| w).sum();
                assert_eq!(total, 10.0, "{page:?}");
            }
        }
    }

    #[test]
    fn types_with_shift_once_then_lowercase() {
        let mut k = Keyboard::new();
        k.move_by(0, 3); // shift row
        k.move_by(-10, 0); // Shift key
        assert_eq!(k.press(), None);
        k.move_by(1, 0); // z
        assert_eq!(k.press(), Some(Press::Type('Z')));
        assert_eq!(k.press(), Some(Press::Type('z')), "shift is one-shot");
    }

    #[test]
    fn up_from_the_top_row_leaves_the_keyboard() {
        let mut k = Keyboard::new();
        assert!(!k.move_by(0, -1));
        assert!(k.move_by(0, 1));
    }

    #[test]
    fn vertical_moves_keep_the_horizontal_position() {
        let mut k = Keyboard::new();
        k.move_by(9, 0); // '0', far right
        k.move_by(0, 4); // bottom row
        assert_eq!(k.press(), Some(Press::Done), "Done is under the right side");
    }

    #[test]
    fn symbols_page_round_trip() {
        let mut k = Keyboard::new();
        k.move_by(0, 4); // bottom row, first key: #+=
        assert_eq!(k.press(), None);
        k.move_by(0, -4); // top row of symbols
        assert_eq!(k.press(), Some(Press::Type('!')));
    }
}
