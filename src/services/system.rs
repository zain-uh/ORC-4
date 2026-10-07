//! Device facts (memory, temperature, uptime) and power actions.
//!
//! Reads small Linux pseudo-files, which take microseconds, so these are
//! called directly rather than as worker jobs. On the dev PC the files don't
//! exist and every value is `None` ("—" on screen).

use std::fs;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceInfo {
    pub model: Option<String>,
    pub uptime: Option<Duration>,
    /// (available, total) in KiB.
    pub memory_kib: Option<(u64, u64)>,
    pub cpu_temp_c: Option<f32>,
}

pub fn device_info() -> DeviceInfo {
    let read = |p: &str| fs::read_to_string(p).ok();
    DeviceInfo {
        model: read("/proc/device-tree/model").map(|m| m.trim_end_matches('\0').trim().to_string()),
        uptime: read("/proc/uptime").and_then(|t| parse_uptime(&t)),
        memory_kib: read("/proc/meminfo").and_then(|t| parse_meminfo(&t)),
        cpu_temp_c: read("/sys/class/thermal/thermal_zone0/temp").and_then(|t| parse_temp(&t)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Reboot,
    PowerOff,
}

/// Asks systemd to reboot or power off. In the simulator it only logs.
pub fn power(action: PowerAction) -> Result<(), String> {
    let verb = match action {
        PowerAction::Reboot => "reboot",
        PowerAction::PowerOff => "poweroff",
    };
    if cfg!(feature = "simulator") {
        log::warn!("simulator: would run `systemctl {verb}`");
        return Ok(());
    }
    log::info!("systemctl {verb}");
    Command::new("systemctl")
        .arg(verb)
        .spawn()
        .map(drop)
        .map_err(|e| format!("systemctl {verb}: {e}"))
}

fn parse_uptime(text: &str) -> Option<Duration> {
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(secs))
}

fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    };
    Some((field("MemAvailable:")?, field("MemTotal:")?))
}

fn parse_temp(text: &str) -> Option<f32> {
    Some(text.trim().parse::<f32>().ok()? / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_pseudo_files() {
        assert_eq!(
            parse_uptime("1234.56 4000.00\n"),
            Some(Duration::from_secs_f64(1234.56))
        );
        let meminfo = "MemTotal:         437248 kB\nMemFree:           12000 kB\nMemAvailable:     301234 kB\n";
        assert_eq!(parse_meminfo(meminfo), Some((301_234, 437_248)));
        assert_eq!(parse_temp("48312\n"), Some(48.312));
        assert_eq!(parse_temp("garbage"), None);
    }

    #[test]
    fn simulator_power_actions_only_log() {
        if cfg!(feature = "simulator") {
            assert!(power(PowerAction::Reboot).is_ok());
        }
    }
}
