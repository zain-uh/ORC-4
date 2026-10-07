//! Screen stack. The launcher is the root; opening an app pushes a screen,
//! Back pops it. Every screen gets the same defaults, transition and toast
//! messages for free, so behaviour is identical everywhere.

use std::time::{Duration, Instant};

use crate::app::action::{Action, Notice, TextEdit, TextRequest};
use crate::config::{Paths, Settings};
use crate::input::remote::link::PhoneLink;
use crate::services::{ServiceEvent, Services};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH, rgba};
use crate::ui::theme::{Theme, color, motion};
use crate::ui::widgets::toast;

/// What a screen may use while handling input or results.
pub struct Cx<'a> {
    pub now: Instant,
    pub settings: &'a mut Settings,
    pub services: &'a Services,
    pub paths: &'a Paths,
    pub link: &'a mut PhoneLink,
    toast: Option<String>,
    exit: bool,
}

impl<'a> Cx<'a> {
    pub fn new(
        now: Instant,
        settings: &'a mut Settings,
        services: &'a Services,
        paths: &'a Paths,
        link: &'a mut PhoneLink,
    ) -> Self {
        Self {
            now,
            settings,
            services,
            paths,
            link,
            toast: None,
            exit: false,
        }
    }

    /// Shows a short message at the bottom of the screen.
    pub fn toast(&mut self, message: impl Into<String>) {
        self.toast = Some(message.into());
    }

    /// Ends the process (systemd restarts it on the device).
    pub fn request_exit(&mut self) {
        self.exit = true;
    }

    pub fn exit_requested(&self) -> bool {
        self.exit
    }

    /// Theme for the current settings, for layout decisions made before drawing.
    pub fn theme(&self) -> Theme {
        Theme::with_edges(self.settings.display.edge_x, self.settings.display.edge_y)
    }

    /// `d`, or zero when the user turned on "Reduce motion".
    pub fn motion(&self, d: Duration) -> Duration {
        if self.settings.display.reduce_motion {
            Duration::ZERO
        } else {
            d
        }
    }
}

pub trait Screen {
    /// Reacts to input. Return `Nav::Unhandled` to get the defaults:
    /// Back pops this screen, Home returns to the launcher.
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav;

    /// Offered every background result, top screen first. Return the event
    /// unchanged (`Err`) if it isn't for this screen.
    fn on_service(&mut self, event: ServiceEvent, _cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        Err(event)
    }

    /// Typing (phone keyboard or physical keyboard). Ignored by default.
    fn text(&mut self, _edit: &TextEdit, _cx: &mut Cx) -> Nav {
        Nav::Unhandled
    }

    /// Something happened elsewhere (a phone connected...). Ignored by default.
    fn on_notice(&mut self, _notice: &Notice, _cx: &mut Cx) -> Nav {
        Nav::Unhandled
    }

    /// `Some` while this screen has a text field focused, so the phone can
    /// open its keyboard.
    fn text_request(&self) -> Option<TextRequest> {
        None
    }

    /// Called when the screen becomes the top one (opened, or returned to).
    fn on_show(&mut self, _cx: &mut Cx) {}

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

struct Toast {
    message: String,
    start: Instant,
    /// 0..1 fade level, sampled by `animate`.
    opacity: f32,
}

pub struct Navigator {
    stack: Vec<Box<dyn Screen>>,
    transition: Option<Transition>,
    toast: Option<Toast>,
}

impl Navigator {
    const TOAST_FOR: Duration = Duration::from_millis(2500);
    const TOAST_FADE: Duration = Duration::from_millis(150);

    pub fn new(root: Box<dyn Screen>) -> Self {
        Self {
            stack: vec![root],
            transition: None,
            toast: None,
        }
    }

    /// Call once before the first frame.
    pub fn start(&mut self, cx: &mut Cx) {
        self.top_mut().on_show(cx);
        self.take_toast(cx);
    }

    /// Routes an action to the top screen. Returns true if a redraw is needed.
    pub fn handle(&mut self, action: Action, cx: &mut Cx) -> bool {
        let nav = match self.top_mut().handle(action, cx) {
            Nav::Unhandled => match action {
                Action::Back => Nav::Pop,
                Action::Home => Nav::Home,
                _ => Nav::Unhandled,
            },
            nav => nav,
        };
        let redraw = self.apply(nav, cx);
        self.take_toast(cx) || redraw
    }

    /// Routes typing to the top screen. Returns true if a redraw is needed.
    pub fn text(&mut self, edit: &TextEdit, cx: &mut Cx) -> bool {
        let nav = self.top_mut().text(edit, cx);
        let redraw = self.apply(nav, cx);
        self.take_toast(cx) || redraw
    }

