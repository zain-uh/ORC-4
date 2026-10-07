//! Phone link logic, on the UI thread: who is connected, pairing, tokens,
//! and turning phone messages into the same Actions a keyboard produces.
//!
//! Security model (v1, Wi-Fi):
//! - A phone must pair once using a 6-digit code shown on the glasses while
//!   the pairing screen is open (valid 5 minutes).
//! - 5 wrong codes lock pairing for 30 s, so the code can't be brute-forced.
//! - Pairing issues a random 128-bit token; the phone presents it on every
//!   later connection. Forgetting a phone deletes its token.
//! - Nothing from an unauthenticated connection reaches the UI.
//! - Not encrypted: fine on a private WPA2 network. BLE (later) brings its
//!   own encryption.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::protocol::{ClientMsg, ErrorCode, ServerMsg, TextOp, VERSION};
use super::server::{ConnId, RemoteEvent};
use crate::app::action::{Action, Notice, TextEdit, TextRequest};
use crate::config::{PairedPhone, Settings};

pub const PAIRING_VALID: Duration = Duration::from_secs(5 * 60);
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(30);
const MAX_CONNECTIONS: usize = 4;
const MAX_TEXT: usize = 256;

/// What the runtime should do as a result of phone traffic.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkOutput {
    Input(Action),
    Text(TextEdit),
    Notice(Notice),
    Time { unix: i64, utc_offset_minutes: i32 },
}

struct Conn {
    out: Sender<String>,
    /// Set once the phone has authenticated.
    phone: Option<PairedPhone>,
}

pub struct PhoneLink {
    conns: HashMap<ConnId, Conn>,
    pairing: Option<(String, Instant)>,
    failures: u32,
    locked_until: Option<Instant>,
    text_request: Option<TextRequest>,
}

impl PhoneLink {
    pub fn new() -> Self {
        Self {
            conns: HashMap::new(),
            pairing: None,
            failures: 0,
            locked_until: None,
            text_request: None,
        }
    }

    pub fn handle(
        &mut self,
        event: RemoteEvent,
        settings: &mut Settings,
        now: Instant,
    ) -> Vec<LinkOutput> {
        let mut out = Vec::new();
        match event {
            RemoteEvent::Connected { id, out: tx } => {
                let conn = Conn {
                    out: tx,
                    phone: None,
                };
                if !settings.phone.enabled {
                    send(
                        &conn,
                        &ServerMsg::error(ErrorCode::Disabled, "Phone link is off"),
                    );
                } else if self.conns.len() >= MAX_CONNECTIONS {
                    send(
                        &conn,
                        &ServerMsg::error(ErrorCode::Busy, "Too many connections"),
                    );
                } else {
                    self.conns.insert(id, conn);
                }
                // A rejected `conn` is dropped here, which closes it.
            }
            RemoteEvent::Closed { id } => {
                if let Some(Conn {
                    phone: Some(phone), ..
                }) = self.conns.remove(&id)
                {
                    out.push(LinkOutput::Notice(Notice::PhoneDisconnected(phone.name)));
                }
            }
            RemoteEvent::Message { id, msg } => match msg {
                Ok(msg) => self.on_message(id, msg, settings, now, &mut out),
                Err(e) => {
                    log::info!("remote {id}: bad message: {e}");
                    self.send_to(
                        id,
                        &ServerMsg::error(ErrorCode::BadMessage, "Unreadable message"),
                    );
                }
            },
        }
        out
    }

