//! Physical key → `Action` mapping.
//!
//! `Key` is our own platform-neutral key set: the simulator window and the
//! Linux evdev reader (M3) both translate their native codes into it, so there
//! is exactly one mapping table to configure.
//!
//! Bindings can be overridden in settings, e.g. for a Bluetooth clicker:
//!
//! ```toml
//! [input.keys]
//! space = "select"
//! backspace = "none"   # unbind
//! ```

use std::collections::{BTreeMap, HashMap};

use crate::app::action::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Backspace,
    Home,
    Tab,
    Space,
}

impl Key {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            "enter" => Self::Enter,
            "escape" | "esc" => Self::Escape,
            "backspace" => Self::Backspace,
            "home" => Self::Home,
            "tab" => Self::Tab,
            "space" => Self::Space,
            _ => return None,
        })
    }
}

pub struct Keymap {
    bindings: HashMap<Key, Action>,
}

impl Default for Keymap {
    fn default() -> Self {
        let bindings = HashMap::from([
            (Key::Up, Action::Up),
            (Key::Down, Action::Down),
            (Key::Left, Action::Left),
            (Key::Right, Action::Right),
            (Key::Enter, Action::Select),
            (Key::Escape, Action::Back),
            (Key::Backspace, Action::Back),
            (Key::Home, Action::Home),
            (Key::Tab, Action::Menu),
        ]);
        Self { bindings }
    }
}

impl Keymap {
    /// Defaults plus the overrides from settings. Unknown names are logged
    /// and skipped, so a typo never disables the keyboard.
    pub fn with_overrides(overrides: &BTreeMap<String, String>) -> Self {
        let mut map = Self::default();
        for (key_name, action_name) in overrides {
            let Some(key) = Key::from_name(&key_name.to_lowercase()) else {
                log::warn!("input.keys: unknown key {key_name:?}");
                continue;
            };
            match action_name.to_lowercase().as_str() {
                "none" => {
                    map.bindings.remove(&key);
                }
                name => match Action::from_name(name) {
                    Some(action) => {
                        map.bindings.insert(key, action);
                    }
                    None => log::warn!("input.keys: unknown action {action_name:?}"),
                },
            }
        }
        map
    }

    pub fn action(&self, key: Key) -> Option<Action> {
        self.bindings.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_cover_navigation() {
        let km = Keymap::default();
        assert_eq!(km.action(Key::Left), Some(Action::Left));
        assert_eq!(km.action(Key::Enter), Some(Action::Select));
        assert_eq!(km.action(Key::Escape), Some(Action::Back));
        assert_eq!(km.action(Key::Home), Some(Action::Home));
        assert_eq!(km.action(Key::Space), None);
    }

    #[test]
    fn overrides_bind_unbind_and_skip_typos() {
        let overrides = BTreeMap::from([
            ("Space".to_string(), "select".to_string()),
            ("backspace".to_string(), "none".to_string()),
            ("f13".to_string(), "back".to_string()),
            ("tab".to_string(), "jump".to_string()),
        ]);
        let km = Keymap::with_overrides(&overrides);
        assert_eq!(km.action(Key::Space), Some(Action::Select));
        assert_eq!(km.action(Key::Backspace), None);
        assert_eq!(
            km.action(Key::Tab),
            Some(Action::Menu),
            "bad action ignored"
        );
        assert_eq!(km.action(Key::Escape), Some(Action::Back), "defaults kept");
    }
}
