//! Files app: one `Folder` screen per folder level, so Back steps up one
//! folder and each level remembers its focus. Works fully offline.
//!
//! Listing happens on the worker and refreshes whenever the folder is shown
//! again (e.g. after a delete). Tab (Menu) on a row offers Delete.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::app::action::Action;
use crate::services::storage::{self, DirEntry, FileKind};
use crate::services::{Reply, ServiceEvent, Ticket, UserError};
use crate::ui::DrawCx;
use crate::ui::canvas::{Canvas, rgb};
use crate::ui::icons::Icon;
use crate::ui::theme::{color, motion};
use crate::ui::widgets::list::{ListView, Row};
use crate::ui::widgets::{hint, message, status_bar};

use super::confirm::Confirm;
use super::image_viewer::ImageViewer;
use super::nav::{Cx, Nav, Screen};
use super::player::{Media, PlayerScreen};
use super::text_viewer::TextViewer;

enum Load {
    NotStarted,
    Loading(Ticket),
    Ready,
    Failed(UserError),
}

pub struct Folder {
    title: String,
    path: PathBuf,
    is_root: bool,
    entries: Vec<DirEntry>,
    list: ListView,
    load: Load,
    /// Set by the delete confirmation so this screen can report the result.
    pending_delete: Rc<Cell<Option<Ticket>>>,
}

impl Folder {
    pub fn root(cx: &Cx) -> Self {
        Self::new("Files".into(), cx.paths.files(), true)
    }

    fn new(title: String, path: PathBuf, is_root: bool) -> Self {
        Self {
            title,
            path,
            is_root,
            entries: Vec::new(),
            list: ListView::new(0),
            load: Load::NotStarted,
            pending_delete: Rc::new(Cell::new(None)),
        }
    }

    fn reload(&mut self, cx: &mut Cx) {
        let path = self.path.clone();
        self.load = Load::Loading(
            cx.services
                .run(move || storage::list_dir(&path).map(Reply::Dir)),
        );
    }

    fn open(&self, cx: &mut Cx) -> Nav {
        let Some(entry) = self.entries.get(self.list.focus()) else {
            return Nav::Stay;
        };
        let (path, name) = (entry.path.clone(), entry.name.clone());
        match storage::kind(entry) {
            FileKind::Folder => {
                let title = format!("{} › {name}", self.title);
                Nav::Push(Box::new(Folder::new(title, path, false)))
            }
            FileKind::Text => Nav::Push(Box::new(TextViewer::open_file(path, name, cx))),
            FileKind::Image => Nav::Push(Box::new(ImageViewer::open(path, name, cx))),
            FileKind::Media => {
                let media = Media {
                    title: name,
                    subtitle: None,
                    target: path.to_string_lossy().into_owned(),
                };
                Nav::Push(Box::new(PlayerScreen::open(media, cx)))
            }
            FileKind::Other => {
                cx.toast(UserError::Unsupported.message());
                Nav::Stay
            }
        }
    }

    fn confirm_delete(&self) -> Nav {
        let Some(entry) = self.entries.get(self.list.focus()) else {
            return Nav::Stay;
        };
        let path = entry.path.clone();
        let pending = Rc::clone(&self.pending_delete);
        Nav::Push(Box::new(Confirm::new(
            format!("Delete “{}”?", entry.name),
            "Delete",
            move |cx| {
                let ticket = cx
                    .services
                    .run(move || storage::delete(&path).map(|()| Reply::Done));
                pending.set(Some(ticket));
            },
        )))
    }
}

impl Screen for Folder {
    fn handle(&mut self, action: Action, cx: &mut Cx) -> Nav {
        if self.list.handle(action, cx.now, cx.motion(motion::FOCUS)) {
            return Nav::Stay;
        }
        match action {
            Action::Select => self.open(cx),
            Action::Menu => self.confirm_delete(),
            _ => Nav::Unhandled,
        }
    }

