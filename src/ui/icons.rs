//! App icons drawn from simple shapes rather than bitmaps: sharp at any size
//! and any colour, with no asset files to ship. Strokes scale with `size`
//! and stay well above the 2 px composite minimum at launcher sizes.

use std::f32::consts::{FRAC_1_SQRT_2, TAU};

use tiny_skia::Color;

use super::canvas::Canvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Search,
    Folder,
    Gear,
    Document,
    Picture,
}

/// Draws `icon` centred on (`cx`, `cy`), fitting a `size`×`size` box.
pub fn draw(c: &mut Canvas, icon: Icon, cx: f32, cy: f32, size: f32, color: Color) {
    let s = size;
    match icon {
        Icon::Play => {
            // Optically centred: the triangle's mass sits right of its box centre.
            c.fill_polygon(
                &[
                    (cx - 0.28 * s, cy - 0.38 * s),
                    (cx - 0.28 * s, cy + 0.38 * s),
                    (cx + 0.40 * s, cy),
                ],
                color,
            );
        }
        Icon::Search => {
            let (rx, ry, r) = (cx - 0.10 * s, cy - 0.10 * s, 0.27 * s);
            let w = 0.09 * s;
            c.stroke_circle(rx, ry, r, w, color);
            let edge = (r + w / 2.0) * FRAC_1_SQRT_2;
            c.stroke_line(
                (rx + edge, ry + edge),
                (cx + 0.38 * s, cy + 0.38 * s),
                0.11 * s,
                color,
            );
        }
        Icon::Folder => {
            let r = 0.06 * s;
            c.fill_rounded_rect(cx - 0.42 * s, cy - 0.34 * s, 0.38 * s, 0.2 * s, r, color);
            c.fill_rounded_rect(cx - 0.42 * s, cy - 0.24 * s, 0.84 * s, 0.6 * s, r, color);
        }
        Icon::Document => {
            // Page with the top-right corner folded over.
            let (l, r, t, b, fold) = (
                cx - 0.3 * s,
                cx + 0.3 * s,
                cy - 0.4 * s,
                cy + 0.4 * s,
                0.2 * s,
            );
            c.fill_polygon(
                &[(l, t), (r - fold, t), (r, t + fold), (r, b), (l, b)],
                color,
            );
        }
        Icon::Picture => {
            // Frame outline, a sun and two hills.
            let w = 0.08 * s;
            let (l, t, fw, fh) = (cx - 0.42 * s, cy - 0.32 * s, 0.84 * s, 0.64 * s);
            c.stroke_rounded_rect(l, t, fw, fh, 0.08 * s, w, color);
            c.fill_circle(cx + 0.18 * s, cy - 0.1 * s, 0.08 * s, color);
            let base = t + fh - w / 2.0;
            c.fill_polygon(
                &[
                    (l + w / 2.0, base),
                    (cx - 0.12 * s, cy - 0.06 * s),
                    (cx + 0.1 * s, base - 0.12 * s),
                    (cx + 0.22 * s, cy + 0.04 * s),
                    (l + fw - w / 2.0, base),
                ],
                color,
            );
        }
        Icon::Gear => {
            // Thick ring (leaves the centre hole transparent) plus 8 teeth.
            c.stroke_circle(cx, cy, 0.22 * s, 0.16 * s, color);
            let (inner, outer, half) = (0.26 * s, 0.42 * s, 0.075 * s);
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let (dx, dy) = (a.cos(), a.sin());
                let (px, py) = (-dy * half, dx * half);
                c.fill_polygon(
                    &[
                        (cx + dx * inner + px, cy + dy * inner + py),
                        (cx + dx * outer + px, cy + dy * outer + py),
                        (cx + dx * outer - px, cy + dy * outer - py),
                        (cx + dx * inner - px, cy + dy * inner - py),
                    ],
                    color,
                );
            }
        }
    }
}
