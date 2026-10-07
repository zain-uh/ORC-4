//! Settings → Device: read-only facts, refreshed each time the page opens.

use std::time::{Duration, Instant};

use crate::app::action::Action;
use crate::services::system::{self, DeviceInfo};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use crate::screens::nav::{Cx, Nav, Screen};

const ROWS: usize = 5;
const UNKNOWN: &str = "—";

pub struct DevicePage {
    info: DeviceInfo,
    list: ListView,
}

impl DevicePage {
    pub fn new() -> Self {
        Self {
            info: DeviceInfo::default(),
            list: ListView::new(ROWS),
        }
    }
}

fn uptime(d: Duration) -> String {
    let mins = d.as_secs() / 60;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m} min"),
        (h, m) if h < 48 => format!("{h} h {m} min"),
        (h, _) => format!("{} days", h / 24),
    }
}

impl Screen for DevicePage {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        Nav::Unhandled
    }

    fn on_show(&mut self, _cx: &mut Cx) {
        self.info = system::device_info();
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let i = &self.info;
        let model = match &i.model {
            Some(m) => m.clone(),
            None if cfg!(feature = "simulator") => "Simulator".into(),
            None => UNKNOWN.into(),
        };
        let rows = [
            Row {
                icon: None,
                label: "Model",
                detail: model,
            },
            Row {
                icon: None,
                label: "Software",
                detail: format!("ORC-4 {}", env!("CARGO_PKG_VERSION")),
            },
            Row {
                icon: None,
                label: "Running for",
                detail: i.uptime.map_or(UNKNOWN.into(), uptime),
            },
            Row {
                icon: None,
                label: "Memory free",
                detail: i.memory_kib.map_or(UNKNOWN.into(), |(avail, total)| {
                    format!("{} of {} MB", avail / 1024, total / 1024)
                }),
            },
            Row {
                icon: None,
                label: "Temperature",
                detail: i
                    .cpu_temp_c
                    .map_or(UNKNOWN.into(), |t| format!("{t:.0} °C")),
            },
        ];
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("Settings › Device"));
        hint::draw(c, cx.theme, "Esc back");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_reads_naturally() {
        assert_eq!(uptime(Duration::from_secs(5 * 60)), "5 min");
        assert_eq!(
            uptime(Duration::from_secs(3 * 3600 + 12 * 60)),
            "3 h 12 min"
        );
        assert_eq!(uptime(Duration::from_secs(5 * 86400)), "5 days");
    }
}
