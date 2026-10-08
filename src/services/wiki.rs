//! Wikipedia: free search and plain-text articles through Wikimedia's
//! public APIs (no account or key). Articles come back as clean text with
//! section headings, so the reader shows exactly the article — no guessing
//! at web page layouts.
//!
//! Text is CC BY-SA; the reader credits Wikipedia at the end.

use serde::Deserialize;

use super::{UserError, http};

const MAX_SEARCH_BYTES: u64 = 256 * 1024;
const MAX_ARTICLE_BYTES: u64 = 2 * 1024 * 1024;
const RESULTS: usize = 12;

/// Trailing sections that are lists of links or empty in plain text.
const SKIPPED_SECTIONS: &[&str] = &[
    "References",
    "External links",
    "See also",
    "Notes",
    "Further reading",
    "Bibliography",
    "Sources",
    "Citations",
    "Footnotes",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub title: String,
    /// Short description ("General-purpose programming language").
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub heading: bool,
    pub text: String,
}

pub fn search(lang: &str, query: &str) -> Result<Vec<Hit>, UserError> {
    let url = format!(
        "https://{}.wikipedia.org/w/rest.php/v1/search/page?limit={RESULTS}&q={}",
        safe_lang(lang),
        http::encode(query)
    );
    parse_search(&http::get(&url, MAX_SEARCH_BYTES)?)
}

pub fn article(lang: &str, title: &str) -> Result<Vec<Block>, UserError> {
    let url = format!(
        "https://{}.wikipedia.org/w/api.php?action=query&prop=extracts&explaintext=1\
         &exsectionformat=wiki&redirects=1&format=json&formatversion=2&titles={}",
        safe_lang(lang),
        http::encode(title)
    );
    parse_article(&http::get(&url, MAX_ARTICLE_BYTES)?)
}

/// Language codes are short [a-z-] strings; anything else falls back to
/// English, so a bad setting can't change the host name.
fn safe_lang(lang: &str) -> &str {
    let ok =
        (2..=12).contains(&lang.len()) && lang.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    if ok { lang } else { "en" }
}

fn parse_search(body: &[u8]) -> Result<Vec<Hit>, UserError> {
    #[derive(Deserialize)]
    struct Response {
        pages: Vec<Page>,
    }
    #[derive(Deserialize)]
    struct Page {
        title: String,
        description: Option<String>,
    }
    let response: Response = serde_json::from_slice(body).map_err(|e| {
        log::warn!("wiki: unexpected search response: {e}");
        UserError::Unsupported
    })?;
    Ok(response
        .pages
        .into_iter()
        .map(|p| Hit {
            title: p.title,
            description: p.description.filter(|d| !d.trim().is_empty()),
        })
        .collect())
}

fn parse_article(body: &[u8]) -> Result<Vec<Block>, UserError> {
    #[derive(Deserialize)]
    struct Response {
        query: Query,
    }
    #[derive(Deserialize)]
    struct Query {
        pages: Vec<Page>,
    }
    #[derive(Deserialize)]
    struct Page {
        #[serde(default)]
        missing: bool,
        extract: Option<String>,
    }
    let response: Response = serde_json::from_slice(body).map_err(|e| {
        log::warn!("wiki: unexpected article response: {e}");
        UserError::Unsupported
    })?;
    let page = response
        .query
        .pages
        .into_iter()
        .next()
        .ok_or(UserError::NotFound)?;
    match page.extract {
        Some(text) if !page.missing && !text.trim().is_empty() => Ok(blocks(&text)),
        _ => Err(UserError::NotFound),
    }
}

/// Splits plain-text wiki output into headings and paragraphs. Headings look
/// like `== History ==`. Stops at the reference sections.
fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("==") && line.ends_with("==") {
            let title = line.trim_matches('=').trim();
            let level = line.chars().take_while(|&c| c == '=').count();
            if level == 2
                && SKIPPED_SECTIONS
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(title))
            {
                break;
            }
            if !title.is_empty() {
                out.push(Block {
                    heading: true,
                    text: title.to_string(),
                });
            }
            continue;
        }
        out.push(Block {
            heading: false,
            text: line.to_string(),
        });
    }
    // Drop headings left with nothing under them.
    let mut cleaned: Vec<Block> = Vec::with_capacity(out.len());
    for block in out {
        if block.heading && cleaned.last().is_some_and(|b| b.heading) {
            cleaned.pop();
        }
        cleaned.push(block);
    }
    if cleaned.last().is_some_and(|b| b.heading) {
        cleaned.pop();
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_results() {
        let body = br#"{"pages":[
            {"id":1,"key":"Rust_(programming_language)","title":"Rust (programming language)",
             "excerpt":"<span class=\"searchmatch\">Rust</span> is","description":"Programming language"},
            {"id":2,"title":"Rust","description":""}
        ]}"#;
        let hits = parse_search(body).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Rust (programming language)");
        assert_eq!(hits[0].description.as_deref(), Some("Programming language"));
        assert_eq!(hits[1].description, None, "empty description dropped");
    }

    #[test]
    fn parses_an_article_into_headings_and_paragraphs() {
        let body = br#"{"query":{"pages":[{"pageid":1,"title":"Owl","extract":
            "Owls are birds.\nThey hunt at night.\n\n\n== Anatomy ==\nBig eyes.\n\n=== Empty ===\n\n== Behaviour ==\nQuiet flight.\n\n== See also ==\nOther birds\n\n== References =="}]}}"#;
        let blocks = parse_article(body).unwrap();
        let texts: Vec<(bool, &str)> = blocks
            .iter()
            .map(|b| (b.heading, b.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                (false, "Owls are birds."),
                (false, "They hunt at night."),
                (true, "Anatomy"),
                (false, "Big eyes."),
                (true, "Behaviour"),
                (false, "Quiet flight."),
            ],
            "empty sub-section and reference sections dropped"
        );
    }

    #[test]
    fn missing_article_is_not_found() {
        let body = br#"{"query":{"pages":[{"title":"Nope","missing":true}]}}"#;
        assert_eq!(parse_article(body).unwrap_err(), UserError::NotFound);
    }

    #[test]
    fn language_setting_cannot_change_the_host() {
        assert_eq!(safe_lang("de"), "de");
        assert_eq!(safe_lang("zh-yue"), "zh-yue");
        assert_eq!(safe_lang("evil.com/x"), "en");
        assert_eq!(safe_lang(""), "en");
    }

    /// Real request; needs internet. `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_and_article() {
        let hits = search("en", "Raspberry Pi").unwrap();
        assert!(!hits.is_empty());
        let blocks = article("en", &hits[0].title).unwrap();
        assert!(blocks.len() > 3);
    }
}
