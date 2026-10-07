//! Settings → System: restart, reboot, shut down, reset. Each asks first.

use std::time::Instant;

use crate::app::action::Action;
use crate::config;
use crate::services::system::{self, PowerAction};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use crate::screens::confirm::Confirm;
use crate::screens::nav::{Cx, Nav, Screen};

const LABELS: [&str; 4] = ["Restart ORC-4", "Reboot", "Shut down", "Reset all settings"];

pub struct SystemPage {
    list: ListView,
}

impl SystemPage {
    pub fn new() -> Self {
        Self {
            list: ListView::new(LABELS.len()),
        }
    }
}

fn power(action: PowerAction, cx: &mut Cx) {
    match system::power(action) {
        Ok(()) if cfg!(feature = "simulator") => cx.toast("Simulator: skipped"),
        Ok(()) => {}
        Err(e) => {
            log::error!("{e}");
            cx.toast("Couldn't do that");
        }
    }
}

impl Screen for SystemPage {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        if action != Action::Select {
            return Nav::Unhandled;
        }
        let confirm = match self.list.focus() {
            0 => Confirm::new("Restart ORC-4?", "Restart", |cx| cx.request_exit()),
            1 => Confirm::new("Reboot the glasses?", "Reboot", |cx| {
                power(PowerAction::Reboot, cx)
            }),
            2 => Confirm::new("Shut down the glasses?", "Shut down", |cx| {
                power(PowerAction::PowerOff, cx)
            }),
            _ => Confirm::new("Reset all settings?", "Reset", |cx| {
                *cx.settings = config::system_defaults(cx.paths);
                cx.toast("Settings reset");
            }),
        };
        Nav::Push(Box::new(confirm))
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let rows: Vec<Row> = LABELS
            .iter()
            .map(|l| Row {
                icon: None,
                label: l,
                detail: String::new(),
            })
            .collect();
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("Settings › System"));
        hint::draw(c, cx.theme, "Enter choose · Esc back");
    }
}
