//! "Search box + recent searches" screen shared by YouTube and Search.
//! Typing on glasses is slow, so recent searches are one press away; Tab
//! clears them.

use std::time::Instant;

use crate::app::action::Action;
use crate::config::{Settings, remember_query};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::icons::Icon;
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, status_bar};

use super::confirm::Confirm;
use super::nav::{Cx, Nav, Screen};
use super::text_entry::TextEntry;

/// Where an app keeps its recent searches.
pub type History = fn(&mut Settings) -> &mut Vec<String>;
/// Opens the results screen for a query.
pub type OpenResults = fn(String, &mut Cx) -> Box<dyn Screen>;

pub struct QueryHome {
    title: &'static str,
    prompt: &'static str,
    history: History,
    open: OpenResults,
    recent: Vec<String>,
    list: ListView,
}

impl QueryHome {
    pub fn new(
        title: &'static str,
        prompt: &'static str,
        history: History,
        open: OpenResults,
        cx: &mut Cx,
    ) -> Self {
        let recent = history(cx.settings).clone();
        Self {
            title,
            prompt,
            history,
            open,
            list: ListView::new(recent.len() + 1),
            recent,
        }
    }
}

/// Remembers the query, then opens its results.
fn search(query: String, history: History, open: OpenResults, cx: &mut Cx) -> Box<dyn Screen> {
    remember_query(history(cx.settings), &query);
    open(query, cx)
}

impl Screen for QueryHome {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        let focus = self.list.focus();
        let (history, open) = (self.history, self.open);
        match action {
            Action::Select if focus == 0 => Nav::Push(Box::new(TextEntry::new(
                self.title,
                self.prompt,
                "",
                100,
                move |cx, query| (!query.is_empty()).then(|| search(query, history, open, cx)),
            ))),
            Action::Select => match self.recent.get(focus - 1) {
                Some(query) => Nav::Push(search(query.clone(), history, open, cx)),
                None => Nav::Stay,
            },
            Action::Menu if !self.recent.is_empty() => Nav::Push(Box::new(Confirm::new(
                "Clear search history?",
                "Clear",
                move |cx| {
                    history(cx.settings).clear();
                    cx.toast("Search history cleared");
                },
            ))),
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.recent = (self.history)(cx.settings).clone();
        self.list.set_len(self.recent.len() + 1);
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let mut rows = vec![Row {
            icon: Some(Icon::Search),
            label: self.prompt,
            detail: "›".into(),
        }];
        rows.extend(self.recent.iter().map(|q| Row {
            icon: Some(Icon::Search),
            label: q,
            detail: String::new(),
        }));
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some(self.title));
        let hints = if self.recent.is_empty() {
            "Enter search · Esc back"
        } else {
            "Enter search · Tab clear history · Esc back"
        };
        hint::draw(c, cx.theme, hints);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::nav::testing::Harness;

    /// Stand-in results screen.
    struct Blank;

    impl Screen for Blank {
        fn handle(&mut self, _: Action, _: &mut Cx) -> Nav {
            Nav::Unhandled
        }
        fn draw(&self, _: &mut Canvas, _: &DrawCx) {}
    }

    fn home(h: &mut Harness) -> QueryHome {
        QueryHome::new(
            "Test",
            "Search tests",
            |s| &mut s.search.recent,
            |_, _| Box::new(Blank),
            &mut h.cx(Instant::now()),
        )
    }

    #[test]
    fn choosing_a_recent_search_moves_it_to_the_front() {
        let mut h = Harness::new();
        h.settings.search.recent = vec!["owls".into(), "Rust".into()];
        let mut q = home(&mut h);
        let mut cx = h.cx(Instant::now());
        q.handle(Action::Down, &mut cx);
        q.handle(Action::Down, &mut cx); // "Rust"
        assert!(matches!(q.handle(Action::Select, &mut cx), Nav::Push(_)));
        assert_eq!(cx.settings.search.recent, ["Rust", "owls"]);
    }

    #[test]
    fn clearing_history_needs_confirmation() {
        let mut h = Harness::new();
        h.settings.search.recent = vec!["owls".into()];
        let mut q = home(&mut h);
        let mut cx = h.cx(Instant::now());
        let Nav::Push(mut confirm) = q.handle(Action::Menu, &mut cx) else {
            panic!("expected confirmation")
        };
        confirm.handle(Action::Right, &mut cx);
        confirm.handle(Action::Select, &mut cx);
        assert!(cx.settings.search.recent.is_empty());
    }
}
