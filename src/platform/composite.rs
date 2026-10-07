//! Simulator-only approximation of how the AV driver board shows a frame,
//! so readability can be judged on the PC (toggle with F2).
//!
//! Composite video (NTSC/PAL) limits the picture in three ways:
//! - luma bandwidth ≈ 330–440 resolvable columns → soft horizontal detail
//! - chroma bandwidth ≈ 100–130 colour columns → colour smears sideways
//! - interlacing → fine horizontal lines soften vertically
//!
//! This is a visual guide, not an exact signal model.

/// Horizontal luma low-pass (binomial, σ ≈ 1.2 px at 960 px wide).
const LUMA_KERNEL: [f32; 7] = [
    1.0 / 64.0,
    6.0 / 64.0,
    15.0 / 64.0,
    20.0 / 64.0,
    15.0 / 64.0,
    6.0 / 64.0,
    1.0 / 64.0,
];
/// Horizontal chroma box filter width (~107 colour columns across 960 px).
const CHROMA_WIDTH: usize = 9;

/// Reusable buffers so a frame doesn't allocate.
#[derive(Default)]
pub struct CompositePreview {
    y: Vec<f32>,
    i: Vec<f32>,
    q: Vec<f32>,
    tmp: Vec<f32>,
}

impl CompositePreview {
    /// Filters a `0x00RRGGBB` frame in place.
    pub fn apply(&mut self, frame: &mut [u32], w: usize, h: usize) {
        let n = w * h;
        for v in [&mut self.y, &mut self.i, &mut self.q, &mut self.tmp] {
            v.resize(n, 0.0);
        }

        for (k, &px) in frame.iter().enumerate() {
            let (r, g, b) = (
                ((px >> 16) & 0xFF) as f32,
                ((px >> 8) & 0xFF) as f32,
                (px & 0xFF) as f32,
            );
            self.y[k] = 0.299 * r + 0.587 * g + 0.114 * b;
            self.i[k] = 0.596 * r - 0.274 * g - 0.322 * b;
            self.q[k] = 0.211 * r - 0.523 * g + 0.312 * b;
        }

        convolve_rows(&mut self.y, &mut self.tmp, w, h, &LUMA_KERNEL);
        let chroma = [1.0 / CHROMA_WIDTH as f32; CHROMA_WIDTH];
        convolve_rows(&mut self.i, &mut self.tmp, w, h, &chroma);
        convolve_rows(&mut self.q, &mut self.tmp, w, h, &chroma);
        soften_columns(&mut self.y, &mut self.tmp, w, h);

        for (k, px) in frame.iter_mut().enumerate() {
            let (y, i, q) = (self.y[k], self.i[k], self.q[k]);
            let to_u8 = |v: f32| v.round().clamp(0.0, 255.0) as u32;
            let r = to_u8(y + 0.956 * i + 0.621 * q);
            let g = to_u8(y - 0.272 * i - 0.647 * q);
            let b = to_u8(y - 1.106 * i + 1.703 * q);
            *px = r << 16 | g << 8 | b;
        }
    }
}

/// Horizontal convolution with edge clamping, result written back to `data`.
fn convolve_rows(data: &mut [f32], tmp: &mut [f32], w: usize, h: usize, kernel: &[f32]) {
    let half = kernel.len() / 2;
    for row in 0..h {
        let line = &data[row * w..(row + 1) * w];
        let out = &mut tmp[row * w..(row + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            *o = kernel
                .iter()
                .enumerate()
                .map(|(k, weight)| {
                    let sx = (x + k).saturating_sub(half).min(w - 1);
                    line[sx] * weight
                })
                .sum();
        }
    }
    data.copy_from_slice(tmp);
}

/// Vertical [1 2 1]/4 blend: the softening interlace gives thin horizontal edges.
fn soften_columns(data: &mut [f32], tmp: &mut [f32], w: usize, h: usize) {
    for row in 0..h {
        let (up, down) = (row.saturating_sub(1), (row + 1).min(h - 1));
        for x in 0..w {
            tmp[row * w + x] =
                (data[up * w + x] + 2.0 * data[row * w + x] + data[down * w + x]) / 4.0;
        }
    }
    data.copy_from_slice(tmp);
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 64;
    const H: usize = 16;

    fn red(px: u32) -> u32 {
        (px >> 16) & 0xFF
    }

    #[test]
    fn flat_colour_is_unchanged() {
        let mut frame = vec![0x3366CC; W * H];
        CompositePreview::default().apply(&mut frame, W, H);
        for px in frame {
            for shift in [16, 8, 0] {
                let (got, want) = ((px >> shift) & 0xFF, (0x3366CC >> shift) & 0xFF);
                assert!(got.abs_diff(want) <= 2, "{px:06x}");
            }
        }
    }

    #[test]
    fn thin_vertical_line_is_softened_and_spread() {
        let mut frame = vec![0x000000; W * H];
        for row in 0..H {
            frame[row * W + 32] = 0xFFFFFF;
        }
        CompositePreview::default().apply(&mut frame, W, H);
        let mid = 8 * W;
        assert!(red(frame[mid + 32]) < 200, "peak reduced");
        assert!(
            red(frame[mid + 31]) > 20 && red(frame[mid + 33]) > 20,
            "spread sideways"
        );
    }
}

/// Not a pass/fail check: writes the launcher as the composite display would
/// roughly show it, to `target/snapshots/launcher-composite.png`, for review.
#[cfg(test)]
mod review {
    use std::path::PathBuf;
    use std::time::Instant;

    use super::CompositePreview;
    use crate::screens::launcher::Launcher;
    use crate::screens::nav::Screen;
    use crate::ui::DrawCx;
    use crate::ui::canvas::{Canvas, HEIGHT, WIDTH};
    use crate::ui::snapshot::write_png;
    use crate::ui::theme::Theme;
    use crate::ui::widgets::status_bar::Status;

    #[test]
    fn render_launcher_through_composite_preview() {
        let mut canvas = Canvas::new();
        let theme = Theme::default();
        let status = Status {
            clock: Some((14, 32)),
        };
        let mut launcher = Launcher::new();
        launcher.animate(Instant::now());
        launcher.draw(
            &mut canvas,
            &DrawCx {
                theme: &theme,
                status: &status,
            },
        );

        let (w, h) = (WIDTH as usize, HEIGHT as usize);
        let mut frame: Vec<u32> = canvas
            .pixmap()
            .pixels()
            .iter()
            .map(|p| (p.red() as u32) << 16 | (p.green() as u32) << 8 | p.blue() as u32)
            .collect();
        CompositePreview::default().apply(&mut frame, w, h);

        let rgba: Vec<u8> = frame
            .iter()
            .flat_map(|px| [(px >> 16) as u8, (px >> 8) as u8, *px as u8, 0xFF])
            .collect();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/snapshots/launcher-composite.png");
        write_png(&out, WIDTH, HEIGHT, &rgba);
    }
}
