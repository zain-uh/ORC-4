//! Settings → Phone: pair a phone, manage paired phones, name the glasses,
//! turn the link off.

use std::net::SocketAddr;
use std::time::Instant;

use crate::app::action::{Action, Notice};
use crate::config::{PairedPhone, Settings};
use crate::screens::confirm::Confirm;
use crate::screens::nav::{Cx, Nav, Screen};
use crate::screens::text_entry::TextEntry;
use crate::services::system;
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, FontId, TextStyle, fonts};
use crate::ui::theme::{color, motion, space, typography};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, message, status_bar};

/// Where a phone should connect, e.g. "192.168.1.23:4004".
pub fn link_address(settings: &Settings) -> String {
    match settings.phone.listen.parse::<SocketAddr>() {
        Ok(addr) if addr.ip().is_unspecified() => match system::local_ip() {
            Some(ip) => format!("{ip}:{}", addr.port()),
            None => "No network".into(),
        },
        Ok(addr) => addr.to_string(),
        Err(_) => settings.phone.listen.clone(),
    }
}

const ROWS: usize = 5;

pub struct PhonePage {
    list: ListView,
    name: String,
    enabled: bool,
    paired: usize,
    connected: Vec<String>,
    address: String,
}

impl PhonePage {
    pub fn new(cx: &Cx) -> Self {
        let mut page = Self {
            list: ListView::new(ROWS),
            name: String::new(),
            enabled: false,
            paired: 0,
            connected: Vec::new(),
            address: String::new(),
        };
        page.refresh(cx);
        page
    }

    fn refresh(&mut self, cx: &Cx) {
        let p = &cx.settings.phone;
        self.name = p.name.clone();
        self.enabled = p.enabled;
        self.paired = p.paired.len();
        self.connected = cx.link.connected_phones();
        self.address = link_address(cx.settings);
    }
}

impl Screen for PhonePage {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        if action != Action::Select {
            return Nav::Unhandled;
        }
        let nav = match self.list.focus() {
            0 => Nav::Push(Box::new(Pairing::new())),
            1 => Nav::Push(Box::new(PairedPhones::new(cx))),
            2 => Nav::Push(Box::new(TextEntry::new(
                "Settings › Phone",
                "Glasses name",
                cx.settings.phone.name.clone(),
                32,
                |cx, name| {
                    if !name.is_empty() {
                        cx.settings.phone.name = name;
                    }
                },
            ))),
            3 => {
                cx.settings.phone.enabled = !cx.settings.phone.enabled;
                Nav::Stay
            }
            _ => Nav::Stay, // address: read-only
        };
        self.refresh(cx);
        nav
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.refresh(cx);
    }

    fn on_notice(&mut self, _notice: &Notice, cx: &mut Cx) -> Nav {
        self.refresh(cx);
        Nav::Unhandled // keep the default toast
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let paired = match (self.paired, self.connected.len()) {
            (0, _) => "None  ›".to_string(),
            (n, 0) => format!("{n}  ›"),
            (n, k) => format!("{n} · {k} connected  ›"),
        };
        let rows = [
            Row {
                icon: None,
                label: "Pair a phone",
                detail: "›".into(),
            },
            Row {
                icon: None,
                label: "Paired phones",
                detail: paired,
            },
            Row {
                icon: None,
                label: "Glasses name",
                detail: format!("{}  ›", self.name),
            },
            Row {
                icon: None,
                label: "Phone link",
                detail: if self.enabled { "On" } else { "Off" }.into(),
            },
            Row {
                icon: None,
                label: "Address",
                detail: self.address.clone(),
            },
        ];
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, cx.status, Some("Settings › Phone"));
        hint::draw(c, cx.theme, "Enter choose · Esc back");
    }
}

/// Shows the pairing code while open. Closing it ends pairing; a phone
/// pairing successfully closes it.
pub struct Pairing {
    code: String,
    started: Instant,
    expired: bool,
    enabled: bool,
    address: String,
}

impl Pairing {
    fn new() -> Self {
        Self {
            code: String::new(),
            started: Instant::now(),
            expired: false,
            enabled: true,
            address: String::new(),
        }
    }

    fn new_code(&mut self, cx: &mut Cx) {
        self.code = cx.link.start_pairing(cx.now);
        self.started = cx.now;
        self.expired = false;
    }
}

impl Screen for Pairing {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        match action {
            Action::Select if self.enabled => {
                self.new_code(cx);
                Nav::Stay
            }
            Action::Back | Action::Home => {
                cx.link.stop_pairing();
                Nav::Unhandled
            }
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.enabled = cx.settings.phone.enabled;
        self.address = link_address(cx.settings);
        if self.enabled {
            self.new_code(cx);
        }
    }

    fn on_notice(&mut self, notice: &Notice, cx: &mut Cx) -> Nav {
        match notice {
            Notice::PhonePaired(name) => {
                cx.toast(format!("Paired with {name}"));
                Nav::Pop
            }
            _ => Nav::Unhandled,
        }
    }

