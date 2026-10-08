//! Network state: is the internet reachable, and Wi-Fi control.
//!
//! The internet check fetches a "204 No Content" page: an empty answer means
//! online, any other answer means a hotel/café sign-in page is in the way,
//! no answer means offline. The URL is a setting (empty disables checking).
//!
//! Wi-Fi control sits behind the `Wifi` trait. The device backend (iwd over
//! D-Bus) arrives in M8b, built and tested on the Pi. Until then the
//! simulator uses a clearly labelled fake, so the screens can be built and
//! tested on the dev PC.

use std::sync::OnceLock;

use super::{UserError, http};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Internet {
    #[default]
    Unknown,
    Online,
    /// Connected, but a sign-in page intercepts traffic.
    SignIn,
    Offline,
}

/// What the status bar and offline badges show.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetInfo {
    pub internet: Internet,
    /// Connected Wi-Fi network, if any.
    pub ssid: Option<String>,
    /// 0–4 bars.
    pub signal: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0–4 bars.
    pub signal: u8,
    pub secured: bool,
    /// Has saved credentials.
    pub known: bool,
    pub connected: bool,
}

pub trait Wifi: Send + Sync {
    /// Name shown in Settings (says when it is simulated).
    fn label(&self) -> &'static str;
    /// Currently connected network: (ssid, bars).
    fn current(&self) -> Result<Option<(String, u8)>, UserError>;
    fn scan(&self) -> Result<Vec<WifiNetwork>, UserError>;
    fn connect(&self, ssid: &str, passphrase: Option<&str>) -> Result<(), UserError>;
    fn forget(&self, ssid: &str) -> Result<(), UserError>;
}

/// The Wi-Fi backend for this build.
pub fn wifi() -> &'static dyn Wifi {
    static WIFI: OnceLock<Box<dyn Wifi>> = OnceLock::new();
    WIFI.get_or_init(|| {
        if cfg!(feature = "simulator") {
            Box::new(simulated::SimulatedWifi::default())
        } else {
            Box::new(Unavailable)
        }
    })
    .as_ref()
}

/// Checks the internet (and the current Wi-Fi network). Runs on the
/// network pool; takes up to the HTTP timeout when offline.
pub fn check(probe_url: &str) -> NetInfo {
    let current = wifi().current().ok().flatten();
    let internet = if probe_url.is_empty() {
        Internet::Unknown
    } else {
        match http::get(probe_url, 4096) {
            Ok(body) if body.is_empty() => Internet::Online,
            Ok(_) => Internet::SignIn,
            Err(_) => Internet::Offline,
        }
    };
    NetInfo {
        internet,
        signal: current.as_ref().map(|(_, bars)| *bars),
        ssid: current.map(|(ssid, _)| ssid),
    }
}

/// Device builds until the iwd backend lands (M8b).
struct Unavailable;

impl Wifi for Unavailable {
    fn label(&self) -> &'static str {
        "Wi-Fi control not available yet"
    }
    fn current(&self) -> Result<Option<(String, u8)>, UserError> {
        Ok(None)
    }
    fn scan(&self) -> Result<Vec<WifiNetwork>, UserError> {
        Err(UserError::Unsupported)
    }
    fn connect(&self, _: &str, _: Option<&str>) -> Result<(), UserError> {
        Err(UserError::Unsupported)
    }
    fn forget(&self, _: &str) -> Result<(), UserError> {
        Err(UserError::Unsupported)
    }
}

/// Fake Wi-Fi for developing the screens on a PC. "ORC Test" accepts the
/// password `orc4test`; "Cafe" is open; everything else fails to connect.
mod simulated {
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    use super::{UserError, Wifi, WifiNetwork};

    pub struct SimulatedWifi {
        networks: Mutex<Vec<WifiNetwork>>,
    }

    impl Default for SimulatedWifi {
        fn default() -> Self {
            let net = |ssid: &str, signal, secured, known, connected| WifiNetwork {
                ssid: ssid.into(),
                signal,
                secured,
                known,
                connected,
            };
            Self {
                networks: Mutex::new(vec![
                    net("Home", 4, true, true, true),
                    net("ORC Test", 3, true, false, false),
                    net("Cafe", 2, false, false, false),
                    net("Neighbour 5G", 1, true, false, false),
                ]),
            }
        }
    }

    impl Wifi for SimulatedWifi {
        fn label(&self) -> &'static str {
            "Simulated Wi-Fi"
        }

        fn current(&self) -> Result<Option<(String, u8)>, UserError> {
            let nets = self.networks.lock().map_err(|_| UserError::Internal)?;
            Ok(nets
                .iter()
                .find(|n| n.connected)
                .map(|n| (n.ssid.clone(), n.signal)))
        }

        fn scan(&self) -> Result<Vec<WifiNetwork>, UserError> {
            thread::sleep(Duration::from_millis(600)); // scans take a moment
            let mut nets = self
                .networks
                .lock()
                .map_err(|_| UserError::Internal)?
                .clone();
            nets.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
            Ok(nets)
        }

        fn connect(&self, ssid: &str, passphrase: Option<&str>) -> Result<(), UserError> {
            thread::sleep(Duration::from_millis(800));
            let mut nets = self.networks.lock().map_err(|_| UserError::Internal)?;
            let target = nets
                .iter()
                .find(|n| n.ssid == ssid)
                .cloned()
                .ok_or(UserError::NotFound)?;
            let ok = match ssid {
                "ORC Test" => target.known || passphrase == Some("orc4test"),
                "Cafe" | "Home" => true,
                _ => false,
            };
            if !ok {
                return Err(if target.secured {
                    UserError::WrongPassword
                } else {
                    UserError::Network
                });
            }
            for n in nets.iter_mut() {
                n.connected = n.ssid == ssid;
                if n.ssid == ssid {
                    n.known = true;
                }
            }
            Ok(())
        }

        fn forget(&self, ssid: &str) -> Result<(), UserError> {
            let mut nets = self.networks.lock().map_err(|_| UserError::Internal)?;
            for n in nets.iter_mut().filter(|n| n.ssid == ssid) {
                n.known = false;
                n.connected = false;
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn connects_with_the_right_password_only() {
            let w = SimulatedWifi::default();
            assert_eq!(
                w.connect("ORC Test", Some("nope")),
                Err(UserError::WrongPassword)
            );
            assert!(w.connect("ORC Test", Some("orc4test")).is_ok());
            assert_eq!(w.current().unwrap().unwrap().0, "ORC Test");
            assert!(
                w.scan().unwrap()[0].connected,
                "connected network listed first"
            );
        }

        #[test]
        fn forgetting_disconnects() {
            let w = SimulatedWifi::default();
            w.forget("Home").unwrap();
            assert_eq!(w.current().unwrap(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_probe_url_disables_checking() {
        assert_eq!(check("").internet, Internet::Unknown);
    }

    #[test]
    fn unreachable_probe_means_offline() {
        assert_eq!(
            check("http://127.0.0.1:9/generate_204").internet,
            Internet::Offline
        );
    }
}
