//! Search results as a horizontal carousel: one large thumbnail with a
//! readable title in the centre, neighbours peeking in from both sides (the
//! same pattern as the launcher). Composite video can't show four small
//! titles legibly, so this replaces the prototype's 2×2 grid.
//!
//! Results load 12 at a time; more are fetched automatically when focus gets
//! within 4 of the end, de-duplicated *before* thumbnails are requested.
//! Thumbnails download in parallel on the network pool. Enter plays.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::time::Instant;

use tiny_skia::{IntSize, Pixmap};

use crate::app::action::Action;
use crate::app::anim::{Ease, Tween};
use crate::screens::nav::{Cx, Nav, Screen};
use crate::screens::player::{Media, PlayerScreen};
use crate::services::youtube::{self, MAX_THUMBNAIL_BYTES, Video};
use crate::services::{JobResult, Reply, ServiceEvent, Ticket, UserError, http, storage};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, WIDTH, rgb, rgba};
use crate::ui::icons::{self, Icon};
use crate::ui::text::{Align, FontId, fonts};
use crate::ui::theme::{color, motion, space, typography};
use crate::ui::widgets::{message, status_bar};

const PAGE: usize = 12;
const MAX_RESULTS: usize = 60;
/// Fetch the next page when focus is this close to the end.
const PREFETCH: usize = 4;

const THUMB_W: f32 = 448.0;
const THUMB_H: f32 = 252.0;
const THUMB_TOP: f32 = 100.0;
const SPACING: f32 = 387.0;
const NEIGHBOUR_SCALE: f32 = 0.62;
const NEIGHBOUR_ALPHA: f32 = 0.45;
const BUMP_DISTANCE: f32 = 24.0;

enum Thumb {
    Loading,
    Ready(Pixmap),
    Failed,
}

pub struct Results {
    query: String,
    ytdlp: String,
    thumbnail_base: String,
    /// Downloads and decodes one thumbnail (replaced in tests, so they
    /// never start downloads).
    fetch_thumbnail: fn(&str) -> JobResult,
    videos: Vec<Video>,
    thumbs: HashMap<String, Thumb>,
    thumb_tickets: HashMap<Ticket, String>,
    /// Search in flight.
    search: Option<Ticket>,
    exhausted: bool,
    /// Shown when the first search fails.
    error: Option<UserError>,
    index: usize,
    scroll: Tween,
    bump: Option<(Instant, f32)>,
    // Sampled by `animate`, read by `draw`.
    pos: f32,
    bump_offset: f32,
}

impl Results {
    pub fn new(query: String, cx: &mut Cx) -> Self {
        let mut results = Self {
            query,
            ytdlp: cx.settings.youtube.ytdlp.clone(),
            thumbnail_base: cx.settings.youtube.thumbnails.clone(),
            fetch_thumbnail: download_thumbnail,
            videos: Vec::new(),
            thumbs: HashMap::new(),
            thumb_tickets: HashMap::new(),
            search: None,
            exhausted: false,
            error: None,
            index: 0,
            scroll: Tween::at(0.0),
            bump: None,
            pos: 0.0,
            bump_offset: 0.0,
        };
        results.load_more(cx);
        results
    }

    fn load_more(&mut self, cx: &mut Cx) {
        if self.search.is_some() || self.exhausted || self.videos.len() >= MAX_RESULTS {
            return;
        }
        let (bin, query, start) = (self.ytdlp.clone(), self.query.clone(), self.videos.len());
        self.search = Some(
            cx.services
                .run_net(move || youtube::search(&bin, &query, start, PAGE).map(Reply::Videos)),
        );
    }

