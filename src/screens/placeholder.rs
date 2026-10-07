//! Stand-in for an app that isn't built yet: proves opening and closing apps
//! works. Each one is replaced by the real screen in its milestone.

use std::time::Instant;

use crate::app::action::Action;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgb};
use crate::ui::icons::{self, Icon};
use crate::ui::nav::{Nav, Screen};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};
use crate::ui::widgets::status_bar;

use super::registry::AppEntry;

pub struct Placeholder {
    name: &'static str,
    icon: Icon,
    accent: u32,
    note: &'static str,
}

impl Placeholder {
    pub fn new(app: &AppEntry, note: &'static str) -> Self {
        Self {
            name: app.name,
            icon: app.icon,
            accent: app.accent,
            note,
        }
    }
}

impl Screen for Placeholder {
    fn handle(&mut self, _action: Action, _now: Instant) -> Nav {
        Nav::Unhandled // Back/Home use the navigator defaults
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        status_bar::draw(c, t, cx.status, Some(self.name));

        let mid = WIDTH as f32 / 2.0;
        let badge_y = 210.0;
        c.fill_circle(mid, badge_y, 72.0, rgb(self.accent));
        icons::draw(c, self.icon, mid, badge_y, 80.0, rgb(color::TEXT));

        let mut y = badge_y + 72.0 + space::M;
        c.text(
            typography::TITLE,
            self.name,
            mid,
            y,
            Align::Center,
            rgb(color::TEXT),
        );
        y += fonts().line_metrics(typography::TITLE).height + space::XS;
        let width = WIDTH as f32 - t.safe.left - t.safe.right;
        c.paragraph(
            typography::BODY,
            self.note,
            t.safe.left,
            y,
            width,
            2,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );

        let hint = typography::CAPTION;
        c.text(
            hint,
            "Esc back",
            mid,
            HEIGHT as f32 - t.safe.bottom - fonts().line_metrics(hint).height,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::registry::apps;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn snapshot_placeholder() {
        let files = apps().into_iter().find(|a| a.name == "Files").unwrap();
        let screen = (files.open)(&files);
        let mut c = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
        };
        screen.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("placeholder-files", &c);
    }
}