    fn on_message(
        &mut self,
        id: ConnId,
        msg: ClientMsg,
        settings: &mut Settings,
        now: Instant,
        out: &mut Vec<LinkOutput>,
    ) {
        let Some(conn) = self.conns.get(&id) else {
            return;
        };
        let authed = conn.phone.is_some();
        match msg {
            ClientMsg::Hello { v, .. } | ClientMsg::Pair { v, .. } if v != VERSION => {
                self.reject(id, ErrorCode::Version, "Please update the ORC-4 app");
            }
            ClientMsg::Hello { token, .. } if !authed => {
                match settings
                    .phone
                    .paired
                    .iter()
                    .find(|p| same(&p.token, &token))
                {
                    Some(phone) => {
                        let phone = phone.clone();
                        self.authenticate(id, phone.clone(), settings);
                        out.push(LinkOutput::Notice(Notice::PhoneConnected(phone.name)));
                    }
                    None => self.reject(id, ErrorCode::NotPaired, "Pair this phone again"),
                }
            }
            ClientMsg::Pair { device, code, .. } if !authed => {
                if self.locked_until.is_some_and(|t| now < t) {
                    self.send_to(
                        id,
                        &ServerMsg::error(ErrorCode::Locked, "Try again in 30 seconds"),
                    );
                    return;
                }
                let valid = self.pairing.as_ref().is_some_and(|(c, start)| {
                    now.duration_since(*start) < PAIRING_VALID && same(c, &code)
                });
                if !valid {
                    self.failures += 1;
                    if self.failures >= MAX_FAILURES {
                        self.failures = 0;
                        self.locked_until = Some(now + LOCKOUT);
                        log::warn!("remote: pairing locked after repeated wrong codes");
                    }
                    self.send_to(id, &ServerMsg::error(ErrorCode::BadCode, "Wrong code"));
                    return;
                }
                let phone = PairedPhone {
                    name: clean_name(&device),
                    token: random_hex(16),
                };
                log::info!("remote: paired {}", phone.name);
                settings.phone.paired.push(phone.clone());
                self.pairing = None;
                self.failures = 0;
                self.send_to(
                    id,
                    &ServerMsg::Paired {
                        token: phone.token.clone(),
                    },
                );
                self.authenticate(id, phone.clone(), settings);
                out.push(LinkOutput::Notice(Notice::PhonePaired(phone.name)));
            }
            _ if !authed => self.reject(id, ErrorCode::NotPaired, "Pair this phone first"),
            ClientMsg::Hello { .. } | ClientMsg::Pair { .. } => {} // already authenticated
            ClientMsg::Action { a } => match Action::from_name(&a) {
                Some(action) => out.push(LinkOutput::Input(action)),
                None => self.send_to(
                    id,
                    &ServerMsg::error(ErrorCode::BadMessage, "Unknown action"),
                ),
            },
            ClientMsg::Text { op, s } => out.push(LinkOutput::Text(match op {
                TextOp::Insert => TextEdit::Insert(clean_text(&s)),
                TextOp::Backspace => TextEdit::Backspace,
                TextOp::Clear => TextEdit::Clear,
                TextOp::Submit => TextEdit::Submit,
            })),
            ClientMsg::Time {
                unix,
                utc_offset_minutes,
            } => {
                // 2025-01-01 .. 2100-01-01, and real-world UTC offsets only.
                if (1_735_689_600..4_102_444_800).contains(&unix)
                    && (-14 * 60..=14 * 60).contains(&utc_offset_minutes)
                {
                    out.push(LinkOutput::Time {
                        unix,
                        utc_offset_minutes,
                    });
                }
            }
        }
    }

    fn authenticate(&mut self, id: ConnId, phone: PairedPhone, settings: &Settings) {
        if let Some(conn) = self.conns.get_mut(&id) {
            conn.phone = Some(phone);
            send(
                conn,
                &ServerMsg::Welcome {
                    v: VERSION,
                    name: settings.phone.name.clone(),
                },
            );
            send(conn, &text_request_msg(&self.text_request));
        }
    }

    /// Sends an error and closes the connection.
    fn reject(&mut self, id: ConnId, code: ErrorCode, message: &str) {
        self.send_to(id, &ServerMsg::error(code, message));
        self.conns.remove(&id);
    }

    fn send_to(&self, id: ConnId, msg: &ServerMsg) {
        if let Some(conn) = self.conns.get(&id) {
            send(conn, msg);
        }
    }

    /// Shows a new pairing code (valid 5 minutes) and returns it.
    pub fn start_pairing(&mut self, now: Instant) -> String {
        let code = format!("{:06}", random_u32() % 1_000_000);
        self.pairing = Some((code.clone(), now));
        code
    }

    pub fn stop_pairing(&mut self) {
        self.pairing = None;
    }

    #[cfg(test)]
    pub fn pairing_active(&self, now: Instant) -> bool {
        self.pairing
            .as_ref()
            .is_some_and(|(_, start)| now.duration_since(*start) < PAIRING_VALID)
    }

