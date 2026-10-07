//! Playback screen for YouTube videos and local media files.
//!
//! While video plays on the device, mpv owns the display and this screen is
//! hidden; it still receives input (from the keyboard or the phone) and
//! drives the player. In audio-only mode, or in the simulator, it shows a
//! "now playing" view with progress.
//!
//! Enter pause · ←/→ seek 10 s · ↑/↓ volume · Menu audio-only · Back stop.

use crate::app::action::Action;
use crate::services::player::{PlayOptions, Player, PlayerCmd, PlayerEvent, PlayerStatus};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::text::{Align, fonts};
use crate::ui::theme::{color, space, typography};
use crate::ui::widgets::{hint, status_bar};

use super::nav::{Cx, Nav, Screen};

const SEEK_SECS: f64 = 10.0;
const VOLUME_STEP: f64 = 5.0;
const BAR_H: f32 = 8.0;

/// Something to play.
#[derive(Debug, Clone)]
pub struct Media {
    pub title: String,
    pub subtitle: Option<String>,
    /// URL (YouTube page) or local file path.
    pub target: String,
}

pub struct PlayerScreen {
    media: Media,
    player: Player,
    status: PlayerStatus,
    video_on: bool,
}

impl PlayerScreen {
    pub fn open(media: Media, cx: &mut Cx) -> Self {
        let p = &cx.settings.player;
        let opts = PlayOptions {
            mpv: p.mpv.clone(),
            ytdlp: cx.settings.youtube.ytdlp.clone(),
            format: p.format.clone(),
            extra_args: p.extra_args.clone(),
            volume: p.volume,
        };
        log::info!("play {:?}", media.title);
        let player = cx.services.play(media.target.clone(), opts);
        Self::with_player(media, player)
    }

    fn with_player(media: Media, player: Player) -> Self {
        Self {
            media,
            player,
            status: PlayerStatus::default(),
            video_on: true,
        }
    }

    fn started(&self) -> bool {
        self.status.position.is_some()
    }
}

impl Screen for PlayerScreen {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        let cmd = match action {
            Action::Select => PlayerCmd::TogglePause,
            Action::Left => PlayerCmd::Seek(-SEEK_SECS),
            Action::Right => PlayerCmd::Seek(SEEK_SECS),
            Action::Up => PlayerCmd::AddVolume(VOLUME_STEP),
            Action::Down => PlayerCmd::AddVolume(-VOLUME_STEP),
            Action::Menu => {
                self.video_on = !self.video_on;
                cx.toast(if self.video_on {
                    "Video on"
                } else {
                    "Audio only"
                });
                PlayerCmd::Video(self.video_on)
            }
            // Back/Home close this screen; dropping the player stops mpv.
            _ => return Nav::Unhandled,
        };
        self.player.send(cmd);
        Nav::Stay
    }

    fn on_player(&mut self, event: &PlayerEvent, cx: &mut Cx) -> Nav {
        match event {
            PlayerEvent::Status(status) => {
                if let Some(v) = status.volume {
                    cx.settings.player.volume = v.round().clamp(0.0, 100.0) as u32;
                }
                self.status = status.clone();
                Nav::Stay
            }
            PlayerEvent::Ended(Ok(())) => Nav::Pop,
            PlayerEvent::Ended(Err(e)) => {
                cx.toast(e.message());
                Nav::Pop
            }
        }
    }

    fn takes_display(&self) -> bool {
        self.video_on && self.started()
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        let (left, top, width, _) = t.content();
        let mid = left + width / 2.0;

        let mut y = top + space::M;
        y += c.paragraph(
            typography::TITLE,
            &self.media.title,
            left,
            y,
            width,
            2,
            Align::Center,
            rgb(color::TEXT),
        );
        if let Some(sub) = &self.media.subtitle {
            let sub = fonts().ellipsize(typography::BODY, sub, width);
            c.text(
                typography::BODY,
                &sub,
                mid,
                y,
                Align::Center,
                rgb(color::TEXT_SECONDARY),
            );
        }

        // Progress bar and times.
        let bar_y = 330.0;
        c.fill_rounded_rect(
            left,
            bar_y,
            width,
            BAR_H,
            BAR_H / 2.0,
            rgb(color::SURFACE_RAISED),
        );
        let s = &self.status;
        if let (Some(pos), Some(dur)) = (s.position, s.duration)
            && dur > 0.0
        {
            let filled = width * (pos / dur).clamp(0.0, 1.0) as f32;
            // Skip until there is something to draw (an empty path is an error).
            if filled >= 1.0 {
                c.fill_rounded_rect(left, bar_y, filled, BAR_H, BAR_H / 2.0, rgb(color::TEXT));
            }
        }
        let times_y = bar_y + BAR_H + space::S;
        let style = typography::NUMERIC;
        c.text(
            style,
            &s.position.map_or("–:––".into(), clock),
            left,
            times_y,
            Align::Left,
            rgb(color::TEXT),
        );
        c.text(
            style,
            &s.duration.map_or(String::new(), clock),
            left + width,
            times_y,
            Align::Right,
            rgb(color::TEXT_SECONDARY),
        );

        let state = if !self.started() {
            "Loading…"
        } else if s.buffering {
            "Buffering…"
        } else if s.paused {
            "Paused"
        } else {
            ""
        };
        let mut info = vec![state.to_string()];
        if !self.video_on {
            info.push("Audio only".into());
        }
        if let Some(v) = s.volume {
            info.push(format!("Volume {}", v.round()));
        }
        let info: Vec<String> = info.into_iter().filter(|s| !s.is_empty()).collect();
        c.text(
            typography::BODY,
            &info.join(" · "),
            mid,
            times_y,
            Align::Center,
            rgb(color::TEXT),
        );

        status_bar::draw(c, t, cx.status, Some("Now playing"));
        hint::draw(
            c,
            t,
            "Enter pause · ← → 10 s · ↑ ↓ volume · Tab audio only · Esc stop",
        );
    }
}

