//! YouTube app: a search box plus recent searches, then a results carousel.
//! Searching needs the internet and the `yt-dlp` program; both failures are
//! shown as plain messages, and nothing else in ORC-4 depends on them.
//! Playback arrives in M7.

mod results;

use std::time::Instant;

use crate::app::action::Action;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::icons::Icon;
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use super::confirm::Confirm;
use super::nav::{Cx, Nav, Screen};
use super::text_entry::TextEntry;
use results::Results;

pub struct YoutubeHome {
    recent: Vec<String>,
    list: ListView,
}

impl YoutubeHome {
    pub fn new(cx: &Cx) -> Self {
        let recent = cx.settings.youtube.recent.clone();
        Self {
            list: ListView::new(recent.len() + 1),
            recent,
        }
    }
}

/// Opens results for `query` and remembers it.
fn open_results(query: String, cx: &mut Cx) -> Box<dyn Screen> {
    cx.settings.youtube.remember(&query);
    Box::new(Results::new(query, cx))
}

impl Screen for YoutubeHome {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        let focus = self.list.focus();
        match action {
            Action::Select if focus == 0 => Nav::Push(Box::new(TextEntry::new(
                "YouTube",
                "Search YouTube",
                "",
                100,
                |cx, query| (!query.is_empty()).then(|| open_results(query, cx)),
            ))),
            Action::Select => match self.recent.get(focus - 1) {
                Some(query) => Nav::Push(open_results(query.clone(), cx)),
                None => Nav::Stay,
            },
            Action::Menu if !self.recent.is_empty() => Nav::Push(Box::new(Confirm::new(
                "Clear search history?",
                "Clear",
                |cx| {
                    cx.settings.youtube.recent.clear();
                    cx.toast("Search history cleared");
                },
            ))),
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.recent = cx.settings.youtube.recent.clone();
        self.list.set_len(self.recent.len() + 1);
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let mut rows = vec![Row {
            icon: Some(Icon::Search),
            label: "Search YouTube",
            detail: "›".into(),
        }];
        rows.extend(self.recent.iter().map(|q| Row {
            icon: Some(Icon::Search),
            label: q,
            detail: String::new(),
        }));
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("YouTube"));
        let hints = if self.recent.is_empty() {
            "Enter search · Esc back"
        } else {
            "Enter search · Tab clear history · Esc back"
        };
        hint::draw(c, cx.theme, hints);
    }
}
