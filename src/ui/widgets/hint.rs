//! One line of key hints at the bottom of the safe area ("Enter open · Esc back").

use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, typography};

pub fn draw(c: &mut Canvas, theme: &Theme, text: &str) {
    let style = typography::CAPTION;
    let y = HEIGHT as f32 - theme.safe.bottom - fonts().line_metrics(style).height;
    c.text(
        style,
        text,
        WIDTH as f32 / 2.0,
        y,
        Align::Center,
        rgb(color::TEXT_SECONDARY),
    );
}
