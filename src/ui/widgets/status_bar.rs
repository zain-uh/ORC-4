//! Top strip inside the safe area: screen title on the left, clock on the
//! right. Wi-Fi, Bluetooth, phone and battery indicators join it as their
//! services arrive (M5, M7, M8); a missing source shows nothing, never a
//! placeholder value.

use crate::ui::canvas::{Canvas, WIDTH, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, space, typography};

/// Live device state shown in the status bar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// Local (hours, minutes), or `None` while the clock is unknown.
    pub clock: Option<(u8, u8)>,
}

pub fn draw(c: &mut Canvas, theme: &Theme, status: &Status, title: Option<&str>) {
    let top = theme.safe.top;
    let (left, right) = (theme.safe.left, WIDTH as f32 - theme.safe.right);

    let centre_y =
        |style| top + (space::STATUS_BAR_HEIGHT - fonts().line_metrics(style).height) / 2.0;

    let mut clock_width = 0.0;
    if let Some((h, m)) = status.clock {
        let style = typography::NUMERIC;
        clock_width = c.text(
            style,
            &format!("{h:02}:{m:02}"),
            right,
            centre_y(style),
            Align::Right,
            rgb(color::TEXT),
        );
    }

    if let Some(title) = title {
        let style = typography::CAPTION;
        let max = right - left - clock_width - space::M;
        let title = fonts().ellipsize(style, title, max);
        c.text(
            style,
            &title,
            left,
            centre_y(style),
            Align::Left,
            rgb(color::TEXT_SECONDARY),
        );
    }
}