    /// Tells the top screen about `notice`; phone connections also get a
    /// toast if the screen doesn't handle them itself.
    pub fn notify(&mut self, notice: &Notice, cx: &mut Cx) -> bool {
        let nav = self.top_mut().on_notice(notice, cx);
        if matches!(nav, Nav::Unhandled) {
            match notice {
                Notice::PhonePaired(name) => cx.toast(format!("Paired with {name}")),
                Notice::PhoneConnected(name) => cx.toast(format!("{name} connected")),
                Notice::PhoneDisconnected(name) => cx.toast(format!("{name} disconnected")),
            }
        }
        let redraw = self.apply(nav, cx);
        self.take_toast(cx) || redraw
    }

    /// The top screen's text request.
    pub fn text_request(&self) -> Option<TextRequest> {
        self.top().text_request()
    }

    /// Offers a background result to each screen, top first. Returns true if
    /// a redraw is needed.
    pub fn dispatch(&mut self, event: ServiceEvent, cx: &mut Cx) -> bool {
        let top = self.stack.len() - 1;
        let mut event = event;
        let mut redraw = false;
        for i in (0..self.stack.len()).rev() {
            match self.stack[i].on_service(event, cx) {
                Ok(nav) => {
                    // Screens below the top update their state silently;
                    // only the top screen may navigate.
                    if i == top {
                        redraw = self.apply(nav, cx);
                    }
                    break;
                }
                Err(e) => event = e,
            }
        }
        self.take_toast(cx) || redraw
    }

    /// Advances the top screen, any transition and the toast. Returns true if
    /// the picture changed.
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

        if let Some(toast) = &mut self.toast {
            let age = now.saturating_duration_since(toast.start);
            let (fade, total) = (Self::TOAST_FADE, Self::TOAST_FOR);
            let opacity = if age >= total {
                0.0
            } else if age < fade {
                age.as_secs_f32() / fade.as_secs_f32()
            } else if age > total - fade {
                (total - age).as_secs_f32() / fade.as_secs_f32()
            } else {
                1.0
            };
            if opacity != toast.opacity {
                toast.opacity = opacity;
                changed = true;
            }
            if age >= total {
                self.toast = None;
            }
        }
        changed
    }

    pub fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        match &self.transition {
            None => self.top().draw(c, cx),
            Some(t) => {
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
        }
        if let Some(t) = &self.toast {
            toast::draw(c, cx.theme, &t.message, t.opacity);
        }
    }

    fn apply(&mut self, nav: Nav, cx: &mut Cx) -> bool {
        match nav {
            Nav::Stay => return true,
            Nav::Unhandled => return false,
            Nav::Push(screen) => {
                self.stack.push(screen);
                self.begin(Outgoing::Below, cx);
            }
            Nav::Pop | Nav::Home if self.stack.len() == 1 => return false,
            Nav::Pop => {
                let popped = self.stack.pop().expect("len > 1");
                self.begin(Outgoing::Popped(popped), cx);
            }
            Nav::Home => {
                let popped = self.stack.pop().expect("len > 1");
                self.stack.truncate(1);
                self.begin(Outgoing::Popped(popped), cx);
            }
        }
        self.top_mut().on_show(cx);
        true
    }

    fn begin(&mut self, outgoing: Outgoing, cx: &Cx) {
        self.transition = if cx.motion(motion::SCREEN).is_zero() {
            None
        } else {
            Some(Transition {
                start: cx.now,
                outgoing,
                progress: 0.0,
            })
        };
    }

    /// Moves a toast requested through `cx` into the navigator.
    fn take_toast(&mut self, cx: &mut Cx) -> bool {
        let Some(message) = cx.toast.take() else {
            return false;
        };
        self.toast = Some(Toast {
            message,
            start: cx.now,
            opacity: if cx.motion(Self::TOAST_FADE).is_zero() {
                1.0
            } else {
                0.0
            },
        });
        true
    }

    fn top(&self) -> &dyn Screen {
        self.stack.last().expect("root is never popped").as_ref()
    }

    fn top_mut(&mut self) -> &mut Box<dyn Screen> {
        self.stack.last_mut().expect("root is never popped")
    }
}

#[cfg(test)]
pub mod testing {
    //! Helpers for driving screens in tests.
    use std::sync::mpsc;

    use super::*;

    pub struct Harness {
        pub settings: Settings,
        pub services: Services,
        pub paths: Paths,
        pub link: PhoneLink,
        pub events: mpsc::Receiver<crate::event::Event>,
    }

