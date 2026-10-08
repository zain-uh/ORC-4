//! Settings → Network: internet status, Wi-Fi networks, connect and forget.
//!
//! The live state comes from the status model (kept fresh by the runtime's
//! network check). Wi-Fi operations run on the network pool through the
//! `Wifi` backend; on the dev PC that backend is simulated and says so.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use crate::app::action::Action;
use crate::screens::confirm::Confirm;
use crate::screens::nav::{Cx, Nav, Screen};
use crate::screens::text_entry::TextEntry;
use crate::services::network::{self, WifiNetwork};
use crate::services::{Reply, ServiceEvent, Ticket, UserError, system};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::status_bar::Connection;
use crate::ui::widgets::{hint, message, status_bar};

const ROWS: usize = 4;

pub struct NetworkPage {
    list: ListView,
    address: String,
}

impl NetworkPage {
    pub fn new() -> Self {
        Self {
            list: ListView::new(ROWS),
            address: String::new(),
        }
    }
}

impl Screen for NetworkPage {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        match action {
            Action::Select if self.list.focus() == 0 => Nav::Push(Box::new(Networks::new())),
            Action::Select => Nav::Stay,
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        cx.check_network();
        self.address = system::local_ip().map_or("—".into(), |ip| ip.to_string());
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        c.clear(rgb(color::BACKGROUND));
        let s = cx.status;
        let internet = match s.connection {
            Connection::Unknown => "Checking…",
            Connection::Online => "Connected",
            Connection::SignIn => "Sign-in page in the way",
            Connection::Offline => "Not connected",
        };
        let rows = [
            Row {
                icon: None,
                label: "Wi-Fi",
                detail: format!("{}  ›", s.wifi_name.as_deref().unwrap_or("Not connected")),
            },
            Row {
                icon: None,
                label: "Internet",
                detail: internet.into(),
            },
            Row {
                icon: None,
                label: "Address",
                detail: self.address.clone(),
            },
            Row {
                icon: None,
                label: "Wi-Fi control",
                detail: network::wifi().label().into(),
            },
        ];
        self.list.draw(c, cx.theme, &rows);
        status_bar::draw(c, cx.theme, s, Some("Settings › Network"));
        hint::draw(c, cx.theme, "Enter choose · Esc back");
    }
}

enum Scan {
    Scanning(Ticket),
    Ready,
    Failed(UserError),
}

/// Nearby networks. Enter connects (asking for a password when needed);
/// Tab forgets a saved network.
pub struct Networks {
    networks: Vec<WifiNetwork>,
    list: ListView,
    scan: Option<Scan>,
    /// Connect or forget in flight, set from callbacks of pushed screens.
    pending: Rc<Cell<Option<(Ticket, Op)>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Connect,
    Forget,
}

impl Networks {
    fn new() -> Self {
        Self {
            networks: Vec::new(),
            list: ListView::new(0),
            scan: None,
            pending: Rc::new(Cell::new(None)),
        }
    }

    fn rescan(&mut self, cx: &mut Cx) {
        let ticket = cx
            .services
            .run_net(|| network::wifi().scan().map(Reply::Networks));
        self.scan = Some(Scan::Scanning(ticket));
    }

    fn connect(ssid: String, password: Option<String>, cx: &mut Cx) -> Ticket {
        cx.toast(format!("Joining {ssid}…"));
        cx.services.run_net(move || {
            network::wifi()
                .connect(&ssid, password.as_deref())
                .map(|()| Reply::Done)
        })
    }
}

impl Screen for Networks {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        let Some(net) = self.networks.get(self.list.focus()).cloned() else {
            return Nav::Unhandled;
        };
        match action {
            Action::Select if net.connected => {
                cx.toast(format!("Already connected to {}", net.ssid));
                Nav::Stay
            }
            Action::Select if net.secured && !net.known => {
                let pending = Rc::clone(&self.pending);
                let ssid = net.ssid.clone();
                Nav::Push(Box::new(
                    TextEntry::new(
                        "Settings › Network",
                        format!("Password for {ssid}"),
                        "",
                        63,
                        move |cx, password| {
                            if !password.is_empty() {
                                let t = Networks::connect(ssid, Some(password), cx);
                                pending.set(Some((t, Op::Connect)));
                            }
                            None
                        },
                    )
                    .keep_spaces(),
                ))
            }
            Action::Select => {
                let t = Networks::connect(net.ssid, None, cx);
                self.pending.set(Some((t, Op::Connect)));
                Nav::Stay
            }
            Action::Menu if net.known => {
                let pending = Rc::clone(&self.pending);
                let ssid = net.ssid.clone();
                Nav::Push(Box::new(Confirm::new(
                    format!("Forget “{}”?", net.ssid),
                    "Forget",
                    move |cx| {
                        let t = cx
                            .services
                            .run_net(move || network::wifi().forget(&ssid).map(|()| Reply::Done));
                        pending.set(Some((t, Op::Forget)));
                    },
                )))
            }
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.rescan(cx);
    }

