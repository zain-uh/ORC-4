//! Settings app (offline). Sections are pushed as their own screens; changes
//! go straight into `cx.settings` and the runtime saves them shortly after.
//!
//! Bluetooth, Network, Phone and Sound sections arrive with their services
//! (M5, M7, M8).

mod calibrate;
mod device;
mod display;
mod system;

use std::time::Instant;

use crate::app::action::Action;
use crate::logging;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::fonts;
use crate::ui::theme::{color, motion, typography};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use super::nav::{Cx, Nav, Screen};
use super::text_viewer::TextViewer;

const SECTIONS: [&str; 4] = ["Display", "Device", "Logs", "System"];

pub struct SettingsHome {
    list: ListView,
}

impl SettingsHome {
    pub fn new() -> Self {
        Self {
            list: ListView::new(SECTIONS.len()),
        }
    }
}

impl Screen for SettingsHome {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        if action != Action::Select {
            return Nav::Unhandled;
        }
        Nav::Push(match self.list.focus() {
            0 => Box::new(display::DisplayPage::new(cx)),
            1 => Box::new(device::DevicePage::new()),
            2 => {
                let style = typography::NUMERIC;
                let (_, _, width, _) = cx.theme().content();
                let lines = logging::recent()
                    .iter()
                    .flat_map(|l| fonts().wrap(style, l, width, usize::MAX))
                    .collect();
                Box::new(TextViewer::with_lines("Logs", style, lines, true))
            }
            _ => Box::new(system::SystemPage::new()),
        })
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let rows: Vec<Row> = SECTIONS
            .iter()
            .map(|s| Row {
                icon: None,
                label: s,
                detail: "›".into(),
            })
            .collect();
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("Settings"));
        hint::draw(c, cx.theme, "Enter open · Esc back");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::nav::testing::Harness;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    pub fn render(screen: &dyn Screen) -> Canvas {
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
        c
    }

    #[test]
    fn every_section_opens() {
        let mut h = Harness::new();
        let mut home = SettingsHome::new();
        let mut cx = h.cx(Instant::now());
        for _ in SECTIONS {
            assert!(matches!(home.handle(Action::Select, &mut cx), Nav::Push(_)));
            home.handle(Action::Down, &mut cx);
        }
    }

    #[test]
    fn snapshot_settings_home() {
        assert_snapshot("settings-home", &render(&SettingsHome::new()));
    }

    #[test]
    fn snapshot_display_page() {
        let mut h = Harness::new();
        let page = display::DisplayPage::new(&h.cx(Instant::now()));
        assert_snapshot("settings-display", &render(&page));
    }

    #[test]
    fn snapshot_calibration() {
        let mut h = Harness::new();
        let page = calibrate::Calibrate::new(&h.cx(Instant::now()));
        assert_snapshot("settings-calibrate", &render(&page));
    }
}
