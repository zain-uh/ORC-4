//! Settings model, layered loading and power-loss-safe saving.
//!
//! Layers, lowest priority first:
//!   1. compiled defaults (`Settings::default`)
//!   2. system config   (`/etc/orc4/config.toml`, shipped in the image)
//!   3. user settings   (`<data_dir>/settings.toml`, written by the Settings app)
//!
//! A layer that fails to parse is skipped with a warning, so a bad file can
//! never stop the device from booting. A corrupt user file is moved aside.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use toml::{Table, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// error | warn | info | debug | trace. `ORC4_LOG` overrides it.
    pub log_level: String,
    pub clock: ClockSettings,
    pub display: DisplaySettings,
    pub input: InputSettings,
    pub phone: PhoneSettings,
    pub youtube: YoutubeSettings,
    pub search: SearchSettings,
    pub player: PlayerSettings,
    pub network: NetworkSettings,
    pub simulator: SimulatorSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            log_level: "info".into(),
            clock: ClockSettings::default(),
            display: DisplaySettings::default(),
            input: InputSettings::default(),
            phone: PhoneSettings::default(),
            youtube: YoutubeSettings::default(),
            search: SearchSettings::default(),
            player: PlayerSettings::default(),
            network: NetworkSettings::default(),
            simulator: SimulatorSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    /// Safe-area inset from the left/right and top/bottom edges, in logical
    /// pixels. Calibrated per device because the AV board crops the edges.
    pub edge_x: f32,
    pub edge_y: f32,
    /// Flip the picture for the optics (applied by the KMS backend, M3).
    pub mirror_x: bool,
    pub mirror_y: bool,
    /// Composite video standard (applied by the KMS backend, M3).
    pub tv_norm: TvNorm,
    /// Replace slides and fades with instant changes.
    pub reduce_motion: bool,
}

