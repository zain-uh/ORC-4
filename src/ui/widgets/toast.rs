//! Short message pill at the bottom of the screen ("Folder isn't empty").
//! Timing and fading are owned by the navigator.

use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgba};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, space, typography};

pub fn draw(c: &mut Canvas, theme: &Theme, message: &str, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    let style = typography::CAPTION;
    let max_text = WIDTH as f32 - theme.safe.left - theme.safe.right - 4.0 * space::M;
    let text = fonts().ellipsize(style, message, max_text);
    let line = fonts().line_metrics(style).height;

    let (w, h) = (
        fonts().measure(style, &text) + 2.0 * space::M,
        line + 2.0 * space::S,
    );
    let x = (WIDTH as f32 - w) / 2.0;
    let y = HEIGHT as f32 - theme.safe.bottom - h;
    c.fill_rounded_rect(x, y, w, h, h / 2.0, rgba(color::SURFACE_RAISED, opacity));
    // Light outline so the pill separates from content on composite video.
    c.stroke_rounded_rect(
        x,
        y,
        w,
        h,
        h / 2.0,
        2.0,
        rgba(color::TEXT_DISABLED, opacity),
    );
    c.text(
        style,
        &text,
        WIDTH as f32 / 2.0,
        y + space::S,
        Align::Center,
        rgba(color::TEXT, opacity),
    );
}
