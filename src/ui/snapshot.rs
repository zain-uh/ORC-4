//! Snapshot tests: render a screen headlessly and compare it with a golden
//! PNG in `tests/snapshots/`. Every run also writes the current render to
//! `target/snapshots/` for inspection.
//!
//! A missing golden is created on first run. To accept an intended visual
//! change, run with `ORC4_UPDATE_SNAPSHOTS=1`.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use super::canvas::Canvas;

/// Pixels may differ by this much per channel (rasteriser rounding).
const CHANNEL_TOLERANCE: u8 = 8;
/// Fraction of pixels allowed beyond the tolerance.
const PIXEL_TOLERANCE: f64 = 0.0001; // ~50 px: catches a changed glyph

pub fn assert_snapshot(name: &str, canvas: &Canvas) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pixmap = canvas.pixmap();
    let (w, h) = (pixmap.width(), pixmap.height());
    let actual = pixmap.data(); // RGBA, opaque so premultiplied == straight

    let out = root.join("target/snapshots").join(format!("{name}.png"));
    write_png(&out, w, h, actual);

    let golden = root.join("tests/snapshots").join(format!("{name}.png"));
    let update = std::env::var_os("ORC4_UPDATE_SNAPSHOTS").is_some();
    if update || !golden.exists() {
        write_png(&golden, w, h, actual);
        eprintln!("snapshot {name}: wrote golden {}", golden.display());
        return;
    }

    let (gw, gh, expected) = read_png(&golden);
    assert_eq!((gw, gh), (w, h), "snapshot {name}: size changed");
    let differing = actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .filter(|(a, e)| {
            a.iter()
                .zip(e.iter())
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
        })
        .count();
    let fraction = differing as f64 / (w * h) as f64;
    assert!(
        fraction <= PIXEL_TOLERANCE,
        "snapshot {name}: {differing} pixels differ ({:.2}%). Compare {} with {}; \
         if the change is intended, rerun with ORC4_UPDATE_SNAPSHOTS=1",
        fraction * 100.0,
        out.display(),
        golden.display()
    );
}

pub fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn read_png(path: &PathBuf) -> (u32, u32, Vec<u8>) {
    let bytes = fs::read(path).unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "golden must be RGBA");
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}
