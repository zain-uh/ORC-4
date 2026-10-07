//! Screen stack. The launcher is the root; opening an app pushes a screen,
//! Back pops it. Every screen gets the same defaults and the same transition
//! for free, so "back" behaves identically everywhere.

use std::time::Instant;

use super::DrawCx;
use super::canvas::{Canvas, HEIGHT, WIDTH, rgba};
use super::theme::{color, motion};
use crate::app::action::Action;

pub trait Screen {
    /// Reacts to input. Return `Nav::Unhandled` to get the defaults:
    /// Back pops this screen, Home returns to the launcher.
    fn handle(&mut self, action: Action, now: Instant) -> Nav;

    /// Advances animations. Returns true if the picture changed.
    fn animate(&mut self, _now: Instant) -> bool {
        false
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx);
}

pub enum Nav {
    /// Handled; redraw.
    Stay,
    /// Not handled; apply the navigator's default for this action.
    Unhandled,
    Push(Box<dyn Screen>),
    Pop,
    Home,
}

/// What is shown during the first half of a transition.
enum Outgoing {
    /// After a push: the screen now second from the top.
    Below,
    /// After a pop or Home: the removed screen, kept alive until it fades.
    Popped(Box<dyn Screen>),
}

/// Dip to black: the old screen fades out, the new one fades in. Cheap (one
/// screen drawn per frame), and on see-through optics black is transparent.
struct Transition {
    start: Instant,
    outgoing: Outgoing,
    progress: f32,
}

pub struct Navigator {
    stack: Vec<Box<dyn Screen>>,
    transition: Option<Transition>,
}

impl Navigator {
    pub fn new(root: Box<dyn Screen>) -> Self {
        Self {
            stack: vec![root],
            transition: None,
        }
    }

    /// Routes an action to the top screen. Returns true if a redraw is needed.
    pub fn handle(&mut self, action: Action, now: Instant) -> bool {
        let nav = match self.top_mut().handle(action, now) {
            Nav::Unhandled => match action {
                Action::Back => Nav::Pop,
                Action::Home => Nav::Home,
                _ => return false,
            },
            nav => nav,
        };

        match nav {
            Nav::Stay | Nav::Unhandled => true,
            Nav::Push(screen) => {
                self.stack.push(screen);
                self.begin(Outgoing::Below, now);
                true
            }
            Nav::Pop | Nav::Home if self.stack.len() == 1 => false,
            Nav::Pop => {
                let popped = self.stack.pop().expect("len > 1");
                self.begin(Outgoing::Popped(popped), now);
                true
            }
            Nav::Home => {
                let popped = self.stack.pop().expect("len > 1");
                self.stack.truncate(1);
                self.begin(Outgoing::Popped(popped), now);
                true
            }
        }
    }

    /// Advances the top screen and any transition. Returns true if the
    /// picture changed.
    pub fn animate(&mut self, now: Instant) -> bool {
        let mut changed = self.top_mut().animate(now);
        if let Some(t) = &mut self.transition {
            let p =
                now.saturating_duration_since(t.start).as_secs_f32() / motion::SCREEN.as_secs_f32();
            if p >= 1.0 {
                self.transition = None; // final frame: new screen, no overlay
            } else {
                t.progress = p;
            }
            changed = true;
        }
        changed
    }

    pub fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let Some(t) = &self.transition else {
            self.top().draw(c, cx);
            return;
        };
        let darkness = if t.progress < 0.5 {
            let outgoing: &dyn Screen = match &t.outgoing {
                Outgoing::Popped(screen) => screen.as_ref(),
                Outgoing::Below => self
                    .stack
                    .len()
                    .checked_sub(2)
                    .map_or(self.top(), |i| self.stack[i].as_ref()),
            };
            outgoing.draw(c, cx);
            t.progress * 2.0
        } else {
            self.top().draw(c, cx);
            (1.0 - t.progress) * 2.0
        };
        c.fill_rect(
            0.0,
            0.0,
            WIDTH as f32,
            HEIGHT as f32,
            rgba(color::BACKGROUND, darkness),
        );
    }

    fn begin(&mut self, outgoing: Outgoing, now: Instant) {
        self.transition = Some(Transition {
            start: now,
            outgoing,
            progress: 0.0,
        });
    }

    fn top(&self) -> &dyn Screen {
        self.stack.last().expect("root is never popped").as_ref()
    }

    fn top_mut(&mut self) -> &mut Box<dyn Screen> {
        self.stack.last_mut().expect("root is never popped")
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use super::*;

    /// Opens a child on Select; everything else falls through to defaults.
    struct Probe {
        draws: Rc<Cell<u32>>,
    }

    impl Screen for Probe {
        fn handle(&mut self, action: Action, _now: Instant) -> Nav {
            match action {
                Action::Select => Nav::Push(Box::new(Probe {
                    draws: Rc::clone(&self.draws),
                })),
                _ => Nav::Unhandled,
            }
        }
        fn draw(&self, _c: &mut Canvas, _cx: &DrawCx) {
            self.draws.set(self.draws.get() + 1);
        }
    }

    fn nav() -> Navigator {
        Navigator::new(Box::new(Probe {
            draws: Rc::new(Cell::new(0)),
        }))
    }

    #[test]
    fn back_pops_and_root_ignores_back() {
        let mut n = nav();
        let t = Instant::now();
        assert!(!n.handle(Action::Back, t), "root: nothing to pop");
        assert!(n.handle(Action::Select, t));
        assert!(n.handle(Action::Select, t));
        assert_eq!(n.stack.len(), 3);
        assert!(n.handle(Action::Back, t));
        assert_eq!(n.stack.len(), 2);
    }

    #[test]
    fn home_returns_to_root_from_any_depth() {
        let mut n = nav();
        let t = Instant::now();
        for _ in 0..3 {
            n.handle(Action::Select, t);
        }
        assert!(n.handle(Action::Home, t));
        assert_eq!(n.stack.len(), 1);
        assert!(!n.handle(Action::Home, t), "already home");
    }

    #[test]
    fn unhandled_non_navigation_actions_do_not_redraw() {
        assert!(!nav().handle(Action::Menu, Instant::now()));
    }

    #[test]
    fn transition_runs_then_ends() {
        let mut n = nav();
        let t0 = Instant::now();
        n.handle(Action::Select, t0);
        assert!(n.animate(t0 + motion::SCREEN / 2));
        assert!(n.transition.is_some());
        assert!(n.animate(t0 + motion::SCREEN), "final frame");
        assert!(n.transition.is_none());
        assert!(!n.animate(t0 + motion::SCREEN + Duration::from_millis(1)));
    }
}