    fn animate(&mut self, now: Instant) -> bool {
        // Redraw once when the code runs out.
        let expired = now.duration_since(self.started) >= crate::input::remote::link::PAIRING_VALID;
        let changed = expired != self.expired;
        self.expired = expired;
        changed
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        status_bar::draw(c, t, cx.status, Some("Settings › Phone › Pair"));
        if !self.enabled {
            message::draw(
                c,
                t,
                "Phone link is off",
                Some("Turn it on in Settings › Phone."),
            );
            hint::draw(c, t, "Esc back");
            return;
        }

        let (left, top, width, _) = t.content();
        let mid = left + width / 2.0;
        let code_style = TextStyle::new(FontId::Mono, 80.0);
        let shown = format!("{} {}", &self.code[..3], &self.code[3..]);
        let code_colour = if self.expired {
            color::TEXT_DISABLED
        } else {
            color::TEXT
        };
        let mut y = top + space::L;
        c.text(code_style, &shown, mid, y, Align::Center, rgb(code_colour));
        y += fonts().line_metrics(code_style).height + space::M;

        let line = if self.expired {
            "Code expired. Press Enter for a new one."
        } else {
            "Enter this code in the ORC-4 phone app."
        };
        c.text(
            typography::BODY,
            line,
            mid,
            y,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
        y += fonts().line_metrics(typography::BODY).height + space::S;
        c.text(
            typography::NUMERIC,
            &format!("Glasses address  {}", self.address),
            mid,
            y,
            Align::Center,
            rgb(color::TEXT_SECONDARY),
        );
        hint::draw(c, t, "Enter new code · Esc cancel");
    }
}

/// List of paired phones; Enter offers "Forget".
pub struct PairedPhones {
    phones: Vec<PairedPhone>,
    list: ListView,
}

impl PairedPhones {
    fn new(cx: &Cx) -> Self {
        let phones = cx.settings.phone.paired.clone();
        Self {
            list: ListView::new(phones.len()),
            phones,
        }
    }
}

impl Screen for PairedPhones {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        let Some(phone) = self.phones.get(self.list.focus()) else {
            return Nav::Unhandled;
        };
        if action != Action::Select {
            return Nav::Unhandled;
        }
        let token = phone.token.clone();
        Nav::Push(Box::new(Confirm::new(
            format!("Forget “{}”?", phone.name),
            "Forget",
            move |cx| {
                cx.settings.phone.paired.retain(|p| p.token != token);
                cx.link.enforce(cx.settings);
                cx.toast("Phone forgotten");
            },
        )))
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.phones = cx.settings.phone.paired.clone();
        self.list.set_len(self.phones.len());
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        if self.phones.is_empty() {
            message::draw(c, cx.theme, "No paired phones", None);
            hint::draw(c, cx.theme, "Esc back");
        } else {
            let rows: Vec<Row> = self
                .phones
                .iter()
                .map(|p| Row {
                    icon: None,
                    label: &p.name,
                    detail: String::new(),
                })
                .collect();
            self.list.draw(c, cx.theme, &rows);
            hint::draw(c, cx.theme, "Enter forget · Esc back");
        }
        status_bar::draw(c, cx.theme, cx.status, Some("Settings › Phone › Paired"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::nav::testing::Harness;

    #[test]
    fn opening_pairing_shows_a_code_and_closing_ends_it() {
        let mut h = Harness::new();
        let mut cx = h.cx(Instant::now());
        let mut p = Pairing::new();
        p.on_show(&mut cx);
        assert_eq!(p.code.len(), 6);
        assert!(cx.link.pairing_active(cx.now));
        p.handle(Action::Back, &mut cx);
        assert!(!cx.link.pairing_active(cx.now));
    }

    #[test]
    fn pairing_screen_closes_when_a_phone_pairs() {
        let mut h = Harness::new();
        let mut p = Pairing::new();
        let nav = p.on_notice(
            &Notice::PhonePaired("Pixel".into()),
            &mut h.cx(Instant::now()),
        );
        assert!(matches!(nav, Nav::Pop));
    }

    #[test]
    fn forgetting_removes_the_phone() {
        let mut h = Harness::new();
        h.settings.phone.paired.push(PairedPhone {
            name: "Pixel".into(),
            token: "t".into(),
        });
        let mut cx = h.cx(Instant::now());
        let mut list = PairedPhones::new(&cx);
        let Nav::Push(mut confirm) = list.handle(Action::Select, &mut cx) else {
            panic!("expected confirmation")
        };
        confirm.handle(Action::Right, &mut cx);
        confirm.handle(Action::Select, &mut cx);
        assert!(cx.settings.phone.paired.is_empty());
    }

    #[test]
    fn address_uses_the_listen_setting() {
        let mut s = Settings::default();
        s.phone.listen = "127.0.0.1:4004".into();
        assert_eq!(link_address(&s), "127.0.0.1:4004");
    }
}
