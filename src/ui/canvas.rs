use tiny_skia::{Color, FillRule, Paint, Path, Pixmap, Rect, Transform};

/// Logical UI size. Everything is designed for this; display backends scale
/// it to the physical output (composite 720×480/576, or native 960×540).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;

/// The frame every screen draws into.
pub struct Canvas {
    pixmap: Pixmap,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            pixmap: Pixmap::new(WIDTH, HEIGHT).expect("non-zero canvas size"),
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

    pub fn fill_path(&mut self, path: &Path, color: Color) {
        self.pixmap.fill_path(
            path,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// `0xRRGGBB` → opaque colour.
pub fn rgb(hex: u32) -> Color {
    Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF)
}

fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}