    /// Appends new videos (skipping ones already shown) and starts their
    /// thumbnail downloads.
    fn add(&mut self, found: Vec<Video>, cx: &mut Cx) {
        let fetched = found.len();
        let mut known: HashSet<String> = self.videos.iter().map(|v| v.id.clone()).collect();
        let fresh: Vec<Video> = found
            .into_iter()
            .filter(|v| known.insert(v.id.clone()))
            .collect();
        if fetched < PAGE || fresh.is_empty() {
            self.exhausted = true;
        }
        for video in &fresh {
            let (url, fetch) = (
                video.thumbnail_url(&self.thumbnail_base),
                self.fetch_thumbnail,
            );
            let ticket = cx.services.run_net(move || fetch(&url));
            self.thumbs.insert(video.id.clone(), Thumb::Loading);
            self.thumb_tickets.insert(ticket, video.id.clone());
        }
        self.videos.extend(fresh);
    }

    fn step(&mut self, dir: isize, cx: &mut Cx) {
        let target = self.index as isize + dir;
        if !(0..self.videos.len() as isize).contains(&target) {
            if !cx.motion(motion::BUMP).is_zero() {
                self.bump = Some((cx.now, dir as f32));
            }
            return;
        }
        self.index = target as usize;
        self.scroll.retarget(
            self.index as f32,
            cx.now,
            cx.motion(motion::PANEL),
            Ease::OutCubic,
        );
        if self.index + PREFETCH >= self.videos.len() {
            self.load_more(cx);
        }
    }
}

fn download_thumbnail(url: &str) -> JobResult {
    let bytes = http::get(url, MAX_THUMBNAIL_BYTES)?;
    storage::decode_image(&bytes, THUMB_W as u32, THUMB_H as u32).map(Reply::Image)
}

impl Screen for Results {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        match action {
            Action::Left => self.step(-1, cx),
            Action::Right => self.step(1, cx),
            Action::Select if self.error.is_some() => {
                self.error = None;
                self.load_more(cx);
            }
            Action::Select if !self.videos.is_empty() => {
                let v = &self.videos[self.index];
                let media = Media {
                    title: fonts().printable(FontId::Sans, &v.title),
                    subtitle: v
                        .channel
                        .as_deref()
                        .map(|c| fonts().printable(FontId::Sans, c)),
                    // The id was validated as [A-Za-z0-9_-]{11}, so this URL is safe.
                    target: format!("https://www.youtube.com/watch?v={}", v.id),
                };
                return Nav::Push(Box::new(PlayerScreen::open(media, cx)));
            }
            _ => return Nav::Unhandled,
        }
        Nav::Stay
    }

    fn on_service(&mut self, event: ServiceEvent, cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if self.search == Some(event.ticket) {
            self.search = None;
            match event.result {
                Ok(Reply::Videos(found)) => self.add(found, cx),
                Ok(other) => log::error!("unexpected reply to search: {other:?}"),
                Err(e) if self.videos.is_empty() => self.error = Some(e),
                Err(e) => cx.toast(e.message()),
            }
            return Ok(Nav::Stay);
        }
        if let Some(id) = self.thumb_tickets.remove(&event.ticket) {
            let thumb = match event.result {
                Ok(Reply::Image(img)) => IntSize::from_wh(img.width, img.height)
                    .and_then(|size| Pixmap::from_vec(img.rgba, size))
                    .map_or(Thumb::Failed, Thumb::Ready),
                _ => Thumb::Failed,
            };
            self.thumbs.insert(id, thumb);
            return Ok(Nav::Stay);
        }
        Err(event)
    }

    fn animate(&mut self, now: Instant) -> bool {
        let pos = self.scroll.value(now);
        let bump_offset = match self.bump {
            Some((start, dir)) => {
                let t =
                    now.saturating_duration_since(start).as_secs_f32() / motion::BUMP.as_secs_f32();
                if t >= 1.0 {
                    self.bump = None;
                    0.0
                } else {
                    -dir * BUMP_DISTANCE * (PI * t).sin()
                }
            }
            None => 0.0,
        };
        let changed = pos != self.pos || bump_offset != self.bump_offset;
        self.pos = pos;
        self.bump_offset = bump_offset;
        changed
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));

        let quoted = format!("“{}”", self.query);
        if self.videos.is_empty() {
            let (title, detail) = match (&self.error, self.search) {
                (Some(e), _) => (e.message(), Some("Press Enter to try again.")),
                (None, Some(_)) => ("Searching…", None),
                (None, None) => ("No results", None),
            };
            message::draw(c, t, title, detail);
            status_bar::draw(c, t, cx.status, Some(&quoted));
            return;
        }

        // Farthest cards first, so the focused one is drawn on top.
        let mut order: Vec<(usize, f32)> = (0..self.videos.len())
            .map(|i| (i, i as f32 - self.pos))
            .filter(|(_, d)| d.abs() < 2.0)
            .collect();
        order.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        for (i, d) in order {
            self.draw_card(c, &self.videos[i], d);
        }

        let more = if self.exhausted { "" } else { "+" };
        let title = format!("{quoted} · {}/{}{more}", self.index + 1, self.videos.len());
        status_bar::draw(c, t, cx.status, Some(&title));
    }
}