    impl Harness {
        pub fn new() -> Self {
            let (tx, events) = mpsc::channel();
            let dir = std::env::temp_dir().join(format!("orc4-harness-{}", std::process::id()));
            Self {
                settings: Settings::default(),
                services: Services::start(tx),
                paths: Paths {
                    system_config: dir.join("system.toml"),
                    data_dir: dir,
                },
                link: PhoneLink::new(),
                events,
            }
        }

        pub fn cx(&mut self, now: Instant) -> Cx<'_> {
            Cx::new(
                now,
                &mut self.settings,
                &self.services,
                &self.paths,
                &mut self.link,
            )
        }

        /// Next background result, waiting up to 5 s.
        pub fn next_service_event(&self) -> ServiceEvent {
            match self.events.recv_timeout(Duration::from_secs(5)) {
                Ok(crate::event::Event::Service(e)) => e,
                other => panic!("expected a service event, got {other:?}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::testing::Harness;
    use super::*;

    /// Opens a child on Select; counts on_show calls.
    struct Probe {
        shows: Rc<Cell<u32>>,
    }

    impl Screen for Probe {
        fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
            match action {
                Action::Select => Nav::Push(Box::new(Probe {
                    shows: Rc::clone(&self.shows),
                })),
                Action::Menu => {
                    cx.toast("hello");
                    Nav::Stay
                }
                _ => Nav::Unhandled,
            }
        }
        fn on_show(&mut self, _cx: &mut Cx) {
            self.shows.set(self.shows.get() + 1);
        }
        fn draw(&self, _c: &mut Canvas, _cx: &DrawCx) {}
    }

    fn nav() -> (Navigator, Rc<Cell<u32>>) {
        let shows = Rc::new(Cell::new(0));
        let n = Navigator::new(Box::new(Probe {
            shows: Rc::clone(&shows),
        }));
        (n, shows)
    }

    #[test]
    fn back_pops_and_root_ignores_back() {
        let mut h = Harness::new();
        let (mut n, _) = nav();
        let mut cx = h.cx(Instant::now());
        assert!(!n.handle(Action::Back, &mut cx), "root: nothing to pop");
        assert!(n.handle(Action::Select, &mut cx));
        assert!(n.handle(Action::Select, &mut cx));
        assert_eq!(n.stack.len(), 3);
        assert!(n.handle(Action::Back, &mut cx));
        assert_eq!(n.stack.len(), 2);
    }

    #[test]
    fn home_returns_to_root_from_any_depth() {
        let mut h = Harness::new();
        let (mut n, _) = nav();
        let mut cx = h.cx(Instant::now());
        for _ in 0..3 {
            n.handle(Action::Select, &mut cx);
        }
        assert!(n.handle(Action::Home, &mut cx));
        assert_eq!(n.stack.len(), 1);
        assert!(!n.handle(Action::Home, &mut cx), "already home");
    }

    #[test]
    fn on_show_runs_when_opened_and_when_returned_to() {
        let mut h = Harness::new();
        let (mut n, shows) = nav();
        let mut cx = h.cx(Instant::now());
        n.start(&mut cx);
        assert_eq!(shows.get(), 1);
        n.handle(Action::Select, &mut cx);
        assert_eq!(shows.get(), 2, "child shown");
        n.handle(Action::Back, &mut cx);
        assert_eq!(shows.get(), 3, "root shown again");
    }

    #[test]
    fn unhandled_non_navigation_actions_do_not_redraw() {
        let mut h = Harness::new();
        let (mut n, _) = nav();
        assert!(!n.handle(Action::Up, &mut h.cx(Instant::now())));
    }

    #[test]
    fn transition_runs_then_ends_and_reduce_motion_skips_it() {
        let mut h = Harness::new();
        let (mut n, _) = nav();
        let t0 = Instant::now();
        n.handle(Action::Select, &mut h.cx(t0));
        assert!(n.animate(t0 + motion::SCREEN / 2));
        assert!(n.transition.is_some());
        assert!(n.animate(t0 + motion::SCREEN), "final frame");
        assert!(n.transition.is_none());

        h.settings.display.reduce_motion = true;
        n.handle(Action::Select, &mut h.cx(t0));
        assert!(n.transition.is_none());
    }

    #[test]
    fn toast_appears_then_expires() {
        let mut h = Harness::new();
        let (mut n, _) = nav();
        let t0 = Instant::now();
        assert!(n.handle(Action::Menu, &mut h.cx(t0)));
        assert!(n.animate(t0 + Navigator::TOAST_FOR / 2));
        assert_eq!(n.toast.as_ref().unwrap().opacity, 1.0);
        assert!(n.animate(t0 + Navigator::TOAST_FOR));
        assert!(n.toast.is_none());
    }
}