    fn on_service(&mut self, event: ServiceEvent, cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if matches!(self.scan, Some(Scan::Scanning(t)) if t == event.ticket) {
            match event.result {
                Ok(Reply::Networks(nets)) => {
                    self.list.set_len(nets.len());
                    self.networks = nets;
                    self.scan = Some(Scan::Ready);
                }
                Ok(_) => self.scan = Some(Scan::Failed(UserError::Internal)),
                Err(e) => self.scan = Some(Scan::Failed(e)),
            }
            return Ok(Nav::Stay);
        }
        match self.pending.get() {
            Some((ticket, op)) if ticket == event.ticket => {
                self.pending.set(None);
                match (event.result, op) {
                    (Ok(_), Op::Connect) => cx.toast("Connected"),
                    (Ok(_), Op::Forget) => cx.toast("Network forgotten"),
                    (Err(e), _) => cx.toast(e.message()),
                }
                cx.check_network();
                self.rescan(cx);
                Ok(Nav::Stay)
            }
            _ => Err(event),
        }
    }

    fn animate(&mut self, now: Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        if self.networks.is_empty() {
            match &self.scan {
                Some(Scan::Failed(e)) => {
                    message::draw(c, t, "Can't list networks", Some(e.message()))
                }
                Some(Scan::Ready) => message::draw(c, t, "No networks found", None),
                _ => message::draw(c, t, "Looking for networks…", None),
            }
            hint::draw(c, t, "Esc back");
        } else {
            let rows: Vec<Row> = self
                .networks
                .iter()
                .map(|n| Row {
                    icon: None,
                    label: &n.ssid,
                    detail: if n.connected {
                        "Connected".into()
                    } else if n.known {
                        "Saved".into()
                    } else {
                        signal_word(n.signal).into()
                    },
                })
                .collect();
            self.list.draw(c, t, &rows);
            hint::draw(c, t, "Enter join · Tab forget · Esc back");
        }
        status_bar::draw(c, t, cx.status, Some("Settings › Network › Wi-Fi"));
    }
}

/// Words read better than bars on composite video.
fn signal_word(bars: u8) -> &'static str {
    match bars {
        4.. => "Excellent",
        3 => "Good",
        2 => "Fair",
        _ => "Weak",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::nav::testing::Harness;

    fn net(ssid: &str, secured: bool, known: bool, connected: bool) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.into(),
            signal: 3,
            secured,
            known,
            connected,
        }
    }

    fn networks_with(list: Vec<WifiNetwork>) -> Networks {
        let mut n = Networks::new();
        n.list.set_len(list.len());
        n.networks = list;
        n.scan = Some(Scan::Ready);
        n
    }

    #[test]
    fn secured_unknown_network_asks_for_a_password() {
        let mut h = Harness::new();
        let mut n = networks_with(vec![net("Locked", true, false, false)]);
        assert!(matches!(
            n.handle(Action::Select, &mut h.cx(Instant::now())),
            Nav::Push(_)
        ));
    }

    #[test]
    fn open_network_connects_directly_and_only_saved_ones_can_be_forgotten() {
        let mut h = Harness::new();
        let mut n = networks_with(vec![net("Open", false, false, false)]);
        let mut cx = h.cx(Instant::now());
        assert!(matches!(n.handle(Action::Select, &mut cx), Nav::Stay));
        assert!(n.pending.get().is_some(), "connect started");
        assert!(matches!(n.handle(Action::Menu, &mut cx), Nav::Unhandled));
    }

    #[test]
    fn signal_words() {
        assert_eq!(signal_word(4), "Excellent");
        assert_eq!(signal_word(0), "Weak");
    }
}

#[cfg(test)]
mod snapshot {
    use super::*;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn snapshot_networks() {
        let mut n = Networks::new();
        n.networks = vec![
            WifiNetwork {
                ssid: "Home".into(),
                signal: 4,
                secured: true,
                known: true,
                connected: true,
            },
            WifiNetwork {
                ssid: "ORC Test".into(),
                signal: 3,
                secured: true,
                known: false,
                connected: false,
            },
            WifiNetwork {
                ssid: "Cafe".into(),
                signal: 1,
                secured: false,
                known: false,
                connected: false,
            },
        ];
        n.list.set_len(3);
        n.scan = Some(Scan::Ready);
        let mut c = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
            connection: Connection::Online,
            wifi_bars: Some(3),
            wifi_name: Some("Home".into()),
        };
        n.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("settings-networks", &c);
    }
}
