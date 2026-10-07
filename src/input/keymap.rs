//! Physical key → `Action` mapping.
//!
//! `Key` is our own platform-neutral key set: the simulator window and the
//! Linux evdev reader (M3) both translate their native codes into it, so there
//! is exactly one mapping table to configure.

use std::collections::HashMap;

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
    }
}