    pub fn connected_phones(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .conns
            .values()
            .filter_map(|c| c.phone.as_ref().map(|p| p.name.clone()))
            .collect();
        names.sort();
        names
    }

    /// Tells connected phones whether a text field is focused (only when it
    /// changes).
    pub fn set_text_request(&mut self, request: Option<TextRequest>) {
        if request == self.text_request {
            return;
        }
        self.text_request = request;
        let msg = text_request_msg(&self.text_request);
        for conn in self.conns.values().filter(|c| c.phone.is_some()) {
            send(conn, &msg);
        }
    }

    /// Drops connections that settings no longer allow (link turned off, or
    /// the phone was forgotten).
    pub fn enforce(&mut self, settings: &Settings) {
        let phones = &settings.phone;
        self.conns.retain(|_, c| match &c.phone {
            _ if !phones.enabled => false,
            Some(p) => phones.paired.iter().any(|q| same(&q.token, &p.token)),
            None => true,
        });
        if !phones.enabled {
            self.pairing = None;
        }
    }
}

fn send(conn: &Conn, msg: &ServerMsg) {
    // A send error means the connection thread is gone; Closed follows.
    let _ = conn.out.send(msg.to_json());
}

fn text_request_msg(request: &Option<TextRequest>) -> ServerMsg {
    ServerMsg::TextRequest {
        prompt: request.as_ref().map(|r| r.prompt.clone()),
        value: request.as_ref().map(|r| r.value.clone()),
    }
}

/// Comparison whose time doesn't depend on where the strings differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(32).collect();
    if name.trim().is_empty() {
        "Phone".into()
    } else {
        name.trim().into()
    }
}

fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect()
}

/// Secret random bytes from the OS.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("OS random number generator available");
    bytes
}

fn random_u32() -> u32 {
    u32::from_le_bytes(random_bytes::<4>())
}

