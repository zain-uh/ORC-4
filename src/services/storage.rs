//! Local file access for the Files app. Plain synchronous functions: screens
//! call them inside worker jobs (`Services::run`), never on the UI thread.
//!
//! Limits protect the Pi's 512 MB: images over 16 MP are refused before
//! decoding, text is read up to 512 KB, and only files or empty folders can
//! be deleted.

use std::fmt;
use std::fs;
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use super::UserError;

const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_IMAGE_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TEXT_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    Text,
    Image,
    /// Audio or video, played by mpv.
    Media,
    Other,
}

pub fn kind(entry: &DirEntry) -> FileKind {
    if entry.is_dir {
        return FileKind::Folder;
    }
    let ext = Path::new(&entry.name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("txt" | "md" | "log" | "csv" | "json" | "toml" | "ini" | "srt") => FileKind::Text,
        Some("png" | "jpg" | "jpeg") => FileKind::Image,
        Some(
            "mp4" | "mkv" | "webm" | "mov" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "flac"
            | "wav",
        ) => FileKind::Media,
        _ => FileKind::Other,
    }
}

/// Visible entries of `dir`: folders first, then files, each sorted by name
/// ignoring case. Hidden (dot) files are skipped.
pub fn list_dir(dir: &Path) -> Result<Vec<DirEntry>, UserError> {
    let mut entries = Vec::new();
    for item in fs::read_dir(dir).map_err(io_error)? {
        let item = item.map_err(io_error)?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = item.metadata() else {
            continue; // vanished or unreadable: skip rather than fail the folder
        };
        entries.push(DirEntry {
            name,
            path: item.path(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// File contents as text (invalid UTF-8 replaced), and whether it was cut
/// at `MAX_TEXT_BYTES`.
pub fn read_text(path: &Path) -> Result<(String, bool), UserError> {
    let file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    Ok((
        String::from_utf8_lossy(&bytes).into_owned(),
        size > MAX_TEXT_BYTES,
    ))
}

/// Deletes a file, or a folder only if it is empty.
pub fn delete(path: &Path) -> Result<(), UserError> {
    let meta = fs::metadata(path).map_err(io_error)?;
    if meta.is_dir() {
        fs::remove_dir(path).map_err(|e| {
            if fs::read_dir(path).is_ok_and(|mut d| d.next().is_some()) {
                UserError::NotEmpty
            } else {
                io_error(e)
            }
        })
    } else {
        fs::remove_file(path).map_err(io_error)
    }
}

/// Decoded image, premultiplied RGBA, scaled down to fit the requested box.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

pub fn load_image(path: &Path, max_w: u32, max_h: u32) -> Result<Image, UserError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err(UserError::Unsupported);
    }
    if fs::metadata(path).map_err(io_error)?.len() > MAX_IMAGE_FILE_BYTES {
        return Err(UserError::TooLarge);
    }
    decode_image(&fs::read(path).map_err(io_error)?, max_w, max_h)
}

/// Decodes PNG or JPEG bytes (detected from the content, not a file name),
/// scaled down to fit `max_w`×`max_h`.
pub fn decode_image(bytes: &[u8], max_w: u32, max_h: u32) -> Result<Image, UserError> {
    let (w, h, rgba) = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        decode_png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        decode_jpeg(bytes)?
    } else {
        return Err(UserError::Unsupported);
    };
    let (width, height, mut rgba) = fit(&rgba, w, h, max_w, max_h);
    premultiply(&mut rgba);
    Ok(Image {
        width,
        height,
        rgba,
    })
}

fn check_size(w: u32, h: u32) -> Result<(), UserError> {
    if w == 0 || h == 0 || w as u64 * h as u64 > MAX_IMAGE_PIXELS {
        return Err(UserError::TooLarge);
    }
    Ok(())
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), UserError> {
    let mut decoder = png::Decoder::new(io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| decode_error("png", e))?;
    let (w, h) = (reader.info().width, reader.info().height);
    check_size(w, h)?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or(UserError::TooLarge)?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| decode_error("png", e))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => expand(&buf, 3, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::GrayscaleAlpha => expand(&buf, 2, |p| [p[0], p[0], p[0], p[1]]),
        png::ColorType::Grayscale => expand(&buf, 1, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::Indexed => return Err(UserError::Unsupported), // expanded by normalize
    };
    Ok((w, h, rgba))
}

fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), UserError> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(io::Cursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|e| decode_error("jpeg", e))?;
    let info = decoder.info().ok_or(UserError::Unsupported)?;
    let (w, h) = (info.width as u32, info.height as u32);
    check_size(w, h)?;
    let pixels = decoder.decode().map_err(|e| decode_error("jpeg", e))?;
    let rgba = match decoder.output_colorspace() {
        Some(ColorSpace::RGBA) => pixels,
        Some(ColorSpace::RGB) => expand(&pixels, 3, |p| [p[0], p[1], p[2], 255]),
        Some(ColorSpace::Luma) => expand(&pixels, 1, |p| [p[0], p[0], p[0], 255]),
        other => {
            log::warn!("jpeg output colourspace {other:?} not handled");
            return Err(UserError::Unsupported);
        }
    };
    Ok((w, h, rgba))
}

fn expand(src: &[u8], channels: usize, f: impl Fn(&[u8]) -> [u8; 4]) -> Vec<u8> {
    src.chunks_exact(channels).flat_map(f).collect()
}