/// "1:05", "1:02:03".
fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;
    use crate::services::UserError;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    fn media() -> Media {
        Media {
            title: "Raspberry Pi Zero 2 W review".into(),
            subtitle: Some("ORC Labs".into()),
            target: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
        }
    }

    #[test]
    fn controls_become_player_commands() {
        let mut h = Harness::new();
        let (player, cmds) = Player::detached();
        let mut s = PlayerScreen::with_player(media(), player);
        let mut cx = h.cx(Instant::now());
        for a in [
            Action::Select,
            Action::Left,
            Action::Right,
            Action::Up,
            Action::Down,
            Action::Menu,
        ] {
            assert!(matches!(s.handle(a, &mut cx), Nav::Stay));
        }
        let got: Vec<PlayerCmd> = cmds.try_iter().collect();
        assert_eq!(
            got,
            [
                PlayerCmd::TogglePause,
                PlayerCmd::Seek(-10.0),
                PlayerCmd::Seek(10.0),
                PlayerCmd::AddVolume(5.0),
                PlayerCmd::AddVolume(-5.0),
                PlayerCmd::Video(false),
            ]
        );
        assert!(matches!(s.handle(Action::Back, &mut cx), Nav::Unhandled));
    }

    #[test]
    fn closing_the_screen_stops_playback() {
        let (player, cmds) = Player::detached();
        drop(PlayerScreen::with_player(media(), player));
        assert_eq!(cmds.try_iter().last(), Some(PlayerCmd::Stop));
    }

    #[test]
    fn video_takes_the_display_only_once_playing_and_not_in_audio_mode() {
        let mut h = Harness::new();
        let (player, _cmds) = Player::detached();
        let mut s = PlayerScreen::with_player(media(), player);
        assert!(!s.takes_display(), "still loading");
        let playing = PlayerEvent::Status(PlayerStatus {
            position: Some(1.0),
            duration: Some(100.0),
            volume: Some(55.0),
            ..PlayerStatus::default()
        });
        let mut cx = h.cx(Instant::now());
        s.on_player(&playing, &mut cx);
        assert!(s.takes_display());
        assert_eq!(cx.settings.player.volume, 55, "volume remembered");
        s.handle(Action::Menu, &mut cx);
        assert!(!s.takes_display(), "audio only");
    }

    #[test]
    fn end_or_failure_closes_the_screen() {
        let mut h = Harness::new();
        let (player, _cmds) = Player::detached();
        let mut s = PlayerScreen::with_player(media(), player);
        let mut cx = h.cx(Instant::now());
        assert!(matches!(
            s.on_player(&PlayerEvent::Ended(Err(UserError::Playback)), &mut cx),
            Nav::Pop
        ));
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(65.4), "1:05");
        assert_eq!(clock(3723.0), "1:02:03");
    }

    #[test]
    fn snapshot_now_playing() {
        let (player, _cmds) = Player::detached();
        let mut s = PlayerScreen::with_player(media(), player);
        s.status = PlayerStatus {
            position: Some(83.0),
            duration: Some(212.0),
            paused: true,
            buffering: false,
            volume: Some(70.0),
        };
        s.video_on = false;
        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
            },
        );
        s.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("player", &c);
    }
}
