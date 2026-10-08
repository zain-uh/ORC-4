//! Top strip inside the safe area: screen title on the left; connection and
//! clock on the right. A missing source shows nothing, never a placeholder
//! value. Bluetooth, phone and battery indicators join as their services
//! arrive.

use crate::ui::canvas::{Canvas, WIDTH, rgb};
use crate::ui::icons;
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{Theme, color, space, typography};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Connection {
    /// Not checked yet, or checking is turned off: show nothing.
    #[default]
    Unknown,
    Online,
    /// A hotel/café sign-in page is in the way.
    SignIn,
    Offline,
}

/// Live device state shown in the status bar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// Local (hours, minutes), or `None` while the clock is unknown.
    pub clock: Option<(u8, u8)>,
    pub connection: Connection,
    /// Wi-Fi signal, 0–4 bars, while connected to Wi-Fi.
    pub wifi_bars: Option<u8>,
    /// Connected Wi-Fi network (shown in Settings, not in the bar).
    pub wifi_name: Option<String>,
}

const ICON_SIZE: f32 = 30.0;

pub fn draw(c: &mut Canvas, theme: &Theme, status: &Status, title: Option<&str>) {
    let top = theme.safe.top;
    let (left, mut right) = (theme.safe.left, WIDTH as f32 - theme.safe.right);
    let centre_y =
        |style| top + (space::STATUS_BAR_HEIGHT - fonts().line_metrics(style).height) / 2.0;

    if let Some((h, m)) = status.clock {
        let style = typography::NUMERIC;
        let w = c.text(
            style,
            &format!("{h:02}:{m:02}"),
            right,
            centre_y(style),
            Align::Right,
            rgb(color::TEXT),
        );
        right -= w + space::M;
    }

    // Words for problems (clearer than an icon on composite video), an icon
    // with signal bars when all is well.
    let words = match status.connection {
        Connection::Offline => Some("Offline"),
        Connection::SignIn => Some("Sign-in needed"),
        Connection::Online | Connection::Unknown => None,
    };
    if let Some(words) = words {
        let style = typography::CAPTION;
        let w = c.text(
            style,
            words,
            right,
            centre_y(style),
            Align::Right,
            rgb(color::TEXT),
        );
        right -= w + space::M;
    } else if let Some(bars) = status.wifi_bars {
        let cy = top + space::STATUS_BAR_HEIGHT / 2.0;
        icons::wifi(c, right - ICON_SIZE / 2.0, cy, ICON_SIZE, bars);
        right -= ICON_SIZE + space::M;
    }

    if let Some(title) = title {
        let style = typography::CAPTION;
        let max = right - left;
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
