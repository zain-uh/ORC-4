use tiny_skia::{
    Color, ColorU8, FillRule, FilterQuality, LineCap, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, PremultipliedColorU8, Rect, Stroke, Transform,
};

use super::text::{Align, Glyph, GlyphCache, TextStyle, fonts};
use super::theme::typography::LINE_HEIGHT;

/// Logical UI size. Everything is designed for this; display backends scale
/// it to the physical output (composite 720×480/576, or native 960×540).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;

/// The frame every screen draws into, plus its glyph cache.
pub struct Canvas {
    pixmap: Pixmap,
    glyphs: GlyphCache,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            pixmap: Pixmap::new(WIDTH, HEIGHT).expect("non-zero canvas size"),
            glyphs: GlyphCache::default(),
        }
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(color);
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            self.pixmap
                .fill_rect(rect, &paint(color), Transform::identity(), None);
        }
    }

    /// Rectangle outline drawn as four filled bars, so it stays crisp.
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, color: Color) {
        self.fill_rect(x, y, w, width, color);
        self.fill_rect(x, y + h - width, w, width, color);
        self.fill_rect(x, y, width, h, color);
        self.fill_rect(x + w - width, y, width, h, color);
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
            self.fill_path(&path, color);
        }
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, width: f32, color: Color) {
        if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
            self.stroke_path(&path, width, LineCap::Butt, color);
        }
    }

    /// Straight line with rounded ends.
    pub fn stroke_line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: Color) {
        let mut pb = PathBuilder::new();
        pb.move_to(from.0, from.1);
        pb.line_to(to.0, to.1);
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, width, LineCap::Round, color);
        }
    }

    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: Color) {
        let mut pb = PathBuilder::new();
        for (i, &(x, y)) in points.iter().enumerate() {
            if i == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
        pb.close();
        if let Some(path) = pb.finish() {
            self.fill_path(&path, color);
        }
    }

    /// Draws a (premultiplied RGBA) image with its top-left at (x, y).
    pub fn draw_image(&mut self, image: &Pixmap, x: f32, y: f32) {
        self.pixmap.draw_pixmap(
            x.round() as i32,
            y.round() as i32,
            image.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    /// Draws an image stretched to `w`×`h` at (x, y), with smoothing.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_scaled(
        &mut self,
        image: &Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        opacity: f32,
    ) {
        let (sx, sy) = (w / image.width() as f32, h / image.height() as f32);
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            image.as_ref(),
            &paint,
            Transform::from_row(sx, 0.0, 0.0, sy, x, y),
            None,
        );
    }

    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
        if let Some(path) = rounded_rect(x, y, w, h, r) {
            self.fill_path(&path, color);
        }
    }

    /// Outline centred on the rectangle's edge.
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        width: f32,
        color: Color,
    ) {
        if let Some(path) = rounded_rect(x, y, w, h, r) {
            self.stroke_path(&path, width, LineCap::Butt, color);
        }
    }

    fn stroke_path(&mut self, path: &Path, width: f32, line_cap: LineCap, color: Color) {
        let stroke = Stroke {
            width,
            line_cap,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &paint(color), &stroke, Transform::identity(), None);
    }

    pub fn fill_path(&mut self, path: &Path, color: Color) {
        self.pixmap.fill_path(
            path,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Draws one line of text. `x` is the left edge, centre or right edge
    /// depending on `align`; `y` is the top of the line box. Returns the width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        style: TextStyle,
        text: &str,
        x: f32,
        y: f32,
        align: Align,
        color: Color,
    ) -> f32 {
        let f = fonts();
        let width = f.measure(style, text);
        let left = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        }
        .round();
        let baseline = (y + f.line_metrics(style).ascent).round() as i32;

        let mut placed = Vec::with_capacity(text.len());
        f.layout(style, text, |ch, pen| placed.push((ch, pen)));

        let color = color.to_color_u8();
        for (ch, pen) in placed {
            let glyph = self.glyphs.get(style, ch);
            let gx = (left + pen).round() as i32 + glyph.metrics.xmin;
            let gy = baseline - glyph.metrics.height as i32 - glyph.metrics.ymin;
            blend_glyph(&mut self.pixmap, glyph, gx, gy, color);
        }
        width
    }

    /// Word-wrapped text in a box `max_width` wide starting at `x`.
    /// Returns the height used.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(
        &mut self,
        style: TextStyle,
        text: &str,
        x: f32,
        y: f32,
        max_width: f32,
        max_lines: usize,
        align: Align,
        color: Color,
    ) -> f32 {
        let anchor = match align {
            Align::Left => x,
            Align::Center => x + max_width / 2.0,
            Align::Right => x + max_width,
        };
        let line_height = (style.size * LINE_HEIGHT).round();
        let lines = fonts().wrap(style, text, max_width, max_lines);
        for (i, line) in lines.iter().enumerate() {
            self.text(
                style,
                line,
                anchor,
                y + i as f32 * line_height,
                align,
                color,
            );
        }
        lines.len() as f32 * line_height
    }
}

