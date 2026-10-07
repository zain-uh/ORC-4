//! Paged text reader (text files, logs). One page per screen; ↑/↓ or ←/→
//! turn pages. No scrolling animation: instant page turns are easier to
//! follow on a small display and cost nothing.

use std::path::PathBuf;

use crate::app::action::Action;
use crate::services::storage::{self, MAX_TEXT_BYTES};
use crate::services::{Reply, ServiceEvent, Ticket, UserError};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, TextStyle, fonts};
use crate::ui::theme::{Theme, color, typography};
use crate::ui::widgets::{message, status_bar};

use super::nav::{Cx, Nav, Screen};

enum Load {
    Loading(Ticket),
    Ready,
    Failed(UserError),
}

pub struct TextViewer {
    title: String,
    style: TextStyle,
    lines: Vec<String>,
    /// Requested page; clamped to the last page when used (so `usize::MAX`
    /// means "open at the end").
    page: usize,
    load: Load,
}

impl TextViewer {
    /// Reads and wraps the file on the worker.
    pub fn open_file(path: PathBuf, name: String, cx: &mut Cx) -> Self {
        let style = typography::BODY;
        let (_, _, width, _) = cx.theme().content();
        let ticket = cx.services.run(move || {
            let (text, truncated) = storage::read_text(&path)?;
            let mut lines = fonts().wrap_document(style, &text, width);
            if lines.iter().all(|l| l.is_empty()) {
                lines = vec!["(Empty file)".into()];
            }
            if truncated {
                lines.push(String::new());
                lines.push(format!(
                    "(Only the first {} KB is shown.)",
                    MAX_TEXT_BYTES / 1024
                ));
            }
            Ok(Reply::Lines(lines))
        });
        Self {
            title: name,
            style,
            lines: Vec::new(),
            page: 0,
            load: Load::Loading(ticket),
        }
    }

    /// Shows already-wrapped lines, optionally starting on the last page.
    pub fn with_lines(
        title: &str,
        style: TextStyle,
        lines: Vec<String>,
        start_at_end: bool,
    ) -> Self {
        Self {
            title: title.into(),
            style,
            lines,
            page: if start_at_end { usize::MAX } else { 0 },
            load: Load::Ready,
        }
    }

    fn line_height(&self) -> f32 {
        (self.style.size * typography::LINE_HEIGHT).round()
    }

    fn per_page(&self, theme: &Theme) -> usize {
        let (_, _, _, height) = theme.content();
        ((height / self.line_height()) as usize).max(1)
    }

    fn pages(&self, theme: &Theme) -> usize {
        self.lines.len().div_ceil(self.per_page(theme)).max(1)
    }

    fn current_page(&self, theme: &Theme) -> usize {
        self.page.min(self.pages(theme) - 1)
    }
}

impl Screen for TextViewer {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        let theme = cx.theme();
        let page = self.current_page(&theme);
        self.page = match action {
            Action::Down | Action::Right => (page + 1).min(self.pages(&theme) - 1),
            Action::Up | Action::Left => page.saturating_sub(1),
            _ => return Nav::Unhandled,
        };
        Nav::Stay
    }

    fn on_service(&mut self, event: ServiceEvent, _cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if !matches!(self.load, Load::Loading(t) if t == event.ticket) {
            return Err(event);
        }
        match event.result {
            Ok(Reply::Lines(lines)) => {
                self.lines = lines;
                self.load = Load::Ready;
            }
            Ok(_) => self.load = Load::Failed(UserError::Internal),
            Err(e) => self.load = Load::Failed(e),
        }
        Ok(Nav::Stay)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));

        let title = match self.load {
            Load::Ready => {
                let (left, top, _, _) = t.content();
                let per_page = self.per_page(t);
                let page = self.current_page(t);
                let lh = self.line_height();
                let lines = self.lines.iter().skip(page * per_page).take(per_page);
                for (i, line) in lines.enumerate() {
                    c.text(
                        self.style,
                        line,
                        left,
                        top + i as f32 * lh,
                        Align::Left,
                        rgb(color::TEXT),
                    );
                }
                format!("{} · {}/{}", self.title, page + 1, self.pages(t))
            }
            Load::Loading(_) => {
                message::draw(c, t, "Loading…", None);
                self.title.clone()
            }
            Load::Failed(e) => {
                message::draw(c, t, "Can't open file", Some(e.message()));
                self.title.clone()
            }
        };
        status_bar::draw(c, t, cx.status, Some(&title));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;

    fn viewer(n: usize, at_end: bool) -> TextViewer {
        let lines = (0..n).map(|i| format!("line {i}")).collect();
        TextViewer::with_lines("t", typography::BODY, lines, at_end)
    }

    #[test]
    fn pages_turn_and_stop_at_the_ends() {
        let mut h = Harness::new();
        let theme = Theme::default();
        let mut v = viewer(25, false);
        let pages = v.pages(&theme);
        assert!(pages >= 3);
        let mut cx = h.cx(Instant::now());
        v.handle(Action::Up, &mut cx);
        assert_eq!(v.current_page(&theme), 0);
        for _ in 0..10 {
            v.handle(Action::Down, &mut cx);
        }
        assert_eq!(v.current_page(&theme), pages - 1);
    }

    #[test]
    fn can_open_at_the_end() {
        let theme = Theme::default();
        let v = viewer(25, true);
        assert_eq!(v.current_page(&theme), v.pages(&theme) - 1);
    }
}
