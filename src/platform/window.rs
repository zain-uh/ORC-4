//! Dev-PC simulator: shows the canvas in a desktop window and acts as the
//! keyboard input source.

use std::sync::mpsc::Sender;

use anyhow::anyhow;
use minifb::{Key as MfKey, KeyRepeat, Scale, Window, WindowOptions};

use super::Display;
use crate::app::action::{InputEvent, SourceKind};
use crate::app::event::Event;
use crate::input::keymap::{Key, Keymap};
use crate::ui::canvas::{Canvas, HEIGHT, WIDTH};

const W: usize = WIDTH as usize;
const H: usize = HEIGHT as usize;

pub struct WindowDisplay {
    window: Window,
    buffer: Vec<u32>,
    keymap: Keymap,
    pending: Vec<InputEvent>,
    /// `present` already polled the window this iteration; skip one `update`
    /// so the loop is paced once per frame, not twice.
    polled_by_present: bool,
}

impl WindowDisplay {
    pub fn new(scale: u32) -> anyhow::Result<Self> {
        let scale = match scale {
            2 => Scale::X2,
            4 => Scale::X4,
            _ => Scale::X1,
        };
        let mut window = Window::new(
            "ORC-4 simulator",
            W,
            H,
            WindowOptions {
                scale,
                ..WindowOptions::default()
            },
        )
        .map_err(|e| anyhow!("open simulator window: {e}"))?;
        window.set_target_fps(60);
        window.set_key_repeat_delay(0.35);
        window.set_key_repeat_rate(0.06);
        log::info!("simulator window {W}x{H}");

        Ok(Self {
            window,
            buffer: vec![0; W * H],
            keymap: Keymap::default(),
            pending: Vec::new(),
            polled_by_present: false,
        })
    }

    /// Reads keys seen by the last window update. Must run after every update,
    /// because the next update replaces them.
    fn collect_keys(&mut self) {
        let fresh = self.window.get_keys_pressed(KeyRepeat::No);
        for key in self.window.get_keys_pressed(KeyRepeat::Yes) {
            let Some(action) = map_key(key).and_then(|k| self.keymap.action(k)) else {
                continue;
            };
            self.pending.push(InputEvent {
                action,
                repeat: !fresh.contains(&key),
                source: SourceKind::Keyboard,
            });
        }
    }
}

impl Display for WindowDisplay {
    fn present(&mut self, canvas: &Canvas) -> anyhow::Result<()> {
        for (dst, px) in self.buffer.iter_mut().zip(canvas.pixmap().pixels()) {
            *dst = (px.red() as u32) << 16 | (px.green() as u32) << 8 | px.blue() as u32;
        }
        self.window
            .update_with_buffer(&self.buffer, W, H)
            .map_err(|e| anyhow!("present: {e}"))?;
        self.collect_keys();
        self.polled_by_present = true;
        Ok(())
    }

    fn pump(&mut self, events: &Sender<Event>) -> anyhow::Result<()> {
        if !std::mem::take(&mut self.polled_by_present) {
            self.window.update();
            self.collect_keys();
        }
        for ev in self.pending.drain(..) {
            let _ = events.send(Event::Input(ev));
        }
        if !self.window.is_open() {
            let _ = events.send(Event::Shutdown);
        }
        Ok(())
    }

    fn needs_polling(&self) -> bool {
        true
    }
}

fn map_key(key: MfKey) -> Option<Key> {
    Some(match key {
        MfKey::Up => Key::Up,
        MfKey::Down => Key::Down,
        MfKey::Left => Key::Left,
        MfKey::Right => Key::Right,
        MfKey::Enter | MfKey::NumPadEnter => Key::Enter,
        MfKey::Escape => Key::Escape,
        MfKey::Backspace => Key::Backspace,
        MfKey::Home => Key::Home,
        MfKey::Tab => Key::Tab,
        _ => return None,
    })
}
