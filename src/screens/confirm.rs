//! "Are you sure?" dialog for destructive actions. Focus starts on Cancel,
//! so pressing Enter twice by accident never deletes anything.

use crate::app::action::Action;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, WIDTH, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};
use crate::ui::widgets::hint;

use super::nav::{Cx, Nav, Screen};

const BUTTON_W: f32 = 260.0;
const BUTTON_H: f32 = 76.0;
const BUTTON_Y: f32 = 320.0;

type OnConfirm = Box<dyn FnOnce(&mut Cx)>;

pub struct Confirm {
    question: String,
    action_label: &'static str,
    /// 0 = Cancel, 1 = the action.
    focus: usize,
    on_confirm: Option<OnConfirm>,
}

impl Confirm {
    pub fn new(
        question: impl Into<String>,
        action_label: &'static str,
        on_confirm: impl FnOnce(&mut Cx) + 'static,
    ) -> Self {
        Self {
            question: question.into(),
            action_label,
            focus: 0,
            on_confirm: Some(Box::new(on_confirm)),
        }
    }
}

impl Screen for Confirm {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        match action {
            Action::Left => self.focus = 0,
            Action::Right => self.focus = 1,
            Action::Select => {
                if self.focus == 1
                    && let Some(f) = self.on_confirm.take()
                {
                    f(cx);
                }
                return Nav::Pop;
            }
            _ => return Nav::Unhandled, // Back = cancel
        }
        Nav::Stay
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));

        let width = WIDTH as f32 - t.safe.left - t.safe.right;
        c.paragraph(
            typography::TITLE,
            &self.question,
            t.safe.left,
            150.0,
            width,
            2,
            Align::Center,
            rgb(color::TEXT),
        );

        let gap = space::L;
        let left = (WIDTH as f32 - 2.0 * BUTTON_W - gap) / 2.0;
        let buttons = [
            ("Cancel", color::SURFACE_RAISED),
            (self.action_label, color::DANGER),
        ];
        for (i, (label, fill)) in buttons.iter().enumerate() {
            let x = left + i as f32 * (BUTTON_W + gap);
            c.fill_rounded_rect(x, BUTTON_Y, BUTTON_W, BUTTON_H, space::RADIUS, rgb(*fill));
            if i == self.focus {
                let g = space::FOCUS_GAP + space::FOCUS_WIDTH / 2.0;
                c.stroke_rounded_rect(
                    x - g,
                    BUTTON_Y - g,
                    BUTTON_W + 2.0 * g,
                    BUTTON_H + 2.0 * g,
                    space::RADIUS + g,
                    space::FOCUS_WIDTH,
                    rgb(color::FOCUS),
                );
            }
            let style = typography::BODY;
            let ty = BUTTON_Y + (BUTTON_H - fonts().line_metrics(style).height) / 2.0;
            c.text(
                style,
                label,
                x + BUTTON_W / 2.0,
                ty,
                Align::Center,
                rgb(color::TEXT),
            );
        }

        hint::draw(c, t, "← → choose · Enter confirm · Esc cancel");
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;

    fn confirm() -> (Confirm, Rc<Cell<bool>>) {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        (
            Confirm::new("Delete x?", "Delete", move |_| flag.set(true)),
            ran,
        )
    }

    #[test]
    fn enter_on_default_focus_cancels() {
        let mut h = Harness::new();
        let (mut c, ran) = confirm();
        assert!(matches!(
            c.handle(Action::Select, &mut h.cx(Instant::now())),
            Nav::Pop
        ));
        assert!(!ran.get());
    }

    #[test]
    fn choosing_the_action_runs_it_once() {
        let mut h = Harness::new();
        let (mut c, ran) = confirm();
        let mut cx = h.cx(Instant::now());
        c.handle(Action::Right, &mut cx);
        assert!(matches!(c.handle(Action::Select, &mut cx), Nav::Pop));
        assert!(ran.get());
    }

    #[test]
    fn back_cancels() {
        let mut h = Harness::new();
        let (mut c, ran) = confirm();
        assert!(matches!(
            c.handle(Action::Back, &mut h.cx(Instant::now())),
            Nav::Unhandled
        ));
        assert!(!ran.get());
    }
}