    fn on_show(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn on_service(&mut self, event: ServiceEvent, cx: &mut Cx) -> Result<Nav, ServiceEvent> {
        if matches!(self.load, Load::Loading(t) if t == event.ticket) {
            match event.result {
                Ok(Reply::Dir(entries)) => {
                    self.list.set_len(entries.len());
                    self.entries = entries;
                    self.load = Load::Ready;
                }
                Ok(other) => {
                    log::error!("unexpected reply to list_dir: {other:?}");
                    self.load = Load::Failed(UserError::Internal);
                }
                Err(e) => self.load = Load::Failed(e),
            }
            return Ok(Nav::Stay);
        }
        if self.pending_delete.get() == Some(event.ticket) {
            self.pending_delete.set(None);
            match event.result {
                Ok(_) => {
                    cx.toast("Deleted");
                    self.reload(cx);
                }
                Err(e) => cx.toast(e.message()),
            }
            return Ok(Nav::Stay);
        }
        Err(event)
    }

    fn animate(&mut self, now: std::time::Instant) -> bool {
        self.list.animate(now)
    }

    fn draw(&self, c: &mut Canvas, cx: &DrawCx) {
        let t = cx.theme;
        c.clear(rgb(color::BACKGROUND));

        if !self.entries.is_empty() {
            let rows: Vec<Row> = self
                .entries
                .iter()
                .map(|e| Row {
                    icon: Some(match storage::kind(e) {
                        FileKind::Folder => Icon::Folder,
                        FileKind::Image => Icon::Picture,
                        FileKind::Media => Icon::Play,
                        _ => Icon::Document,
                    }),
                    label: &e.name,
                    detail: if e.is_dir {
                        String::new()
                    } else {
                        human_size(e.size)
                    },
                })
                .collect();
            self.list.draw(c, t, &rows);
            hint::draw(c, t, "Enter open · Tab delete · Esc back");
        } else {
            let empty_detail = format!("Add files to {}", self.path.display());
            let (title, detail) = match &self.load {
                Load::Failed(e) => ("Can't open folder", Some(e.message())),
                Load::Ready if self.is_root => ("No files yet", Some(empty_detail.as_str())),
                Load::Ready => ("This folder is empty", None),
                Load::NotStarted | Load::Loading(_) => ("Loading…", None),
            };
            message::draw(c, t, title, detail);
            hint::draw(c, t, "Esc back");
        }
        status_bar::draw(c, t, cx.status, Some(&self.title));
    }
}

/// "512 B", "2.4 KB", "13 MB": at most 3 significant digits.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Instant;

    use super::*;
    use crate::screens::nav::testing::Harness;
    use crate::ui::snapshot::assert_snapshot;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(2458), "2.4 KB");
        assert_eq!(human_size(13 * 1024 * 1024), "13 MB");
    }

    /// Lists a real temp folder through the worker, like the app does.
    fn loaded_folder(h: &mut Harness, dir: &std::path::Path) -> Folder {
        let mut f = Folder::new("Files".into(), dir.to_path_buf(), true);
        let mut cx = h.cx(Instant::now());
        f.on_show(&mut cx);
        drop(cx);
        let event = h.next_service_event();
        assert!(f.on_service(event, &mut h.cx(Instant::now())).is_ok());
        f
    }

    #[test]
    fn lists_then_deletes_after_confirmation() {
        let dir = std::env::temp_dir().join(format!("orc4-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Music")).unwrap();
        fs::write(dir.join("notes.txt"), "hello").unwrap();

        let mut h = Harness::new();
        let mut f = loaded_folder(&mut h, &dir);
        assert_eq!(f.entries.len(), 2);

        // Focus notes.txt, ask to delete, confirm.
        let mut cx = h.cx(Instant::now());
        f.handle(Action::Down, &mut cx);
        let Nav::Push(mut confirm) = f.handle(Action::Menu, &mut cx) else {
            panic!("expected confirmation")
        };
        confirm.handle(Action::Right, &mut cx);
        confirm.handle(Action::Select, &mut cx);
        drop(cx);

        let done = h.next_service_event();
        assert!(f.on_service(done, &mut h.cx(Instant::now())).is_ok());
        assert!(!dir.join("notes.txt").exists());
    }

    #[test]
    fn snapshot_folder() {
        let mut f = Folder::new("Files".into(), PathBuf::from("/data/files"), true);
        f.entries = vec![
            entry("Music", true, 0),
            entry("Photos", true, 0),
            entry("holiday.jpg", false, 2_400_000),
            entry(
                "A very long document name that will not fit.txt",
                false,
                812,
            ),
            entry("notes.txt", false, 2458),
            entry("todo.md", false, 120),
        ];
        f.list.set_len(f.entries.len());
        f.load = Load::Ready;

        let mut c = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
        };
        f.draw(
            &mut c,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );
        assert_snapshot("files-folder", &c);
    }

    fn entry(name: &str, is_dir: bool, size: u64) -> DirEntry {
        DirEntry {
            name: name.into(),
            path: PathBuf::from(name),
            is_dir,
            size,
        }
    }
}
