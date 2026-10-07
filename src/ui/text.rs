//! Text: embedded fonts, measurement, ellipsis and word wrap.
//!
//! Font data is parsed once into a process-wide `Fonts`. Rasterised glyphs
//! are cached per `Canvas` (see `GlyphCache`). Layout is simple
//! left-to-right with kerning, which covers Latin, Greek and Cyrillic UI text.
//! Scripts needing shaping (Arabic, Indic) or CJK fonts are not supported yet.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use fontdue::{Font, FontSettings, Metrics};

const SANS_TTF: &[u8] = include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf");
const MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/IBMPlexMono-SemiBold.ttf");

const ELLIPSIS: char = '…';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontId {
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font: FontId,
    pub size: f32,
}

impl TextStyle {
    pub const fn new(font: FontId, size: f32) -> Self {
        Self { font, size }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Vertical metrics for one line, in pixels, rounded so text sits on whole
/// pixel rows.
#[derive(Debug, Clone, Copy)]
pub struct LineMetrics {
    /// Distance from the top of the line box to the baseline.
    pub ascent: f32,
    /// Height of the line box (ascent + descent).
    pub height: f32,
}

pub struct Fonts {
    sans: Font,
    mono: Font,
}

/// The embedded fonts, parsed on first use.
pub fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let parse = |bytes: &'static [u8]| {
            Font::from_bytes(bytes, FontSettings::default()).expect("embedded font is valid")
        };
        Fonts {
            sans: parse(SANS_TTF),
            mono: parse(MONO_TTF),
        }
    })
}

impl Fonts {
    pub fn font(&self, id: FontId) -> &Font {
        match id {
            FontId::Sans => &self.sans,
            FontId::Mono => &self.mono,
        }
    }

    pub fn line_metrics(&self, style: TextStyle) -> LineMetrics {
        let m = self
            .font(style.font)
            .horizontal_line_metrics(style.size)
            .expect("horizontal font");
        LineMetrics {
            ascent: m.ascent.round(),
            height: (m.ascent - m.descent).round(),
        }
    }

    /// Calls `f(ch, pen_x)` for every glyph, in layout order. The single
    /// source of truth for horizontal positions, shared by measuring and
    /// drawing so they always agree. Returns the total advance.
    pub fn layout(&self, style: TextStyle, text: &str, mut f: impl FnMut(char, f32)) -> f32 {
        let font = self.font(style.font);
        let mut pen = 0.0;
        let mut prev = None;
        for ch in text.chars() {
            if let Some(p) = prev {
                pen += font.horizontal_kern(p, ch, style.size).unwrap_or(0.0);
            }
            f(ch, pen);
            pen += font.metrics(ch, style.size).advance_width;
            prev = Some(ch);
        }
        pen
    }

    pub fn measure(&self, style: TextStyle, text: &str) -> f32 {
        self.layout(style, text, |_, _| {})
    }

    /// `text` if it fits in `max_width`, else the longest prefix that fits
    /// with "…" appended.
    pub fn ellipsize<'a>(&self, style: TextStyle, text: &'a str, max_width: f32) -> Cow<'a, str> {
        if self.measure(style, text) <= max_width {
            return Cow::Borrowed(text);
        }
        let budget = max_width - self.measure(style, "…");
        let mut cut = self.fit_prefix(style, text, budget);
        // End on a whole word when that keeps most of the visible text;
        // "two pi…" reads worse than "two…".
        let mid_word = text[cut..].starts_with(|c: char| !c.is_whitespace());
        if mid_word
            && let Some(space) = text[..cut].rfind(char::is_whitespace)
            && space * 10 >= cut * 6
        {
            cut = space;
        }
        Cow::Owned(format!("{}{ELLIPSIS}", text[..cut].trim_end()))
    }

    /// Byte length of the longest prefix of `text` that fits in `max_width`.
    /// One pass, same arithmetic as `layout`.
    fn fit_prefix(&self, style: TextStyle, text: &str, max_width: f32) -> usize {
        let font = self.font(style.font);
        let mut pen = 0.0;
        let mut prev = None;
        for (i, ch) in text.char_indices() {
            if let Some(p) = prev {
                pen += font.horizontal_kern(p, ch, style.size).unwrap_or(0.0);
            }
            let end = pen + font.metrics(ch, style.size).advance_width;
            if end > max_width {
                return i;
            }
            pen = end;
            prev = Some(ch);
        }
        text.len()
    }

    /// Greedy word wrap into at most `max_lines` lines. If the text does not
    /// fit, the last line ends with "…". Words longer than a line are split.
    pub fn wrap(
        &self,
        style: TextStyle,
        text: &str,
        max_width: f32,
        max_lines: usize,
    ) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();

        for word in text.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if self.measure(style, &candidate) <= max_width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            // Split a word that is wider than a whole line.
            let mut rest = word;
            while self.measure(style, rest) > max_width {
                let cut = self.fit_prefix(style, rest, max_width);
                if cut == 0 {
                    break; // a single glyph wider than the line: give up splitting
                }
                lines.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            current = rest.to_string();
        }
        if !current.is_empty() {
            lines.push(current);
        }

        if lines.len() > max_lines {
            let overflow = lines.split_off(max_lines - 1).join(" ");
            lines.push(self.ellipsize(style, &overflow, max_width).into_owned());
        }
        lines
    }
}

