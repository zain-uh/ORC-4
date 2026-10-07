//! Centred message for empty, loading and error states.

use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, space, typography};

pub fn draw(c: &mut Canvas, theme: &Theme, title: &str, detail: Option<&str>) {
    let (left, top, width, height) = theme.content();
    let title_h = fonts().line_metrics(typography::TITLE).height;
    let detail_h = if detail.is_some() {
        space::S + 2.0 * (typography::BODY.size * typography::LINE_HEIGHT)
    } else {
        0.0
    };
    let mut y = top + (height - title_h - detail_h) / 2.0;
    c.text(
        typography::TITLE,
        title,
        left + width / 2.0,
        y,
        Align::Center,
        rgb(color::TEXT),
    );
    if let Some(detail) = detail {
        y += title_h + space::S;
        c.paragraph(
            typography::BODY,
            detail,
            left,
            y,
            width,
            2,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
    }
}
