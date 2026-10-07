//! Media playback through the `mpv` program.
//!
//! mpv runs as a separate process (a crash or a bad file can't take ORC-4
//! down) and is driven over its JSON IPC channel: a Unix socket on the
//! device, a named pipe on the Windows dev PC. YouTube URLs are resolved by
//! mpv's built-in yt-dlp hook using the configured format.
//!
//! One thread per playback: it starts mpv, applies commands from the UI and
//! polls a few properties 4× a second, sending `PlayerEvent`s when they
//! change. Dropping the `Player` handle stops playback.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::UserError;
use crate::event::Event;

const POLL: Duration = Duration::from_millis(250);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const QUIT_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerCmd {
    TogglePause,
    /// Relative seek in seconds.
    Seek(f64),
    /// Relative volume change.
    AddVolume(f64),
    /// Show (true) or drop (false) the video track: audio-only mode.
    Video(bool),
    Stop,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerStatus {
    /// `None` until playback has started.
    pub position: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
    /// Waiting for the network.
    pub buffering: bool,
    pub volume: Option<f64>,
}

#[derive(Debug)]
pub enum PlayerEvent {
    Status(PlayerStatus),
    /// mpv exited: `Ok` when playback finished or was stopped.
    Ended(Result<(), UserError>),
}

/// What to play and how.
#[derive(Debug, Clone)]
pub struct PlayOptions {
    pub mpv: String,
    pub ytdlp: String,
    pub format: String,
    pub extra_args: Vec<String>,
    pub volume: u32,
}

/// Handle to one playback. Dropping it stops playback.
pub struct Player {
    cmds: Sender<PlayerCmd>,
}

impl Player {
    pub fn send(&self, cmd: PlayerCmd) {
        let _ = self.cmds.send(cmd);
    }

    /// A player not connected to mpv, for tests: commands go to the receiver.
    #[cfg(test)]
    pub fn detached() -> (Self, Receiver<PlayerCmd>) {
        let (cmds, rx) = mpsc::channel();
        (Self { cmds }, rx)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.cmds.send(PlayerCmd::Stop);
    }
}

pub fn start(target: String, opts: PlayOptions, events: Sender<Event>) -> Player {
    let (cmds, rx) = mpsc::channel();
    let spawned = thread::Builder::new().name("player".into()).spawn(move || {
        let result = run(&target, &opts, &rx, &events);
        let _ = events.send(Event::Player(PlayerEvent::Ended(result)));
    });
    if let Err(e) = spawned {
        log::error!("player: cannot spawn thread: {e}");
    }
    Player { cmds }
}

fn run(
    target: &str,
    opts: &PlayOptions,
    cmds: &Receiver<PlayerCmd>,
    events: &Sender<Event>,
) -> Result<(), UserError> {
    let ipc_path = ipc_path();
    let mut child = Command::new(&opts.mpv)
        .args([
            "--no-terminal",
            "--idle=no",
            "--keep-open=no",
            "--cache=yes",
            // Generous buffering for Wi-Fi sharing the radio with Bluetooth,
            // capped for the Pi's 512 MB.
            "--demuxer-max-bytes=32MiB",
            "--demuxer-readahead-secs=30",
        ])
        .arg(format!("--input-ipc-server={ipc_path}"))
        .arg(format!("--volume={}", opts.volume.min(100)))
        .arg(format!("--ytdl-format={}", opts.format))
        .arg(format!("--script-opts=ytdl_hook-ytdl_path={}", opts.ytdlp))
        .args(&opts.extra_args)
        .arg("--")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            log::warn!("player: cannot start mpv: {e}");
            if e.kind() == io::ErrorKind::NotFound {
                UserError::MissingTool
            } else {
                UserError::Internal
            }
        })?;
    log::info!("player: mpv started for {target}");

    let mut ipc = match connect(&ipc_path, &mut child) {
        Ok(ipc) => ipc,
        Err(e) => {
            log::warn!("player: no IPC connection: {e}");
            return finish(&mut child, None);
        }
    };

    let mut last = PlayerStatus::default();
    loop {
        match cmds.recv_timeout(POLL) {
            Ok(PlayerCmd::Stop) | Err(RecvTimeoutError::Disconnected) => {
                let _ = ipc.request(json!(["quit"]));
                return finish(&mut child, Some(Instant::now() + QUIT_GRACE));
            }
            Ok(cmd) => {
                if let Err(e) = ipc.request(command_json(cmd)) {
                    log::info!("player: command failed: {e}");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if let Ok(Some(_)) = child.try_wait() {
            return finish(&mut child, None);
        }
        match poll(&mut ipc) {
            Ok(status) if status != last => {
                log::trace!("player: {status:?}");
                last = status.clone();
                if events
                    .send(Event::Player(PlayerEvent::Status(status)))
                    .is_err()
                {
                    let _ = ipc.request(json!(["quit"]));
                    return finish(&mut child, Some(Instant::now() + QUIT_GRACE));
                }
            }
            Ok(_) => {}
            // mpv went away mid-request; the exit status says why.
            Err(_) => return finish(&mut child, Some(Instant::now() + QUIT_GRACE)),
        }
    }
}

fn command_json(cmd: PlayerCmd) -> Value {
    match cmd {
        PlayerCmd::TogglePause => json!(["cycle", "pause"]),
        PlayerCmd::Seek(secs) => json!(["seek", secs, "relative"]),
        PlayerCmd::AddVolume(delta) => json!(["add", "volume", delta]),
        PlayerCmd::Video(true) => json!(["set_property", "vid", "auto"]),
        PlayerCmd::Video(false) => json!(["set_property", "vid", "no"]),
        PlayerCmd::Stop => json!(["quit"]),
    }
}

fn poll(ipc: &mut Ipc) -> io::Result<PlayerStatus> {
    Ok(PlayerStatus {
        position: ipc.request(json!(["get_property", "time-pos"]))?.as_f64(),
        duration: ipc.request(json!(["get_property", "duration"]))?.as_f64(),
        paused: ipc.request(json!(["get_property", "pause"]))?.as_bool() == Some(true),
        buffering: ipc
            .request(json!(["get_property", "paused-for-cache"]))?
            .as_bool()
            == Some(true),
        volume: ipc.request(json!(["get_property", "volume"]))?.as_f64(),
    })
}

/// Waits for mpv to exit (killing it after `deadline`) and maps its exit
/// code: 0 = finished or quit, anything else = couldn't play.
fn finish(child: &mut Child, deadline: Option<Instant>) -> Result<(), UserError> {
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if deadline.is_none_or(|d| Instant::now() < d) => {
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
        }
    };
    log::info!("player: mpv exited ({status})");
    if status.success() {
        Ok(())
    } else {
        Err(UserError::Playback)
    }
}

/// Unique per playback, so a slowly exiting mpv can't clash with the next.
fn ipc_path() -> String {
    static SESSION: AtomicU32 = AtomicU32::new(0);
    let n = SESSION.fetch_add(1, Ordering::Relaxed);
    let name = format!("orc4-mpv-{}-{n}", std::process::id());
    if cfg!(windows) {
        format!(r"\\.\pipe\{name}")
    } else {
        std::env::temp_dir()
            .join(format!("{name}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(unix)]
type Pipe = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Pipe = std::fs::File;

#[cfg(unix)]
fn open_pipe(path: &str) -> io::Result<(Pipe, Pipe)> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok((stream.try_clone()?, stream))
}

#[cfg(windows)]
fn open_pipe(path: &str) -> io::Result<(Pipe, Pipe)> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    Ok((file.try_clone()?, file))
}

/// mpv creates its IPC endpoint shortly after starting; retry until then.
fn connect(path: &str, child: &mut Child) -> io::Result<Ipc> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match open_pipe(path) {
            Ok((reader, writer)) => {
                return Ok(Ipc {
                    reader: BufReader::new(reader),
                    writer,
                    next_id: 1,
                });
            }
            Err(e) => {
                if Instant::now() >= deadline || matches!(child.try_wait(), Ok(Some(_))) {
                    return Err(e);
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

struct Ipc {
    reader: BufReader<Pipe>,
    writer: Pipe,
    next_id: u64,
}

impl Ipc {
    /// Sends a command and returns its `data` (`Null` when mpv reports an
    /// error such as "property unavailable"). Event lines are skipped.
    fn request(&mut self, command: Value) -> io::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut line = json!({ "command": command, "request_id": id }).to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        loop {
            let mut reply = String::new();
            if self.reader.read_line(&mut reply)? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if let Some(data) = reply_data(&reply, id) {
                return Ok(data);
            }
        }
    }
}

/// `Some(data)` if `line` is the reply to request `id`.
fn reply_data(line: &str, id: u64) -> Option<Value> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("request_id")?.as_u64()? != id {
        return None;
    }
    Some(
        if v.get("error").and_then(Value::as_str) == Some("success") {
            v.get("data").cloned().unwrap_or(Value::Null)
        } else {
            Value::Null
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_matched_by_id_and_events_skipped() {
        let reply = r#"{"data":12.5,"error":"success","request_id":7}"#;
        assert_eq!(reply_data(reply, 7), Some(json!(12.5)));
        assert_eq!(reply_data(reply, 8), None, "someone else's reply");
        assert_eq!(reply_data(r#"{"event":"playback-restart"}"#, 7), None);
        assert_eq!(
            reply_data(r#"{"error":"property unavailable","request_id":3}"#, 3),
            Some(Value::Null)
        );
    }

    #[test]
    fn commands_map_to_mpv_ipc() {
        assert_eq!(
            command_json(PlayerCmd::Seek(-10.0)),
            json!(["seek", -10.0, "relative"])
        );
        assert_eq!(
            command_json(PlayerCmd::Video(false)),
            json!(["set_property", "vid", "no"])
        );
    }

    #[test]
    fn missing_mpv_is_reported() {
        let (tx, rx) = mpsc::channel();
        let opts = PlayOptions {
            mpv: "definitely-not-mpv-xyz".into(),
            ytdlp: "yt-dlp".into(),
            format: "b".into(),
            extra_args: vec![],
            volume: 50,
        };
        let _player = start("x.mp4".into(), opts, tx);
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::Player(PlayerEvent::Ended(Err(UserError::MissingTool)))) => {}
            other => panic!("expected MissingTool, got {other:?}"),
        }
    }
}