impl Results {
    fn draw_card(&self, c: &mut Canvas, video: &Video, d: f32) {
        let k = d.abs().min(1.0);
        let scale = 1.0 - (1.0 - NEIGHBOUR_SCALE) * k;
        let alpha = 1.0 - (1.0 - NEIGHBOUR_ALPHA) * k;
        let (w, h) = (THUMB_W * scale, THUMB_H * scale);
        let centre_x = WIDTH as f32 / 2.0 + d * SPACING + self.bump_offset;
        let (x, y) = (centre_x - w / 2.0, THUMB_TOP + (THUMB_H - h) / 2.0);

        match self.thumbs.get(&video.id) {
            Some(Thumb::Ready(image)) => c.draw_image_scaled(image, x, y, w, h, alpha),
            _ => {
                c.fill_rect(x, y, w, h, rgba(color::SURFACE, alpha));
                icons::draw(
                    c,
                    Icon::Play,
                    centre_x,
                    y + h / 2.0,
                    h * 0.3,
                    rgba(color::TEXT_DISABLED, alpha),
                );
            }
        }

        // Title and details only for the card nearest the centre; they
        // cross-fade while sliding.
        let text_alpha = (1.0 - 2.0 * k).max(0.0);
        if text_alpha <= 0.0 {
            return;
        }
        let width = 760.0;
        let mut ty = THUMB_TOP + THUMB_H + space::S;
        let title = fonts().printable(FontId::Sans, &video.title);
        ty += c.paragraph(
            typography::BODY,
            &title,
            centre_x - width / 2.0,
            ty,
            width,
            2,
            Align::Center,
            rgba(color::TEXT, text_alpha),
        );
        let details: Vec<String> = [
            video
                .channel
                .as_deref()
                .map(|ch| fonts().printable(FontId::Sans, ch)),
            Some(video.duration_label()),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
        let details = details.join(" · ");
        let line = fonts().ellipsize(typography::CAPTION, &details, width);
        c.text(
            typography::CAPTION,
            &line,
            centre_x,
            ty,
            Align::Center,
            rgba(color::TEXT_SECONDARY, text_alpha),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::screens::nav::testing::Harness;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    fn video(n: usize) -> Video {
        Video {
            id: format!("vid{n:08}"),
            title: format!("Video number {n} about smart glasses \u{1F576}"),
            channel: Some("ORC Labs".into()),
            duration_secs: Some(60 * n as u32 + 7),
            live: false,
        }
    }

    /// Results whose first search is answered with `n` videos (the yt-dlp
    /// program is fake, so only the injected reply matters).
    fn results_with(h: &mut Harness, n: usize) -> Results {
        h.settings.youtube.ytdlp = "not-a-real-yt-dlp".into();
        let mut r = Results::new("smart glasses".into(), &mut h.cx(Instant::now()));
        // Thumbnail jobs fail at once instead of downloading anything.
        r.fetch_thumbnail = |_| Err(UserError::Network);
        let ticket = r.search.expect("search started");
        let found = (0..n).map(video).collect();
        let event = ServiceEvent {
            ticket,
            result: Ok(Reply::Videos(found)),
        };
        assert!(r.on_service(event, &mut h.cx(Instant::now())).is_ok());
        r
    }

    #[test]
    fn a_full_page_keeps_more_available_and_prefetches_near_the_end() {
        let mut h = Harness::new();
        let mut r = results_with(&mut h, PAGE);
        assert_eq!(r.videos.len(), PAGE);
        assert!(!r.exhausted);
        assert_eq!(r.thumb_tickets.len(), PAGE, "one thumbnail download each");
        assert!(r.search.is_none());

        let mut cx = h.cx(Instant::now());
        for _ in 0..(PAGE - PREFETCH - 1) {
            r.handle(Action::Right, &mut cx);
        }
        assert!(r.search.is_none(), "not yet near the end");
        r.handle(Action::Right, &mut cx);
        assert!(r.search.is_some(), "next page requested");
    }

    #[test]
    fn duplicates_are_dropped_before_thumbnails_are_fetched() {
        let mut h = Harness::new();
        let mut r = results_with(&mut h, PAGE);
        r.load_more(&mut h.cx(Instant::now()));
        let ticket = r.search.unwrap();
        // Page 2 repeats two videos from page 1.
        let mut page2: Vec<Video> = (PAGE..PAGE * 2 - 2).map(video).collect();
        page2.push(video(0));
        page2.push(video(1));
        let event = ServiceEvent {
            ticket,
            result: Ok(Reply::Videos(page2)),
        };
        assert!(r.on_service(event, &mut h.cx(Instant::now())).is_ok());
        assert_eq!(r.videos.len(), PAGE * 2 - 2);
        assert_eq!(r.thumb_tickets.len(), PAGE * 2 - 2);
    }

    #[test]
    fn short_page_means_no_more_and_errors_are_retryable() {
        let mut h = Harness::new();
        let r = results_with(&mut h, 3);
        assert!(r.exhausted);

        h.settings.youtube.ytdlp = "not-a-real-yt-dlp".into();
        let mut r = Results::new("x".into(), &mut h.cx(Instant::now()));
        let event = ServiceEvent {
            ticket: r.search.unwrap(),
            result: Err(UserError::Network),
        };
        assert!(r.on_service(event, &mut h.cx(Instant::now())).is_ok());
        assert_eq!(r.error, Some(UserError::Network));
        r.handle(Action::Select, &mut h.cx(Instant::now()));
        assert!(r.error.is_none() && r.search.is_some(), "Enter retries");
    }

    #[test]
    fn stops_at_the_ends_with_a_bump() {
        let mut h = Harness::new();
        let mut r = results_with(&mut h, 3);
        let mut cx = h.cx(Instant::now());
        r.handle(Action::Left, &mut cx);
        assert_eq!(r.index, 0);
        assert!(r.bump.is_some());
    }

    #[test]
    fn snapshot_results() {
        let mut h = Harness::new();
        let mut r = results_with(&mut h, PAGE);
        // One real-looking thumbnail (grey gradient), the rest placeholders.
        let mut thumb = Pixmap::new(320, 180).unwrap();
        thumb.fill(tiny_skia::Color::from_rgba8(70, 90, 120, 255));
        r.thumbs.insert(r.videos[1].id.clone(), Thumb::Ready(thumb));
        let t0 = Instant::now();
        r.handle(Action::Right, &mut h.cx(t0));
        r.animate(t0 + Duration::from_secs(1));

        let mut c = Canvas::new();
        let (theme, status) = (
            Theme::default(),
            Status {
                clock: Some((14, 32)),
                ..Status::default()
            },
        );
        r.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("youtube-results", &c);
    }
}
