//! WebSocket transport. One thread accepts connections; each connection gets
//! a thread that turns incoming text frames into `RemoteEvent`s and writes
//! whatever the UI thread queues for it. No decisions are made here — pairing
//! and authorisation live in `PhoneLink` on the UI thread.

use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use tungstenite::Message;
use tungstenite::protocol::WebSocketConfig;

use super::protocol::{self, ClientMsg};
use crate::event::Event;

pub type ConnId = u64;

/// Largest message accepted from a phone.
const MAX_MESSAGE: usize = 64 * 1024;
/// A client must finish the WebSocket handshake within this time.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a connection thread checks for outgoing messages while idle.
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub enum RemoteEvent {
    /// `out` delivers messages to this phone; dropping it closes the
    /// connection (after anything already queued is sent).
    Connected {
        id: ConnId,
        out: Sender<String>,
    },
    Message {
        id: ConnId,
        msg: Result<ClientMsg, String>,
    },
    Closed {
        id: ConnId,
    },
}

/// Starts listening on `addr` (e.g. "0.0.0.0:4004"). Returns the bound
/// address (useful with port 0 in tests).
pub fn start(addr: &str, events: Sender<Event>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    thread::Builder::new()
        .name("remote-listen".into())
        .spawn(move || {
            let mut next_id: ConnId = 1;
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let (id, events) = (next_id, events.clone());
                next_id += 1;
                let spawned = thread::Builder::new()
                    .name(format!("remote-{id}"))
                    .spawn(move || serve(id, stream, events));
                if let Err(e) = spawned {
                    log::error!("remote: cannot spawn connection thread: {e}");
                }
            }
        })?;
    Ok(local)
}

fn serve(id: ConnId, stream: TcpStream, events: Sender<Event>) {
    let peer = stream.peer_addr().ok();
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    let mut ws = match tungstenite::accept_with_config(stream, Some(config)) {
        Ok(ws) => ws,
        Err(e) => {
            log::info!("remote {id}: handshake failed: {e}");
            return;
        }
    };
    let _ = ws.get_ref().set_read_timeout(Some(POLL));

    let (out, outbox) = mpsc::channel::<String>();
    if events
        .send(Event::Remote(RemoteEvent::Connected { id, out }))
        .is_err()
    {
        return;
    }
    log::info!("remote {id}: connected from {peer:?}");

    'conn: loop {
        loop {
            match outbox.try_recv() {
                Ok(text) => {
                    if ws.send(Message::text(text)).is_err() {
                        break 'conn;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The UI dropped this connection.
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    break 'conn;
                }
            }
        }
        match ws.read() {
            Ok(Message::Text(text)) => {
                let msg = protocol::parse(&text);
                if events
                    .send(Event::Remote(RemoteEvent::Message { id, msg }))
                    .is_err()
                {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {} // pings are answered by tungstenite; binary is ignored
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                log::info!("remote {id}: {e}");
                break;
            }
        }
    }
    log::info!("remote {id}: closed");
    let _ = events.send(Event::Remote(RemoteEvent::Closed { id }));
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn next_remote(rx: &mpsc::Receiver<Event>) -> RemoteEvent {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Remote(e)) => e,
            other => panic!("expected a remote event, got {other:?}"),
        }
    }

    /// Real sockets: a client connects, both directions work, closing is seen.
    #[test]
    fn messages_flow_both_ways_over_a_real_socket() {
        let (tx, rx) = mpsc::channel();
        let addr = start("127.0.0.1:0", tx).unwrap();

        let stream = TcpStream::connect(addr).unwrap();
        let (mut client, _) = tungstenite::client(format!("ws://{addr}/"), stream).unwrap();

        let RemoteEvent::Connected { id, out } = next_remote(&rx) else {
            panic!("expected Connected")
        };

        client
            .send(Message::text(r#"{"t":"action","a":"left"}"#))
            .unwrap();
        match next_remote(&rx) {
            RemoteEvent::Message { id: got, msg } => {
                assert_eq!(got, id);
                assert_eq!(msg.unwrap(), ClientMsg::Action { a: "left".into() });
            }
            other => panic!("unexpected {other:?}"),
        }

        out.send(r#"{"t":"welcome"}"#.into()).unwrap();
        let reply = client.read().unwrap();
        assert_eq!(reply.to_text().unwrap(), r#"{"t":"welcome"}"#);

        drop(out); // UI drops the connection
        assert!(matches!(next_remote(&rx), RemoteEvent::Closed { .. }));
    }
}
