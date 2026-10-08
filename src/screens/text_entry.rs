//! A single text field: type on the phone (its keyboard opens automatically),
//! on a physical keyboard, or with the on-screen keyboard (↓ from the field;
//! arrows and Enter only). Enter on the field or "Done" submits, Esc
//! cancels. Used for search queries, the glasses' name and Wi-Fi passwords.

use crate::app::action::{Action, TextEdit, TextRequest};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};
use crate::ui::widgets::keyboard::{Keyboard, Press};
use crate::ui::widgets::{hint, status_bar};

use super::nav::{Cx, Nav, Screen};

/// Called with the entered text. May return a screen to open in place of
/// the text entry (e.g. search results).
type OnSubmit = Box<dyn FnOnce(&mut Cx, String) -> Option<Box<dyn Screen>>>;

const FIELD_H: f32 = 64.0;

pub struct TextEntry {
    title: String,
    prompt: String,
    value: String,
    max_chars: usize,
    /// Passwords keep leading/trailing spaces; other text is trimmed.
    trim: bool,
    on_submit: Option<OnSubmit>,
    keyboard: Keyboard,
    /// Focus is on the on-screen keyboard (else on the field).
    on_keys: bool,
}

impl TextEntry {
    pub fn new(
        title: impl Into<String>,
        prompt: impl Into<String>,
        initial: impl Into<String>,
        max_chars: usize,
        on_submit: impl FnOnce(&mut Cx, String) -> Option<Box<dyn Screen>> + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            value: initial.into(),
            max_chars,
            trim: true,
            on_submit: Some(Box::new(on_submit)),
            keyboard: Keyboard::new(),
            on_keys: false,
        }
    }

    /// Submit the text exactly as typed (for passwords).
    pub fn keep_spaces(mut self) -> Self {
        self.trim = false;
        self
    }

    fn submit(&mut self, cx: &mut Cx) -> Nav {
        let value = if self.trim {
            self.value.trim().to_string()
        } else {
            self.value.clone()
        };
        match self.on_submit.take() {
            Some(f) => match f(cx, value) {
                Some(next) => Nav::Replace(next),
                None => Nav::Pop,
            },
            None => Nav::Pop,
        }
    }

    fn insert(&mut self, s: &str) {
        let room = self.max_chars.saturating_sub(self.value.chars().count());
        self.value.extend(s.chars().take(room));
    }
}

impl Screen for TextEntry {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if !self.on_keys {
            return match action {
                Action::Select => self.submit(cx),
                Action::Down => {
                    self.on_keys = true;
                    Nav::Stay
                }
                _ => Nav::Unhandled, // Back cancels
            };
        }
        match action {
            Action::Left => {
                self.keyboard.move_by(-1, 0);
            }
            Action::Right => {
                self.keyboard.move_by(1, 0);
            }
            Action::Down => {
                self.keyboard.move_by(0, 1);
            }
            Action::Up => self.on_keys = self.keyboard.move_by(0, -1),
            Action::Select => match self.keyboard.press() {
                Some(Press::Type(ch)) => self.insert(&ch.to_string()),
                Some(Press::Backspace) => {
                    self.value.pop();
                }
                Some(Press::Done) => return self.submit(cx),
                None => {}
            },
            _ => return Nav::Unhandled,
        }
        Nav::Stay
    }

    fn text(&mut self, edit: &TextEdit, cx: &mut Cx) -> Nav {
        match edit {
            TextEdit::Insert(s) => self.insert(s),
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

        let mut y = top + space::XS;
        c.text(
            typography::BODY,
            &self.prompt,
            left,
            y,
            Align::Left,
            rgb(color::TEXT_SECONDARY),
        );
        y += fonts().line_metrics(typography::BODY).height + space::XS;

        c.fill_rounded_rect(left, y, width, FIELD_H, space::RADIUS, rgb(color::SURFACE));
        if !self.on_keys {
            c.stroke_rounded_rect(
                left + space::FOCUS_WIDTH / 2.0,
                y + space::FOCUS_WIDTH / 2.0,
                width - space::FOCUS_WIDTH,
                FIELD_H - space::FOCUS_WIDTH,
                space::RADIUS,
                space::FOCUS_WIDTH,
                rgb(color::FOCUS),
            );
        }

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
        c.fill_rect(
            left + space::M + text_w + 2.0,
            y + space::S,
            3.0,
            FIELD_H - 2.0 * space::S,
            rgb(color::TEXT),
        );
        y += FIELD_H + space::S;

        self.keyboard.draw(c, left, y, width, self.on_keys);

        status_bar::draw(c, t, cx.status, Some(&self.title));
        hint::draw(
            c,
            t,
            if self.on_keys {
                "Enter type · ↑ back to the field · Esc cancel"
            } else {
                "Phone or keyboard · ↓ on-screen keys · Enter done · Esc cancel"
            },
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
            *sink.borrow_mut() = Some(v);
            None
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
    fn on_screen_keyboard_types_and_up_returns_to_the_field() {
        let mut h = Harness::new();
        let mut cx = h.cx(Instant::now());
        let (mut e, got) = entry();
        e.text(&TextEdit::Clear, &mut cx);
        e.handle(Action::Down, &mut cx); // into the keys, on "1"
        e.handle(Action::Select, &mut cx);
        e.handle(Action::Right, &mut cx);
        e.handle(Action::Select, &mut cx);
        assert_eq!(e.value, "12");
        e.handle(Action::Up, &mut cx); // back to the field
        assert!(!e.on_keys);
        assert!(matches!(e.handle(Action::Select, &mut cx), Nav::Pop));
        assert_eq!(got.borrow().as_deref(), Some("12"));
    }

    #[test]
    fn passwords_keep_their_spaces() {
        let mut h = Harness::new();
        let got = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&got);
        let mut e = TextEntry::new("Wi-Fi", "Password", "", 63, move |_, v| {
            *sink.borrow_mut() = Some(v);
            None
        })
        .keep_spaces();
        let mut cx = h.cx(Instant::now());
        e.text(&TextEdit::Insert(" pass word ".into()), &mut cx);
        e.text(&TextEdit::Submit, &mut cx);
        assert_eq!(got.borrow().as_deref(), Some(" pass word "));
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
            |_, _| None,
        );
        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
                ..Status::default()
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
