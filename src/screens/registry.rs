//! The apps shown on the launcher, in order. Adding an app = adding an entry.

use crate::ui::icons::Icon;
use crate::ui::theme::color;

use super::files::Folder;
use super::nav::{Cx, Screen};
use super::query_home::QueryHome;
use super::search::WikiResults;
use super::settings::SettingsHome;
use super::youtube::Results;

pub struct AppEntry {
    pub name: &'static str,
    pub icon: Icon,
    /// Large-area accent (card fill), never used for text.
    pub accent: u32,
    /// Shows an "Offline" badge on the launcher when there's no internet.
    pub needs_network: bool,
    pub open: fn(&AppEntry, &mut Cx) -> Box<dyn Screen>,
}

pub fn apps() -> Vec<AppEntry> {
    vec![
        AppEntry {
            name: "YouTube",
            icon: Icon::Play,
            accent: color::YOUTUBE,
            needs_network: true,
            open: |_, cx| {
                Box::new(QueryHome::new(
                    "YouTube",
                    "Search YouTube",
                    |s| &mut s.youtube.recent,
                    |q, cx| Box::new(Results::new(q, cx)),
                    cx,
                ))
            },
        },
        AppEntry {
            name: "Search",
            icon: Icon::Search,
            accent: color::SEARCH,
            needs_network: true,
            open: |_, cx| {
                Box::new(QueryHome::new(
                    "Search",
                    "Search Wikipedia",
                    |s| &mut s.search.recent,
                    |q, cx| Box::new(WikiResults::new(q, cx)),
                    cx,
                ))
            },
        },
        AppEntry {
            name: "Files",
            icon: Icon::Folder,
            accent: color::FILES,
            needs_network: false,
            open: |_, cx| Box::new(Folder::root(cx)),
        },
        AppEntry {
            name: "Settings",
            icon: Icon::Gear,
            accent: color::SETTINGS,
            needs_network: false,
            open: |_, _| Box::new(SettingsHome::new()),
        },
    ]
}
