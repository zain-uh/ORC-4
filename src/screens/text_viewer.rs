//! Paged text reader (text files, logs, Wikipedia articles). One page per
//! screen; ↑/↓ or ←/→
//! turn pages. No scrolling animation: instant page turns are easier to
//! follow on a small display and cost nothing. Heading lines get a bar.

use std::path::PathBuf;

use crate::app::action::Action;
use crate::services::storage::{self, MAX_TEXT_BYTES};
use crate::services::{DocLine, Reply, ServiceEvent, Ticket, UserError};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, TextStyle, fonts};
use crate::ui::theme::{Theme, color, space, typography};
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
    lines: Vec<DocLine>,
    /// Shown when loading fails ("Can't open file").
    failure: &'static str,
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
            Ok(Reply::Doc(lines.into_iter().map(DocLine::plain).collect()))
        });
        Self::loading(name, style, ticket, "Can't open file")
    }

    /// Shows the `Reply::Doc` lines that the job behind `ticket` produces.
    pub fn loading(title: String, style: TextStyle, ticket: Ticket, failure: &'static str) -> Self {
        Self {
            title,
            style,
            lines: Vec::new(),
            failure,
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
            lines: lines.into_iter().map(DocLine::plain).collect(),
            failure: "",
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
            Ok(Reply::Doc(lines)) => {
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
                    let y = top + i as f32 * lh;
                    let mut x = left;
                    if line.heading {
                        // A bar, not colour: colour smears on composite video.
                        c.fill_rect(left, y + lh * 0.15, 4.0, lh * 0.7, rgb(color::TEXT));
                        x += space::M;
                    }
                    c.text(self.style, &line.text, x, y, Align::Left, rgb(color::TEXT));
                }
                format!("{} · {}/{}", self.title, page + 1, self.pages(t))
            }
            Load::Loading(_) => {
                message::draw(c, t, "Loading…", None);
                self.title.clone()
            }
            Load::Failed(e) => {
                message::draw(c, t, self.failure, Some(e.message()));
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

#[cfg(test)]
mod snapshot {
    use super::*;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn snapshot_reader_with_headings() {
        let line = |text: &str, heading| DocLine {
            text: text.into(),
            heading,
        };
        let mut v = TextViewer::with_lines("Owl", typography::BODY, Vec::new(), false);
        v.lines = vec![
            line("Owls are birds from the order Strigiformes,", false),
            line("which includes over 200 species of mostly", false),
            line("solitary and nocturnal birds of prey.", false),
            line("", false),
            line("Anatomy", true),
            line("", false),
            line("Owls have large forward-facing eyes and", false),
            line("ear-holes, a hawk-like beak and a flat face.", false),
        ];
        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
                ..Status::default()
            },
        );
        v.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("reader", &c);
    }
}
