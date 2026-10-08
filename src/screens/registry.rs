//! The apps shown on the launcher, in order. Adding an app = adding an entry.

use crate::ui::icons::Icon;
use crate::ui::theme::color;

use super::files::Folder;
use super::nav::{Cx, Screen};
use super::placeholder::Placeholder;
use super::settings::SettingsHome;
use super::youtube::YoutubeHome;

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
            open: |_, cx| Box::new(YoutubeHome::new(cx)),
        },
        AppEntry {
            name: "Search",
            icon: Icon::Search,
            accent: color::SEARCH,
            needs_network: true,
            open: |e, _| Box::new(Placeholder::new(e, "Web search and reader arrive in M9.")),
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