/// A rasterised glyph: coverage bitmap plus placement metrics.
pub struct Glyph {
    pub metrics: Metrics,
    pub coverage: Vec<u8>,
}

/// Rasterised glyphs keyed by font, character and size. Bounded: cleared
/// when full, which only costs re-rasterising what is on screen.
#[derive(Default)]
pub struct GlyphCache {
    glyphs: HashMap<(FontId, char, u32), Glyph>,
}

impl GlyphCache {
    const CAPACITY: usize = 2048;

    pub fn get(&mut self, style: TextStyle, ch: char) -> &Glyph {
        // Quarter-pixel size buckets keep keys stable for float sizes.
        let key = (style.font, ch, (style.size * 4.0).round() as u32);
        if self.glyphs.len() >= Self::CAPACITY && !self.glyphs.contains_key(&key) {
            self.glyphs.clear();
        }
        self.glyphs.entry(key).or_insert_with(|| {
            let (metrics, coverage) = fonts().font(style.font).rasterize(ch, style.size);
            Glyph { metrics, coverage }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: TextStyle = TextStyle::new(FontId::Sans, 32.0);

    #[test]
    fn measure_grows_with_text() {
        let f = fonts();
        assert_eq!(f.measure(BODY, ""), 0.0);
        assert!(f.measure(BODY, "ORC-4 glasses") > f.measure(BODY, "ORC-4"));
    }

    #[test]
    fn mono_digits_have_equal_width() {
        let f = fonts();
        let mono = TextStyle::new(FontId::Mono, 26.0);
        assert_eq!(f.measure(mono, "11:11"), f.measure(mono, "88:88"));
    }

    #[test]
    fn ellipsize_keeps_short_text_and_trims_long_text() {
        let f = fonts();
        assert_eq!(f.ellipsize(BODY, "Files", 400.0), "Files");
        let long = "A very long video title that cannot possibly fit";
        let cut = f.ellipsize(BODY, long, 300.0);
        assert!(cut.ends_with('…'));
        assert!(f.measure(BODY, &cut) <= 300.0);
    }

    #[test]
    fn ellipsize_prefers_word_boundaries() {
        let f = fonts();
        let text = "nothing is thinner than two pixels";
        // Room for "…than two pi…" but not "pixels".
        let width = f.measure(BODY, "nothing is thinner than two pi…");
        let cut = f.ellipsize(BODY, text, width);
        assert_eq!(cut, "nothing is thinner than two…");

        // A single long word still gets cut mid-word.
        let word = f.ellipsize(BODY, "Supercalifragilistic", 150.0);
        assert!(word.ends_with('…') && word.len() > "…".len());
    }

    #[test]
    fn wrap_respects_width_and_line_limit() {
        let f = fonts();
        let text = "Large bold text in white on black stays readable over composite video";
        let lines = f.wrap(BODY, text, 420.0, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| f.measure(BODY, l) <= 420.0));
        assert!(lines[1].ends_with('…'));

        let all = f.wrap(BODY, text, 420.0, 10);
        assert_eq!(all.join(" "), text, "nothing lost when lines suffice");
    }

    #[test]
    fn wrap_splits_words_longer_than_a_line() {
        let f = fonts();
        let lines = f.wrap(BODY, "Supercalifragilisticexpialidocious", 150.0, 10);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| f.measure(BODY, l) <= 150.0));
        assert_eq!(lines.concat(), "Supercalifragilisticexpialidocious");
    }
}