impl DisplaySettings {
    pub const EDGE_MAX: f32 = 120.0;
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            edge_x: 56.0,
            edge_y: 40.0,
            mirror_x: false,
            mirror_y: false,
            tv_norm: TvNorm::Pal,
            reduce_motion: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TvNorm {
    Pal,
    Ntsc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Page used to check for internet access: it must answer
    /// "204 No Content". Each check contacts this server, so it is
    /// configurable; empty turns checking off.
    pub probe_url: String,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            probe_url: "http://connectivitycheck.gstatic.com/generate_204".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerSettings {
    /// The mpv program: a name on PATH or a full path.
    pub mpv: String,
    /// yt-dlp format choice. Default: H.264 at 480p or less (the Pi Zero
    /// decodes H.264 in hardware; the display is 540 lines tall anyway).
    pub format: String,
    /// Extra mpv options for this device (video output, decoding).
    pub extra_args: Vec<String>,
    /// Last volume, 0–100.
    pub volume: u32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        let extra: &[&str] = if cfg!(feature = "simulator") {
            &[
                "--force-window=immediate",
                "--geometry=960x540",
                "--title=ORC-4 video",
            ]
        } else {
            &["--vo=gpu", "--gpu-context=drm", "--hwdec=v4l2m2m-copy"]
        };
        Self {
            mpv: "mpv".into(),
            format: "bv*[vcodec^=avc1][height<=480]+ba[ext=m4a]/b[vcodec^=avc1][height<=480]/b[height<=480]/b"
                .into(),
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
            volume: 70,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchSettings {
    /// Wikipedia language edition ("en", "de", "fr"...).
    pub language: String,
    /// Recent searches, newest first.
    pub recent: Vec<String>,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            language: "en".into(),
            recent: Vec::new(),
        }
    }
}

/// How many recent searches each app keeps.
pub const MAX_RECENT: usize = 8;

/// Moves `query` to the front of a recent-search list, without duplicates
/// (ignoring case).
pub fn remember_query(recent: &mut Vec<String>, query: &str) {
    let key = query.to_lowercase();
    recent.retain(|q| q.to_lowercase() != key);
    recent.insert(0, query.to_string());
    recent.truncate(MAX_RECENT);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YoutubeSettings {
    /// The yt-dlp program: a name on PATH or a full path.
    pub ytdlp: String,
    /// Where thumbnails come from (`<base>/<video id>/mqdefault.jpg`). Can
    /// point at a privacy proxy.
    pub thumbnails: String,
    /// Recent searches, newest first (cleared from the YouTube screen).
    pub recent: Vec<String>,
}

impl Default for YoutubeSettings {
    fn default() -> Self {
        Self {
            ytdlp: "yt-dlp".into(),
            thumbnails: "https://i.ytimg.com/vi".into(),
            recent: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhoneSettings {
    /// Accept phone connections at all.
    pub enabled: bool,
    /// Address the phone link listens on. The simulator only listens
    /// locally, so the PC firewall never asks.
    pub listen: String,
    /// Name shown to the phone.
    pub name: String,
    pub paired: Vec<PairedPhone>,
}

impl Default for PhoneSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            listen: if cfg!(feature = "simulator") {
                "127.0.0.1:4004"
            } else {
                "0.0.0.0:4004"
            }
            .into(),
            name: "ORC-4".into(),
            paired: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedPhone {
    pub name: String,
    /// Secret issued at pairing; the phone presents it on every connect.
    pub token: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSettings {
    /// Key name -> action name overrides, e.g. `space = "select"`. See
    /// `input::keymap` for the names. Lenient: unknown names are skipped.
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockSettings {
    /// Minutes east of UTC (UK summer time = 60). The phone will set this.
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatorSettings {
    /// Integer window scale on the dev PC (1 = 960×540, 2 = 1920×1080).
    pub scale: u32,
    /// Start with the composite-video preview filter on (toggle with F2).
    pub composite_preview: bool,
}

impl Default for SimulatorSettings {
    fn default() -> Self {
        Self {
            scale: 1,
            composite_preview: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub system_config: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    /// Device builds use the fixed image layout; simulator builds keep
    /// everything under `./dev-data`. Both can be overridden by env vars.
    pub fn from_env() -> Self {
        let (system_config, data_dir) = if cfg!(feature = "simulator") {
            (
                PathBuf::from("dev-data/config.toml"),
                PathBuf::from("dev-data"),
            )
        } else {
            (
                PathBuf::from("/etc/orc4/config.toml"),
                PathBuf::from("/data/orc4"),
            )
        };
        Self {
            system_config: std::env::var_os("ORC4_SYSTEM_CONFIG")
                .map(PathBuf::from)
                .unwrap_or(system_config),
            data_dir: std::env::var_os("ORC4_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(data_dir),
        }
    }

    pub fn user_settings(&self) -> PathBuf {
        self.data_dir.join("settings.toml")
    }

    /// Root of the Files app.
    pub fn files(&self) -> PathBuf {
        self.data_dir.join("files")
    }
}

pub fn load(paths: &Paths) -> Settings {
    let mut merged = Table::new();

    if let Some(layer) = read_layer(&paths.system_config) {
        merge(&mut merged, layer);
    }

    let user_path = paths.user_settings();
    match read_layer_checked(&user_path) {
        Ok(Some(layer)) => merge(&mut merged, layer),
        Ok(None) => {}
        Err(err) => {
            log::warn!(
                "user settings {} unusable ({err:#}); using defaults",
                user_path.display()
            );
            quarantine(&user_path);
        }
    }

    // Each layer was validated on its own, so the merge cannot fail in
    // practice; fall back to defaults rather than panic if it somehow does.
    merged.try_into().unwrap_or_else(|err| {
        log::error!("merged settings invalid ({err}); using defaults");
        Settings::default()
    })
}

/// Defaults plus the system config, without the user's changes ("Reset all
/// settings").
pub fn system_defaults(paths: &Paths) -> Settings {
    let mut merged = Table::new();
    if let Some(layer) = read_layer(&paths.system_config) {
        merge(&mut merged, layer);
    }
    merged.try_into().unwrap_or_default()
}

/// Saves the user layer: only values that differ from defaults + system
/// config. Values the user never touched stay unpinned, so a later image
/// update can still change them.
pub fn save_user(paths: &Paths, settings: &Settings) -> anyhow::Result<()> {
    let mut base = to_table(&Settings::default())?;
    if let Some(system) = read_layer(&paths.system_config) {
        merge(&mut base, system);
    }
    let changes = diff(&to_table(settings)?, &base);
    let text = toml::to_string_pretty(&changes).context("serialize settings")?;
    write_atomic(&paths.user_settings(), &text)
}

fn to_table(settings: &Settings) -> anyhow::Result<Table> {
    match Value::try_from(settings).context("serialize settings")? {
        Value::Table(t) => Ok(t),
        _ => anyhow::bail!("settings did not serialize to a table"),
    }
}

/// Keys of `current` whose values differ from `base` (recursing into tables).
fn diff(current: &Table, base: &Table) -> Table {
    let mut out = Table::new();
    for (key, value) in current {
        match (value, base.get(key)) {
            (Value::Table(c), Some(Value::Table(b))) => {
                let d = diff(c, b);
                if !d.is_empty() {
                    out.insert(key.clone(), Value::Table(d));
                }
            }
            (v, Some(b)) if v == b => {}
            (v, _) => {
                out.insert(key.clone(), v.clone());
            }
        }
    }
    out
}

/// Writes `text` so that a power cut leaves either the old or the new file
/// on disk, never a truncated one.
fn write_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;

    let tmp = path.with_extension("toml.tmp");
    {
        let mut file =
            fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;

    // Persist the rename itself (directory entry) on Linux.
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// System layer: skipped on any problem (it lives on a read-only root, so we
/// cannot move it aside).
fn read_layer(path: &Path) -> Option<Table> {
    match read_layer_checked(path) {
        Ok(layer) => layer,
        Err(err) => {
            log::warn!("ignoring {} ({err:#})", path.display());
            None
        }
    }
}

/// `Ok(None)` when the file does not exist. Validates the layer against the
/// schema on its own so a bad value is attributed to the right file.
fn read_layer_checked(path: &Path) -> anyhow::Result<Option<Table>> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            log::debug!("no config at {}", path.display());
            return Ok(None);
        }
        Err(e) => return Err(e).context("read"),
    };
    let table: Table = toml::from_str(&text).context("parse")?;
    table.clone().try_into::<Settings>().context("schema")?;
    log::info!("loaded config {}", path.display());
    Ok(Some(table))
}

fn quarantine(path: &Path) {
    let bad = path.with_extension("toml.bad");
    match fs::rename(path, &bad) {
        Ok(()) => log::warn!("moved corrupt settings to {}", bad.display()),
        Err(e) => log::warn!("could not move corrupt settings aside: {e}"),
    }
}

/// Deep merge: tables merge key by key, any other value replaces.
fn merge(base: &mut Table, over: Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(b)), Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(key, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh, empty directory per test under the system temp dir.
    fn temp_paths(name: &str) -> Paths {
        let dir = std::env::temp_dir().join(format!("orc4-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Paths {
            system_config: dir.join("system.toml"),
            data_dir: dir,
        }
    }

    #[test]
    fn missing_files_give_defaults() {
        let p = temp_paths("missing");
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn user_layer_overrides_system_layer_per_key() {
        let p = temp_paths("layers");
        fs::write(
            &p.system_config,
            "log_level = \"debug\"\n[simulator]\nscale = 2\n",
        )
        .unwrap();
        fs::write(p.user_settings(), "log_level = \"warn\"\n").unwrap();
        let s = load(&p);
        assert_eq!(s.log_level, "warn");
        assert_eq!(
            s.simulator.scale, 2,
            "system value survives when user does not set it"
        );
    }

    #[test]
    fn corrupt_user_file_is_moved_aside_and_defaults_used() {
        let p = temp_paths("corrupt");
        fs::write(p.user_settings(), "this is = = not toml").unwrap();
        assert_eq!(load(&p), Settings::default());
        assert!(!p.user_settings().exists());
        assert!(p.user_settings().with_extension("toml.bad").exists());
    }

    #[test]
    fn wrong_type_in_system_layer_is_ignored() {
        let p = temp_paths("wrongtype");
        fs::write(&p.system_config, "[simulator]\nscale = \"big\"\n").unwrap();
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn unknown_keys_are_tolerated() {
        let p = temp_paths("unknown");
        fs::write(
            p.user_settings(),
            "future_option = true\nlog_level = \"error\"\n",
        )
        .unwrap();
        assert_eq!(load(&p).log_level, "error");
    }

    #[test]
    fn save_then_load_round_trips() {
        let p = temp_paths("roundtrip");
        let mut s = Settings::default();
        s.simulator.scale = 2;
        s.display.tv_norm = TvNorm::Ntsc;
        save_user(&p, &s).unwrap();
        assert!(!p.user_settings().with_extension("toml.tmp").exists());
        assert_eq!(load(&p), s);
    }

    #[test]
    fn save_writes_only_user_changes() {
        let p = temp_paths("diff");
        fs::write(&p.system_config, "[display]\nedge_x = 70.0\n").unwrap();
        let mut s = load(&p);
        assert_eq!(s.display.edge_x, 70.0);
        s.display.reduce_motion = true;
        save_user(&p, &s).unwrap();

        let written = fs::read_to_string(p.user_settings()).unwrap();
        assert!(written.contains("reduce_motion = true"), "{written}");
        assert!(
            !written.contains("edge_x"),
            "system value not pinned: {written}"
        );
        assert!(
            !written.contains("log_level"),
            "defaults not written: {written}"
        );

        // A later image update to the system layer still takes effect.
        fs::write(&p.system_config, "[display]\nedge_x = 64.0\n").unwrap();
        let after = load(&p);
        assert_eq!(after.display.edge_x, 64.0);
        assert!(after.display.reduce_motion);
    }
}