/// Area-average downscale so the image fits in `max_w`×`max_h`. Never
/// upscales: small images stay pixel-exact.
fn fit(src: &[u8], sw: u32, sh: u32, max_w: u32, max_h: u32) -> (u32, u32, Vec<u8>) {
    let scale = (max_w as f64 / sw as f64)
        .min(max_h as f64 / sh as f64)
        .min(1.0);
    if scale >= 1.0 {
        return (sw, sh, src.to_vec());
    }
    let dw = ((sw as f64 * scale).round() as u32).max(1);
    let dh = ((sh as f64 * scale).round() as u32).max(1);
    let (sw_, sh_) = (sw as u64, sh as u64);
    let mut out = Vec::with_capacity((dw * dh * 4) as usize);
    for dy in 0..dh as u64 {
        let y0 = dy * sh_ / dh as u64;
        let y1 = ((dy + 1) * sh_ / dh as u64).max(y0 + 1);
        for dx in 0..dw as u64 {
            let x0 = dx * sw_ / dw as u64;
            let x1 = ((dx + 1) * sw_ / dw as u64).max(x0 + 1);
            let mut sum = [0u64; 4];
            for y in y0..y1 {
                let row = (y * sw_) as usize * 4;
                for x in x0..x1 {
                    let i = row + x as usize * 4;
                    for c in 0..4 {
                        sum[c] += src[i + c] as u64;
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.map(|s| ((s + n / 2) / n) as u8));
        }
    }
    (dw, dh, out)
}

fn premultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
}

fn io_error(e: io::Error) -> UserError {
    match e.kind() {
        ErrorKind::NotFound => UserError::NotFound,
        _ => {
            log::warn!("storage: {e}");
            UserError::Storage
        }
    }
}

fn decode_error(format: &str, e: impl fmt::Display) -> UserError {
    log::warn!("{format} decode failed: {e}");
    UserError::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orc4-storage-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_folders_first_sorted_without_hidden_files() {
        let d = temp_dir("list");
        fs::write(d.join("b.txt"), "hi").unwrap();
        fs::write(d.join("A.txt"), "hello").unwrap();
        fs::write(d.join(".hidden"), "").unwrap();
        fs::create_dir(d.join("zeta")).unwrap();
        let names: Vec<_> = list_dir(&d).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["zeta", "A.txt", "b.txt"]);
    }

    #[test]
    fn missing_folder_is_not_found() {
        assert_eq!(
            list_dir(Path::new("/definitely/not/here")).unwrap_err(),
            UserError::NotFound
        );
    }

    #[test]
    fn delete_removes_files_and_only_empty_folders() {
        let d = temp_dir("delete");
        fs::write(d.join("f.txt"), "x").unwrap();
        delete(&d.join("f.txt")).unwrap();
        assert!(!d.join("f.txt").exists());

        fs::create_dir(d.join("full")).unwrap();
        fs::write(d.join("full/inside.txt"), "x").unwrap();
        assert_eq!(delete(&d.join("full")).unwrap_err(), UserError::NotEmpty);
        assert!(d.join("full/inside.txt").exists(), "nothing deleted");

        fs::create_dir(d.join("empty")).unwrap();
        delete(&d.join("empty")).unwrap();
        assert!(!d.join("empty").exists());
    }

    #[test]
    fn read_text_is_lossy_and_reports_truncation() {
        let d = temp_dir("text");
        fs::write(d.join("bad.txt"), b"ok \xFF end").unwrap();
        let (text, cut) = read_text(&d.join("bad.txt")).unwrap();
        assert_eq!(text, "ok \u{FFFD} end");
        assert!(!cut);

        fs::write(d.join("big.txt"), vec![b'a'; MAX_TEXT_BYTES as usize + 10]).unwrap();
        let (text, cut) = read_text(&d.join("big.txt")).unwrap();
        assert_eq!(text.len() as u64, MAX_TEXT_BYTES);
        assert!(cut);
    }

    #[test]
    fn fit_downscales_by_area_average_and_never_upscales() {
        // 4×2: left half black, right half white → 2×1 = [black, white].
        let mut src = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                src.extend([v, v, v, 255]);
            }
        }
        let (w, h, out) = fit(&src, 4, 2, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out, [0, 0, 0, 255, 255, 255, 255, 255]);

        let (w, h, _) = fit(&src, 4, 2, 100, 100);
        assert_eq!((w, h), (4, 2), "no upscaling");
    }

    #[test]
    fn loads_png_scaled_and_premultiplied() {
        let d = temp_dir("png");
        let path = d.join("half.png");
        {
            let file = fs::File::create(&path).unwrap();
            let mut enc = png::Encoder::new(io::BufWriter::new(file), 8, 4);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            // White at 50% alpha.
            enc.write_header()
                .unwrap()
                .write_image_data(&[255, 255, 255, 128].repeat(32))
                .unwrap();
        }
        let img = load_image(&path, 4, 4).unwrap();
        assert_eq!((img.width, img.height), (4, 2));
        assert_eq!(&img.rgba[..4], &[128, 128, 128, 128]);
    }

    #[test]
    fn rejects_unknown_formats_and_huge_images() {
        assert_eq!(
            load_image(Path::new("x.gif"), 10, 10).unwrap_err(),
            UserError::Unsupported
        );
        assert_eq!(check_size(5000, 5000).unwrap_err(), UserError::TooLarge);
        assert!(check_size(4000, 3000).is_ok());
    }
}