/// `0xRRGGBB` → opaque colour.
pub fn rgb(hex: u32) -> Color {
    rgba(hex, 1.0)
}

/// `0xRRGGBB` with opacity 0.0–1.0.
pub fn rgba(hex: u32, alpha: f32) -> Color {
    let ch = |shift: u32| ((hex >> shift) & 0xFF) as f32 / 255.0;
    Color::from_rgba(ch(16), ch(8), ch(0), alpha.clamp(0.0, 1.0)).expect("components in 0..=1")
}

pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_8 * r; // cubic approximation of a quarter circle
    let (x2, y2) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x2 - r, y);
    pb.cubic_to(x2 - r + k, y, x2, y + r - k, x2, y + r);
    pb.line_to(x2, y2 - r);
    pb.cubic_to(x2, y2 - r + k, x2 - r + k, y2, x2 - r, y2);
    pb.line_to(x + r, y2);
    pb.cubic_to(x + r - k, y2, x, y2 - r + k, x, y2 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

/// Source-over blend of a glyph's coverage mask in a solid colour.
fn blend_glyph(pixmap: &mut Pixmap, glyph: &Glyph, x0: i32, y0: i32, color: ColorU8) {
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let gw = glyph.metrics.width as i32;
    let pixels = pixmap.pixels_mut();

    for gy in 0..glyph.metrics.height as i32 {
        let py = y0 + gy;
        if !(0..h).contains(&py) {
            continue;
        }
        for gx in 0..gw {
            let px = x0 + gx;
            if !(0..w).contains(&px) {
                continue;
            }
            let coverage = glyph.coverage[(gy * gw + gx) as usize] as u32;
            if coverage == 0 {
                continue;
            }
            let a = coverage * color.alpha() as u32 / 255;
            let inv = 255 - a;
            let dst = &mut pixels[(py * w + px) as usize];
            let mix = |s: u8, d: u8| ((s as u32 * a + d as u32 * inv + 127) / 255) as u8;
            let out_a = (a + dst.alpha() as u32 * inv / 255) as u8;
            if let Some(out) = PremultipliedColorU8::from_rgba(
                mix(color.red(), dst.red()),
                mix(color.green(), dst.green()),
                mix(color.blue(), dst.blue()),
                out_a,
            ) {
                *dst = out;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text::FontId;

    fn rgb_at(c: &Canvas, x: u32, y: u32) -> (u8, u8, u8) {
        let p = c.pixmap().pixel(x, y).unwrap();
        (p.red(), p.green(), p.blue())
    }

    #[test]
    fn text_draws_inside_its_box_only() {
        let mut c = Canvas::new();
        c.clear(rgb(0x000000));
        let style = TextStyle::new(FontId::Sans, 44.0);
        let w = c.text(style, "HHHH", 100.0, 100.0, Align::Left, rgb(0xFFFFFF));
        let lm = fonts().line_metrics(style);

        let mut lit_inside = 0;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if rgb_at(&c, x, y) != (0, 0, 0) {
                    let inside = (x as f32) >= 99.0
                        && (x as f32) <= 100.0 + w + 1.0
                        && (y as f32) >= 99.0
                        && (y as f32) <= 100.0 + lm.height + 1.0;
                    assert!(inside, "stray text pixel at {x},{y}");
                    lit_inside += 1;
                }
            }
        }
        assert!(lit_inside > 500, "text actually drew something");
    }

    #[test]
    fn centred_text_is_centred() {
        let mut c = Canvas::new();
        c.clear(rgb(0x000000));
        let style = TextStyle::new(FontId::Sans, 44.0);
        c.text(style, "OOOO", 480.0, 200.0, Align::Center, rgb(0xFFFFFF));
        let lit: Vec<u32> = (0..WIDTH)
            .filter(|&x| (200..260).any(|y| rgb_at(&c, x, y) != (0, 0, 0)))
            .collect();
        let mid = (lit[0] + lit[lit.len() - 1]) as f32 / 2.0;
        assert!((mid - 480.0).abs() <= 3.0, "ink centred at {mid}");
    }
}
