//! A single text field: type on the phone (its keyboard opens automatically)
//! or on a physical keyboard. Enter or the phone's "Done" submits, Esc
//! cancels. Reused for search queries, the glasses' name and (later) Wi-Fi
//! passwords.

use crate::app::action::{Action, TextEdit, TextRequest};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};
use crate::ui::widgets::{hint, status_bar};

use super::nav::{Cx, Nav, Screen};

type OnSubmit = Box<dyn FnOnce(&mut Cx, String)>;

const FIELD_H: f32 = 88.0;

pub struct TextEntry {
    title: String,
    prompt: String,
    value: String,
    max_chars: usize,
    on_submit: Option<OnSubmit>,
}

impl TextEntry {
    pub fn new(
        title: impl Into<String>,
        prompt: impl Into<String>,
        initial: impl Into<String>,
        max_chars: usize,
        on_submit: impl FnOnce(&mut Cx, String) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            value: initial.into(),
            max_chars,
            on_submit: Some(Box::new(on_submit)),
        }
    }

    fn submit(&mut self, cx: &mut Cx) -> Nav {
        if let Some(f) = self.on_submit.take() {
            f(cx, self.value.trim().to_string());
        }
        Nav::Pop
    }
}

impl Screen for TextEntry {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        match action {
            Action::Select => self.submit(cx),
            _ => Nav::Unhandled, // Back cancels
        }
    }

    fn text(&mut self, edit: &TextEdit, cx: &mut Cx) -> Nav {
        match edit {
            TextEdit::Insert(s) => {
                let room = self.max_chars.saturating_sub(self.value.chars().count());
                self.value.extend(s.chars().take(room));
            }
            TextEdit::Backspace => {
                self.value.pop();
            }
            TextEdit::Clear => self.value.clear(),
            TextEdit::Submit => return self.submit(cx),
        }
        Nav::Stay
    }

    fn text_request(&self) -> Option<TextRequest> {
        Some(TextRequest {
            prompt: self.prompt.clone(),
            value: self.value.clone(),
        })
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        let (left, top, width, _) = t.content();

        let mut y = top + space::L;
        c.text(
            typography::BODY,
            &self.prompt,
            left,
            y,
            Align::Left,
            rgb(color::TEXT_SECONDARY),
        );
        y += fonts().line_metrics(typography::BODY).height + space::S;

        // Field: always focused, so it always has the focus ring.
        c.fill_rounded_rect(left, y, width, FIELD_H, space::RADIUS, rgb(color::SURFACE));
        c.stroke_rounded_rect(
            left + space::FOCUS_WIDTH / 2.0,
            y + space::FOCUS_WIDTH / 2.0,
            width - space::FOCUS_WIDTH,
            FIELD_H - space::FOCUS_WIDTH,
            space::RADIUS,
            space::FOCUS_WIDTH,
            rgb(color::FOCUS),
        );

        // Show the end of long text, so the caret is always visible.
        let style = typography::TITLE;
        let inner = width - 2.0 * space::M - space::S;
        let visible = tail_that_fits(&self.value, style, inner);
        let ty = y + (FIELD_H - fonts().line_metrics(style).height) / 2.0;
        let text_w = c.text(
            style,
            visible,
            left + space::M,
            ty,
            Align::Left,
            rgb(color::TEXT),
        );
        let caret_x = left + space::M + text_w + 2.0;
        c.fill_rect(
            caret_x,
            y + space::M,
            3.0,
            FIELD_H - 2.0 * space::M,
            rgb(color::TEXT),
        );

        status_bar::draw(c, t, cx.status, Some(&self.title));
        hint::draw(
            c,
            t,
            "Type on your phone or keyboard · Enter done · Esc cancel",
        );
    }
}

/// Longest suffix of `text` that fits in `width`.
fn tail_that_fits(text: &str, style: crate::ui::text::TextStyle, width: f32) -> &str {
    let mut start = 0;
    while fonts().measure(style, &text[start..]) > width {
        match text[start..].chars().next() {
            Some(ch) => start += ch.len_utf8(),
            None => break,
        }
    }
    &text[start..]
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    fn entry() -> (TextEntry, Rc<RefCell<Option<String>>>) {
        let got = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&got);
        let e = TextEntry::new("Name", "Glasses name", "OR", 6, move |_, v| {
            *sink.borrow_mut() = Some(v)
        });
        (e, got)
    }

    #[test]
    fn edits_respect_the_limit_and_submit_once() {
        let mut h = Harness::new();
        let mut cx = h.cx(Instant::now());
        let (mut e, got) = entry();
        e.text(&TextEdit::Insert("C-4 extra".into()), &mut cx);
        assert_eq!(e.value, "ORC-4 ", "capped at 6 characters");
        e.text(&TextEdit::Backspace, &mut cx);
        assert_eq!(e.text_request().unwrap().value, "ORC-4");
        assert!(matches!(e.text(&TextEdit::Submit, &mut cx), Nav::Pop));
        assert_eq!(got.borrow().as_deref(), Some("ORC-4"));
    }

    #[test]
    fn back_cancels_without_submitting() {
        let mut h = Harness::new();
        let (mut e, got) = entry();
        assert!(matches!(
            e.handle(Action::Back, &mut h.cx(Instant::now())),
            Nav::Unhandled
        ));
        assert!(got.borrow().is_none());
    }

    #[test]
    fn long_text_shows_its_end() {
        let style = typography::TITLE;
        let text = "a fairly long search query about smart glasses";
        let tail = tail_that_fits(text, style, 300.0);
        assert!(text.ends_with(tail) && tail.len() < text.len());
        assert!(fonts().measure(style, tail) <= 300.0);
    }

    #[test]
    fn snapshot_text_entry() {
        let e = TextEntry::new(
            "Settings › Phone",
            "Glasses name",
            "My ORC-4",
            32,
            |_, _| {},
        );
        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
            },
        );
        e.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("text-entry", &c);
    }
}
