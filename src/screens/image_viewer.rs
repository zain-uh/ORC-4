//! Shows one picture, decoded and scaled on the worker to fit the content
//! area (never upscaled).

use std::path::PathBuf;

use tiny_skia::{IntSize, Pixmap};

use crate::app::action::Action;
use crate::services::storage;
use crate::services::{Reply, ServiceEvent, Ticket, UserError};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::theme::color;
use crate::ui::widgets::{message, status_bar};

use super::nav::{Cx, Nav, Screen};

enum Load {
    Loading(Ticket),
    Ready(Pixmap),
    Failed(UserError),
}

pub struct ImageViewer {
    title: String,
    load: Load,
}

impl ImageViewer {
    pub fn open(path: PathBuf, name: String, cx: &mut Cx) -> Self {
        let (_, _, w, h) = cx.theme().content();
        let ticket = cx
            .services
            .run(move || storage::load_image(&path, w as u32, h as u32).map(Reply::Image));
        Self {
            title: name,
            load: Load::Loading(ticket),
        }
    }
}

impl Screen for ImageViewer {
    fn handle(&mut self, _action: Action, _cx: &mut Cx) -> Nav {
        Nav::Unhandled
    }

    fn on_service(&mut self, event: ServiceEvent, _cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if !matches!(self.load, Load::Loading(t) if t == event.ticket) {
            return Err(event);
        }
        self.load = match event.result {
            Ok(Reply::Image(img)) => IntSize::from_wh(img.width, img.height)
                .and_then(|size| Pixmap::from_vec(img.rgba, size))
                .map_or(Load::Failed(UserError::Internal), Load::Ready),
            Ok(_) => Load::Failed(UserError::Internal),
            Err(e) => Load::Failed(e),
        };
        Ok(Nav::Stay)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));
        match &self.load {
            Load::Ready(image) => {
                let (left, top, width, height) = t.content();
                let x = left + (width - image.width() as f32) / 2.0;
                let y = top + (height - image.height() as f32) / 2.0;
                c.draw_image(image, x, y);
            }
            Load::Loading(_) => message::draw(c, t, "Loading…", None),
            Load::Failed(e) => message::draw(c, t, "Can't show picture", Some(e.message())),
        }
        status_bar::draw(c, t, cx.status, Some(&self.title));
    }
}
