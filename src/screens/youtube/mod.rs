//! YouTube app: the shared search screen (`QueryHome`) opens this results
//! carousel. Searching needs the internet and the `yt-dlp` program; both
//! failures are shown as plain messages, and nothing else in ORC-4 depends
//! on them.

mod results;

pub use results::Results;
