//! Search app: Wikipedia results for a query, opened in the reader.
//! Uses Wikimedia's free public API; a web search provider can be added
//! behind the same screens later.

use std::time::Instant;

use crate::app::action::Action;
use crate::services::wiki::{self, Block, Hit};
use crate::services::{DocLine, Reply, ServiceEvent, Ticket, UserError};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{TextStyle, fonts};
use crate::ui::theme::{color, motion, space, typography};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, message, status_bar};

use super::nav::{Cx, Nav, Screen};
use super::text_viewer::TextViewer;

const ATTRIBUTION: &str = "Text from Wikipedia, licensed CC BY-SA 4.0.";

enum Load {
    Loading(Ticket),
    Ready,
    Failed(UserError),
}

pub struct WikiResults {
    query: String,
    lang: String,
    hits: Vec<Hit>,
    list: ListView,
    load: Load,
}

impl WikiResults {
    pub fn new(query: String, cx: &mut Cx) -> Self {
        let lang = cx.settings.search.language.clone();
        let (l, q) = (lang.clone(), query.clone());
        let ticket = cx
            .services
            .run_net(move || wiki::search(&l, &q).map(Reply::Hits));
        Self {
            query,
            lang,
            hits: Vec::new(),
            list: ListView::new(0),
            load: Load::Loading(ticket),
        }
    }

    fn open(&self, hit: &Hit, cx: &mut Cx) -> Nav {
        let style = typography::BODY;
        let (_, _, width, _) = cx.theme().content();
        let (lang, title) = (self.lang.clone(), hit.title.clone());
        // Fetch and lay out on the network pool; the reader shows the result.
        let ticket = cx.services.run_net(move || {
            let blocks = wiki::article(&lang, &title)?;
            Ok(Reply::Doc(layout(&blocks, style, width)))
        });
        Nav::Push(Box::new(TextViewer::loading(
            hit.title.clone(),
            style,
            ticket,
            "Can't open article",
        )))
    }
}

/// Wraps article blocks into reader lines: a blank line before each heading
/// and between paragraphs, the credit at the end.
fn layout(blocks: &[Block], style: TextStyle, width: f32) -> Vec<DocLine> {
    let f = fonts();
    let mut lines: Vec<DocLine> = Vec::new();
    for block in blocks {
        if !lines.is_empty() {
            lines.push(DocLine::plain(String::new()));
        }
        let text = f.printable(style.font, &block.text);
        // Headings are indented past their marker bar.
        let w = if block.heading {
            width - space::M
        } else {
            width
        };
        lines.extend(
            f.wrap(style, &text, w, usize::MAX)
                .into_iter()
                .map(|t| DocLine {
                    text: t,
                    heading: block.heading,
                }),
        );
    }
    lines.push(DocLine::plain(String::new()));
    lines.extend(
        f.wrap(style, ATTRIBUTION, width, usize::MAX)
            .into_iter()
            .map(DocLine::plain),
    );
    lines
}

impl Screen for WikiResults {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        match (action, self.hits.get(self.list.focus())) {
            (Action::Select, Some(hit)) => self.open(&hit.clone(), cx),
            _ => Nav::Unhandled,
        }
    }

    fn on_service(&mut self, event: ServiceEvent, _cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if !matches!(self.load, Load::Loading(t) if t == event.ticket) {
            return Err(event);
        }
        match event.result {
            Ok(Reply::Hits(hits)) => {
                self.list.set_len(hits.len());
                self.hits = hits;
                self.load = Load::Ready;
            }
            Ok(_) => self.load = Load::Failed(UserError::Internal),
            Err(e) => self.load = Load::Failed(e),
        }
        Ok(Nav::Stay)
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        if self.hits.is_empty() {
            match &self.load {
                Load::Loading(_) => message::draw(c, t, "Searching…", None),
                Load::Failed(e) => message::draw(c, t, e.message(), None),
                Load::Ready => message::draw(c, t, "No results", Some("Try other words.")),
            }
            hint::draw(c, t, "Esc back");
        } else {
            let rows: Vec<Row> = self
                .hits
                .iter()
                .map(|h| Row {
                    icon: None,
                    label: &h.title,
                    detail: String::new(),
                })
                .collect();
            self.list.draw(c, t, &rows);
            // The focused result's description replaces the key hints.
            let desc = self
                .hits
                .get(self.list.focus())
                .and_then(|h| h.description.as_deref())
                .unwrap_or("Enter read · Esc back");
            hint::draw(c, t, &fonts().printable(typography::CAPTION.font, desc));
        }
        let title = format!("Wikipedia · “{}”", self.query);
        status_bar::draw(c, t, cx.status, Some(&title));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn layout_separates_blocks_marks_headings_and_credits() {
        let blocks = [
            Block {
                heading: false,
                text: "Owls are birds.".into(),
            },
            Block {
                heading: true,
                text: "Anatomy".into(),
            },
            Block {
                heading: false,
                text: "Big eyes.".into(),
            },
        ];
        let lines = layout(&blocks, typography::BODY, 848.0);
        let texts: Vec<(&str, bool)> = lines.iter().map(|l| (l.text.as_str(), l.heading)).collect();
        assert_eq!(
            &texts[..5],
            [
                ("Owls are birds.", false),
                ("", false),
                ("Anatomy", true),
                ("", false),
                ("Big eyes.", false),
            ]
        );
        assert!(lines.last().unwrap().text.contains("Wikipedia"));
    }

    #[test]
    fn snapshot_results() {
        // Built directly: `new` would start a real search.
        let mut r = WikiResults {
            query: "owls".into(),
            lang: "en".into(),
            hits: Vec::new(),
            list: ListView::new(0),
            load: Load::Ready,
        };
        r.hits = vec![
            Hit {
                title: "Owl".into(),
                description: Some("Order of birds".into()),
            },
            Hit {
                title: "Barn owl".into(),
                description: Some("Species of bird".into()),
            },
            Hit {
                title: "Snowy owl".into(),
                description: None,
            },
        ];
        r.list.set_len(3);
        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
                ..Status::default()
            },
        );
        r.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("search-results", &c);
    }
}
