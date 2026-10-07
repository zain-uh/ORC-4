//! Settings model, layered loading and power-loss-safe saving.
//!
//! Layers, lowest priority first:
//!   1. compiled defaults (`Settings::default`)
//!   2. system config   (`/etc/orc4/config.toml`, shipped in the image)
//!   3. user settings   (`<data_dir>/settings.toml`, written by the Settings app)
//!
//! A layer that fails to parse is skipped with a warning, so a bad file can
//! never stop the device from booting. A corrupt user file is moved aside.

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
    pub simulator: SimulatorSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            log_level: "info".into(),
            clock: ClockSettings::default(),
            simulator: SimulatorSettings::default(),
        }
    }
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

/// Writes `settings` so that a power cut leaves either the old or the new
/// file on disk, never a truncated one.
#[cfg_attr(not(test), allow(dead_code))] // first caller: the Settings app (M4)
pub fn save_atomic(path: &Path, settings: &Settings) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;

    let text = toml::to_string_pretty(settings).context("serialize settings")?;
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
        save_atomic(&p.user_settings(), &s).unwrap();
        assert!(!p.user_settings().with_extension("toml.tmp").exists());
        assert_eq!(load(&p), s);
    }
}
