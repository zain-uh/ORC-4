//! Settings → Display.

use std::time::Instant;

use crate::app::action::Action;
use crate::config::{DisplaySettings, TvNorm};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use super::calibrate::Calibrate;
use crate::screens::nav::{Cx, Nav, Screen};

const ROWS: usize = 5;
const NEXT_START: &str = "Applies next time ORC-4 starts";

pub struct DisplayPage {
    /// Copy of the current values for drawing (refreshed after each change).
    values: DisplaySettings,
    list: ListView,
}

impl DisplayPage {
    pub fn new(cx: &Cx) -> Self {
        Self {
            values: cx.settings.display.clone(),
            list: ListView::new(ROWS),
        }
    }
}

fn on_off(v: bool) -> String {
    if v { "On" } else { "Off" }.into()
}

impl Screen for DisplayPage {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        if action != Action::Select {
            return Nav::Unhandled;
        }
        let d = &mut cx.settings.display;
        match self.list.focus() {
            0 => return Nav::Push(Box::new(Calibrate::new(cx))),
            1 => d.mirror_x = !d.mirror_x,
            2 => d.mirror_y = !d.mirror_y,
            3 => {
                d.tv_norm = match d.tv_norm {
                    TvNorm::Pal => TvNorm::Ntsc,
                    TvNorm::Ntsc => TvNorm::Pal,
                }
            }
            _ => d.reduce_motion = !d.reduce_motion,
        }
        if (1..=3).contains(&self.list.focus()) {
            cx.toast(NEXT_START);
        }
        self.values = cx.settings.display.clone();
        Nav::Stay
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.values = cx.settings.display.clone(); // after calibration
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let v = &self.values;
        let rows = [
            Row {
                icon: None,
                label: "Screen edges",
                detail: format!("{:.0} × {:.0}  ›", v.edge_x, v.edge_y),
            },
            Row {
                icon: None,
                label: "Mirror left–right",
                detail: on_off(v.mirror_x),
            },
            Row {
                icon: None,
                label: "Mirror up–down",
                detail: on_off(v.mirror_y),
            },
            Row {
                icon: None,
                label: "Video standard",
                detail: match v.tv_norm {
                    TvNorm::Pal => "PAL",
                    TvNorm::Ntsc => "NTSC",
                }
                .into(),
            },
            Row {
                icon: None,
                label: "Reduce motion",
                detail: on_off(v.reduce_motion),
            },
        ];
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("Settings › Display"));
        hint::draw(c, cx.theme, "Enter change · Esc back");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::nav::testing::Harness;

    #[test]
    fn toggles_write_through_to_settings() {
        let mut h = Harness::new();
        let mut page = DisplayPage::new(&h.cx(Instant::now()));
        let mut cx = h.cx(Instant::now());
        for _ in 0..4 {
            page.handle(Action::Down, &mut cx);
        }
        page.handle(Action::Select, &mut cx);
        assert!(cx.settings.display.reduce_motion);
        assert!(page.values.reduce_motion, "shown value refreshed");
    }

    #[test]
    fn video_standard_cycles() {
        let mut h = Harness::new();
        let mut page = DisplayPage::new(&h.cx(Instant::now()));
        let mut cx = h.cx(Instant::now());
        for _ in 0..3 {
            page.handle(Action::Down, &mut cx);
        }
        page.handle(Action::Select, &mut cx);
        assert_eq!(cx.settings.display.tv_norm, TvNorm::Ntsc);
        page.handle(Action::Select, &mut cx);
        assert_eq!(cx.settings.display.tv_norm, TvNorm::Pal);
    }
}
