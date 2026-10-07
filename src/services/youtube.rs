//! YouTube search through the `yt-dlp` program (run on the network pool).
//!
//! Each search starts `yt-dlp` once. If that proves too slow on the Pi Zero
//! (spike S1), a persistent helper process can replace `search` without any
//! screen changing. The query is passed as a single argument (no shell), so
//! it can't inject options or commands.
//!
//! Thumbnails come straight from YouTube's image server by video id.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::UserError;

const TIMEOUT: Duration = Duration::from_secs(45);
/// Largest thumbnail accepted (mqdefault.jpg is ~15–30 KB).
pub const MAX_THUMBNAIL_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration_secs: Option<u32>,
    pub live: bool,
}

impl Video {
    /// 320×180 thumbnail under `base` (normally `https://i.ytimg.com/vi`).
    pub fn thumbnail_url(&self, base: &str) -> String {
        format!("{}/{}/mqdefault.jpg", base.trim_end_matches('/'), self.id)
    }

    /// "12:34", "1:02:03", "Live", or empty when unknown.
    pub fn duration_label(&self) -> String {
        if self.live {
            return "Live".into();
        }
        match self.duration_secs {
            Some(s) if s >= 3600 => format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
            Some(s) => format!("{}:{:02}", s / 60, s % 60),
            None => String::new(),
        }
    }
}

/// Results `start..start+count` (0-based) for `query`.
pub fn search(
    ytdlp: &str,
    query: &str,
    start: usize,
    count: usize,
) -> Result<Vec<Video>, UserError> {
    let end = start + count;
    let started = Instant::now();
    let output = run(Command::new(ytdlp).args([
        "--ignore-config",
        "--flat-playlist",
        "--dump-json",
        "--no-warnings",
        "--playlist-items",
        &format!("{}:{end}", start + 1),
        &format!("ytsearch{end}:{query}"),
    ]))?;
    let videos = parse_entries(&output);
    log::info!(
        "youtube: {} results for {:?} [{}..{end}] in {} ms",
        videos.len(),
        query,
        start,
        started.elapsed().as_millis()
    );
    Ok(videos)
}

/// One JSON object per line, as printed by `--dump-json`. Lines that aren't
/// videos (or aren't JSON) are skipped.
fn parse_entries(output: &str) -> Vec<Video> {
    #[derive(Deserialize)]
    struct Entry {
        id: Option<String>,
        title: Option<String>,
        channel: Option<String>,
        uploader: Option<String>,
        duration: Option<f64>,
        live_status: Option<String>,
    }
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
        .filter_map(|e| {
            let id = e.id.filter(|id| is_video_id(id))?;
            Some(Video {
                title: e.title.unwrap_or_else(|| "Untitled".into()),
                channel: e.channel.or(e.uploader),
                duration_secs: e.duration.filter(|d| *d >= 0.0).map(|d| d.round() as u32),
                live: e.live_status.as_deref() == Some("is_live"),
                id,
            })
        })
        .collect()
}

/// Video ids are 11 characters of [A-Za-z0-9_-]. Checking keeps channel and
/// playlist entries out, and makes the id safe to put in a URL.
fn is_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Runs the command, returning stdout. Kills it after `TIMEOUT`.
fn run(cmd: &mut Command) -> Result<String, UserError> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            log::warn!("youtube: cannot start yt-dlp: {e}");
            if e.kind() == std::io::ErrorKind::NotFound {
                UserError::MissingTool
            } else {
                UserError::Internal
            }
        })?;

    // Read both pipes on their own threads so a full pipe can't stall it.
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out = thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                log::warn!("youtube: yt-dlp timed out");
                let _ = child.kill();
                let _ = child.wait();
                return Err(UserError::Network);
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                log::warn!("youtube: waiting for yt-dlp: {e}");
                return Err(UserError::Internal);
            }
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() && stdout.trim().is_empty() {
        log::warn!("youtube: yt-dlp failed ({status}): {}", stderr.trim());
        return Err(UserError::Network);
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yt_dlp_output_and_skips_non_videos() {
        let output = concat!(
            r#"{"_type":"url","id":"dQw4w9WgXcQ","title":"A song","channel":"Rick","duration":212.0}"#,
            "\n",
            r#"{"_type":"url","id":"UCuAXFkgsw1L7xaCfnd5JJOw","title":"A channel"}"#,
            "\n",
            "WARNING: not json\n",
            r#"{"id":"abcdefghijk","title":"Live now","uploader":"Up","live_status":"is_live"}"#,
            "\n",
        );
        let videos = parse_entries(output);
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].channel.as_deref(), Some("Rick"));
        assert_eq!(videos[0].duration_label(), "3:32");
        assert_eq!(
            videos[1].channel.as_deref(),
            Some("Up"),
            "falls back to uploader"
        );
        assert_eq!(videos[1].duration_label(), "Live");
    }

    #[test]
    fn durations_and_thumbnails() {
        let mut v = Video {
            id: "dQw4w9WgXcQ".into(),
            title: String::new(),
            channel: None,
            duration_secs: Some(3723),
            live: false,
        };
        assert_eq!(v.duration_label(), "1:02:03");
        v.duration_secs = None;
        assert_eq!(v.duration_label(), "");
        assert_eq!(
            v.thumbnail_url("https://i.ytimg.com/vi/"),
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
        );
    }

    #[test]
    fn video_ids_are_validated() {
        assert!(is_video_id("dQw4w9WgXcQ"));
        assert!(!is_video_id("../../etc/x"));
        assert!(!is_video_id("short"));
    }

    #[test]
    fn missing_program_is_reported_as_missing_tool() {
        assert_eq!(
            search("definitely-not-yt-dlp-xyz", "cats", 0, 4).unwrap_err(),
            UserError::MissingTool
        );
    }

    /// Real search; needs yt-dlp and internet. `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search() {
        let videos = search("yt-dlp", "raspberry pi zero", 0, 4).unwrap();
        assert!(!videos.is_empty());
    }
}
