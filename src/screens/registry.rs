//! The apps shown on the launcher, in order. Adding an app = adding an entry.

use crate::ui::icons::Icon;
use crate::ui::nav::Screen;
use crate::ui::theme::color;

use super::placeholder::Placeholder;

pub struct AppEntry {
    pub name: &'static str,
    pub icon: Icon,
    /// Large-area accent (card fill), never used for text.
    pub accent: u32,
    pub open: fn(&AppEntry) -> Box<dyn Screen>,
}

pub fn apps() -> Vec<AppEntry> {
    vec![
        AppEntry {
            name: "YouTube",
            icon: Icon::Play,
            accent: color::YOUTUBE,
            open: |e| Box::new(Placeholder::new(e, "Search and playback arrive in M6.")),
        },
        AppEntry {
            name: "Search",
            icon: Icon::Search,
            accent: color::SEARCH,
            open: |e| Box::new(Placeholder::new(e, "Web search and reader arrive in M9.")),
        },
        AppEntry {
            name: "Files",
            icon: Icon::Folder,
            accent: color::FILES,
            open: |e| Box::new(Placeholder::new(e, "Offline file browser arrives in M4.")),
        },
        AppEntry {
            name: "Settings",
            icon: Icon::Gear,
            accent: color::SETTINGS,
            open: |e| Box::new(Placeholder::new(e, "Device settings arrive in M4.")),
        },
    ]
}