fn random_hex(len: usize) -> String {
    let bytes = random_bytes::<32>();
    bytes[..len].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};

    use super::*;

    struct Phone {
        id: ConnId,
        inbox: Receiver<String>,
    }

    impl Phone {
        fn received(&self) -> Vec<String> {
            self.inbox.try_iter().collect()
        }
    }

    fn connect(link: &mut PhoneLink, settings: &mut Settings, id: ConnId) -> Phone {
        let (out, inbox) = mpsc::channel();
        link.handle(RemoteEvent::Connected { id, out }, settings, Instant::now());
        Phone { id, inbox }
    }

    fn say(
        link: &mut PhoneLink,
        s: &mut Settings,
        p: &Phone,
        json: &str,
        now: Instant,
    ) -> Vec<LinkOutput> {
        let msg = super::super::protocol::parse(json);
        link.handle(RemoteEvent::Message { id: p.id, msg }, s, now)
    }

    fn pair(link: &mut PhoneLink, s: &mut Settings, p: &Phone, code: &str) -> Vec<LinkOutput> {
        let json = format!(r#"{{"t":"pair","v":1,"device":"Pixel","code":"{code}"}}"#);
        say(link, s, p, &json, Instant::now())
    }

    #[test]
    fn pairing_with_the_shown_code_issues_a_token() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        let code = link.start_pairing(Instant::now());
        let phone = connect(&mut link, &mut s, 1);

        let out = pair(&mut link, &mut s, &phone, &code);
        assert_eq!(
            out,
            [LinkOutput::Notice(Notice::PhonePaired("Pixel".into()))]
        );
        assert_eq!(s.phone.paired.len(), 1);
        assert_eq!(s.phone.paired[0].token.len(), 32);
        let msgs = phone.received();
        assert!(msgs[0].contains(r#""t":"paired""#));
        assert!(msgs[1].contains(r#""t":"welcome""#));
        assert!(!link.pairing_active(Instant::now()), "code is single use");
    }

    #[test]
    fn a_paired_phone_reconnects_with_its_token_and_can_drive_the_ui() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        s.phone.paired.push(PairedPhone {
            name: "Pixel".into(),
            token: "abc123".into(),
        });
        let phone = connect(&mut link, &mut s, 1);
        let now = Instant::now();
        let out = say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"hello","v":1,"device":"x","token":"abc123"}"#,
            now,
        );
        assert_eq!(
            out,
            [LinkOutput::Notice(Notice::PhoneConnected("Pixel".into()))]
        );

        let out = say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"action","a":"select"}"#,
            now,
        );
        assert_eq!(out, [LinkOutput::Input(Action::Select)]);
        let out = say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"text","op":"insert","s":"ca\u0007ts"}"#,
            now,
        );
        assert_eq!(
            out,
            [LinkOutput::Text(TextEdit::Insert("cats".into()))],
            "control chars stripped"
        );
    }

    #[test]
    fn unpaired_phones_cannot_send_input_and_are_disconnected() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        let phone = connect(&mut link, &mut s, 1);
        let out = say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"action","a":"select"}"#,
            Instant::now(),
        );
        assert!(out.is_empty());
        assert!(phone.received()[0].contains("not_paired"));
        assert!(link.conns.is_empty());
    }

    #[test]
    fn wrong_codes_lock_pairing() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        let code = link.start_pairing(Instant::now());
        let wrong = if code == "000000" { "111111" } else { "000000" };
        let phone = connect(&mut link, &mut s, 1);
        for _ in 0..MAX_FAILURES {
            pair(&mut link, &mut s, &phone, wrong);
        }
        // Even the right code is refused while locked.
        let out = pair(&mut link, &mut s, &phone, &code);
        assert!(out.is_empty());
        assert!(phone.received().last().unwrap().contains("locked"));
        assert!(s.phone.paired.is_empty());
    }

    #[test]
    fn no_pairing_without_the_screen_open() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        let phone = connect(&mut link, &mut s, 1);
        assert!(pair(&mut link, &mut s, &phone, "123456").is_empty());
        assert!(s.phone.paired.is_empty());
    }

    #[test]
    fn disabled_link_refuses_connections_and_forgetting_disconnects() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        s.phone.paired.push(PairedPhone {
            name: "Pixel".into(),
            token: "t".into(),
        });
        let phone = connect(&mut link, &mut s, 1);
        say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"hello","v":1,"device":"x","token":"t"}"#,
            Instant::now(),
        );
        assert_eq!(link.connected_phones(), ["Pixel"]);

        s.phone.paired.clear(); // forgotten in Settings
        link.enforce(&s);
        assert!(link.connected_phones().is_empty());

        s.phone.enabled = false;
        let other = connect(&mut link, &mut s, 2);
        assert!(other.received()[0].contains("disabled"));
        assert!(link.conns.is_empty());
    }

    #[test]
    fn text_requests_are_sent_only_when_they_change() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        s.phone.paired.push(PairedPhone {
            name: "P".into(),
            token: "t".into(),
        });
        let phone = connect(&mut link, &mut s, 1);
        say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"hello","v":1,"device":"x","token":"t"}"#,
            Instant::now(),
        );
        phone.received();

        let req = TextRequest {
            prompt: "Name".into(),
            value: "OR".into(),
        };
        link.set_text_request(Some(req.clone()));
        link.set_text_request(Some(req));
        link.set_text_request(None);
        let msgs = phone.received();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].contains(r#""prompt":"Name""#));
        assert_eq!(msgs[1], r#"{"t":"text_request"}"#);
    }

    #[test]
    fn version_mismatch_and_bad_time_are_rejected() {
        let (mut link, mut s) = (PhoneLink::new(), Settings::default());
        let phone = connect(&mut link, &mut s, 1);
        say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"hello","v":2,"device":"x","token":"t"}"#,
            Instant::now(),
        );
        assert!(phone.received()[0].contains("version"));

        s.phone.paired.push(PairedPhone {
            name: "P".into(),
            token: "t".into(),
        });
        let phone = connect(&mut link, &mut s, 2);
        let now = Instant::now();
        say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"hello","v":1,"device":"x","token":"t"}"#,
            now,
        );
        let out = say(
            &mut link,
            &mut s,
            &phone,
            r#"{"t":"time","unix":5,"utc_offset_minutes":0}"#,
            now,
        );
        assert!(out.is_empty(), "1970 is not a real time");
    }

    #[test]
    fn constant_time_compare_works() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
    }
}
