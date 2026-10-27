//! Thumbnail and preview generation, and their on-disk caches.
//!
//! Thumbnails are JPEGs (long edge `THUMB_SIZE`) in `<catalog>/thumbs/<xx>/<file id>.jpg`,
//! sharded by the low byte of the id so no folder gets huge. Large previews for formats
//! the preview panel cannot show directly (HEIC, PSD, PDF…) are made on demand and
//! cached the same way under `<catalog>/previews`.

use crate::tools;
use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbImage};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const THUMB_SIZE: u32 = 384;
pub const PREVIEW_SIZE: u32 = 1600;
const JPEG_QUALITY: u8 = 80;

/// Formats rendered with ImageMagick: HEIC needs libheif (ffmpeg picks the portrait depth
/// mask instead of the photo), vector and layered formats need a renderer.
pub const MAGICK_EXTS: &[&str] = &[
    "heic", "heif", "avif", "psd", "psb", "svg", "tga", "exr", "hdr", "jxl", "ai", "eps", "pdf",
];
/// Vector formats, rasterised at a density matching the output size.
const VECTOR_EXTS: &[&str] = &["svg", "ai", "eps", "pdf"];

fn cache_path(catalog_dir: &Path, folder: &str, file_id: i64) -> PathBuf {
    catalog_dir
        .join(folder)
        .join(format!("{:02x}", file_id & 0xff))
        .join(format!("{file_id}.jpg"))
}

pub fn thumb_path(catalog_dir: &Path, file_id: i64) -> PathBuf {
    cache_path(catalog_dir, "thumbs", file_id)
}

pub fn preview_path(catalog_dir: &Path, file_id: i64) -> PathBuf {
    cache_path(catalog_dir, "previews", file_id)
}

pub fn save(catalog_dir: &Path, file_id: i64, jpeg: &[u8]) -> Result<()> {
    write_cached(&thumb_path(catalog_dir, file_id), jpeg)
}

pub fn write_cached(path: &Path, jpeg: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, jpeg).with_context(|| format!("cannot write {}", path.display()))
}

/// Decodes an image in memory and returns a JPEG (long edge at most `size`), rotated upright.
/// Fails with an unsupported-format error for formats the decoder lacks (HEIC, AVIF…).
pub fn from_image_bytes(bytes: &[u8], orientation: Option<u32>, size: u32) -> Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)?;
    encode_jpeg(&orient(shrink(image, size), orientation.unwrap_or(1)))
}

fn shrink(image: DynamicImage, size: u32) -> DynamicImage {
    if image.width() > size || image.height() > size {
        // A cheap pre-shrink keeps the quality filter fast on 50-megapixel photos.
        let pre = image.thumbnail(size * 2, size * 2);
        pre.resize(size, size, FilterType::Triangle)
    } else {
        image
    }
}

/// Renders the first page / layer / frame with ImageMagick, already rotated upright.
pub fn from_magick(path: &Path, ext: &str, size: u32) -> Result<Vec<u8>> {
    let mut cmd = tools::command("magick");
    cmd.args(["-limit", "memory", "1GiB", "-limit", "map", "2GiB"]);
    if VECTOR_EXTS.contains(&ext) {
        // 72 dpi is "actual size"; render big enough for the requested output.
        let density = if size > THUMB_SIZE { "150" } else { "96" };
        cmd.args(["-density", density, "-background", "none"]);
    }
    let mut input = path.as_os_str().to_owned();
    input.push("[0]");
    cmd.arg(input).args([
        "-auto-orient",
        "-thumbnail",
        &format!("{size}x{size}>"),
        "-background",
        "#808080",
        "-flatten",
        "-quality",
        "82",
        "jpg:-",
    ]);
    let jpeg = tools::run(cmd, Duration::from_secs(90)).context("ImageMagick failed")?;
    anyhow::ensure!(!jpeg.is_empty(), "ImageMagick produced no image");
    Ok(jpeg)
}

/// Figma files saved from the app are zip archives with a ready-made `thumbnail.png`.
pub fn from_fig(path: &Path, size: u32) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).context("not a zipped Figma file (no thumbnail inside)")?;
    let mut png = Vec::new();
    archive
        .by_name("thumbnail.png")
        .context("this Figma file has no thumbnail")?
        .read_to_end(&mut png)?;
    from_image_bytes(&png, None, size)
}

/// Extracts one frame with ffmpeg (videos, and images the built-in decoder cannot read).
/// ffmpeg applies video rotation itself.
pub fn from_ffmpeg(path: &Path, seek_seconds: Option<f64>) -> Result<Vec<u8>> {
    let mut cmd = tools::command("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    if let Some(seek) = seek_seconds {
        cmd.args(["-ss", &format!("{seek:.2}")]);
    }
    cmd.arg("-i").arg(path).args([
        "-frames:v",
        "1",
        "-vf",
        &format!("scale={THUMB_SIZE}:{THUMB_SIZE}:force_original_aspect_ratio=decrease"),
        "-f",
        "image2pipe",
        "-c:v",
        "mjpeg",
        "-q:v",
        "5",
        "-",
    ]);
    let jpeg = tools::run(cmd, Duration::from_secs(60)).context("ffmpeg failed")?;
    anyhow::ensure!(!jpeg.is_empty(), "ffmpeg produced no frame");
    Ok(jpeg)
}

/// 64-bit difference hash of an image: shrink to 9×8 grey pixels and record, for each
/// pixel pair in a row, whether brightness goes up. Resizing, recompressing and small
/// edits barely change it, so similar images have hashes a few bits apart.
/// None for (nearly) flat images, whose hash says nothing about their content.
pub fn dhash(jpeg: &[u8]) -> Option<i64> {
    let small = image::load_from_memory(jpeg).ok()?.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash: u64 = 0;
    for y in 0..8 {
        for x in 0..8 {
            let brighter = small.get_pixel(x + 1, y).0[0] > small.get_pixel(x, y).0[0];
            hash = (hash << 1) | brighter as u64;
        }
    }
    let ones = hash.count_ones();
    (ones > 2 && ones < 62).then_some(hash as i64)
}

/// Rotates/flips per EXIF orientation 1–8 so the thumbnail displays upright.
fn orient(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

/// Encodes as JPEG, flattening any transparency onto a neutral grey.
fn encode_jpeg(image: &DynamicImage) -> Result<Vec<u8>> {
    let rgb: RgbImage = if image.color().has_alpha() {
        let rgba = image.to_rgba8();
        RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let p = rgba.get_pixel(x, y).0;
            let a = p[3] as u32;
            let blend = |c: u8| ((c as u32 * a + 128 * (255 - a)) / 255) as u8;
            image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])])
        })
    } else {
        image.to_rgb8()
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn has_magick() -> bool {
        std::process::Command::new("magick").arg("-version").output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn shrinks_and_rotates() {
        let jpeg = from_image_bytes(&png(1200, 600), Some(6), THUMB_SIZE).unwrap();
        let thumb = image::load_from_memory(&jpeg).unwrap();
        // Rotated 90°: portrait, long edge THUMB_SIZE.
        assert_eq!((thumb.width(), thumb.height()), (THUMB_SIZE / 2, THUMB_SIZE));
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let jpeg = from_image_bytes(&png(100, 50), None, THUMB_SIZE).unwrap();
        let thumb = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (100, 50));
    }

    #[test]
    fn rejects_garbage() {
        assert!(from_image_bytes(b"not an image", None, THUMB_SIZE).is_err());
    }

    #[test]
    fn sharded_paths() {
        assert_eq!(thumb_path(Path::new("/c"), 0x1234), Path::new("/c/thumbs/34/4660.jpg"));
        assert_eq!(preview_path(Path::new("/c"), 0x1234), Path::new("/c/previews/34/4660.jpg"));
    }

    /// Manual check on real files:
    /// `MC_FILES="a.HEIC:b.psd" MC_OUT=/tmp/out cargo test render_real_files -- --ignored`
    #[test]
    #[ignore]
    fn render_real_files() {
        let out = PathBuf::from(std::env::var("MC_OUT").unwrap());
        for (i, file) in std::env::var("MC_FILES").unwrap().split(':').enumerate() {
            let path = Path::new(file);
            let ext = crate::paths::ext_lower(file);
            for (name, size) in [("thumb", THUMB_SIZE), ("preview", PREVIEW_SIZE)] {
                let jpeg = from_magick(path, &ext, size).unwrap();
                std::fs::write(out.join(format!("{i}-{name}.jpg")), &jpeg).unwrap();
                let img = image::load_from_memory(&jpeg).unwrap();
                println!("{file}: {name} {}x{}", img.width(), img.height());
            }
        }
    }

    #[test]
    fn dhash_survives_resizing_but_tells_images_apart() {
        let gradient = |w: u32, h: u32, flip: bool| {
            let img = RgbImage::from_fn(w, h, |x, y| {
                let v = ((x * 255 / w) as u8).wrapping_add((y * 40 / h) as u8);
                image::Rgb(if flip { [255 - v, 255 - v, 255 - v] } else { [v, v / 2, 255 - v] })
            });
            let mut out = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        };
        let big = dhash(&gradient(800, 600, false)).unwrap();
        let small = dhash(&gradient(200, 150, false)).unwrap();
        let other = dhash(&gradient(800, 600, true)).unwrap();
        assert!((big ^ small).count_ones() <= 4, "resized copy should match");
        assert!((big ^ other).count_ones() > 20, "a different image should not");
        // A flat image has no meaningful hash.
        assert_eq!(dhash(&png(100, 100)), None);
    }

    #[test]
    fn figma_thumbnail_from_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("design.fig");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("thumbnail.png", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut zip, &png(800, 400)).unwrap();
        zip.finish().unwrap();

        let thumb = image::load_from_memory(&from_fig(&path, THUMB_SIZE).unwrap()).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (THUMB_SIZE, THUMB_SIZE / 2));
        assert!(from_fig(Path::new("/nonexistent.fig"), THUMB_SIZE).is_err());
    }

    #[test]
    fn magick_renders_svg_and_pdf() {
        if !has_magick() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("logo.svg");
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="red"/></svg>"#).unwrap();
        let thumb = image::load_from_memory(&from_magick(&svg, "svg", THUMB_SIZE).unwrap()).unwrap();
        assert!(thumb.width() >= thumb.height() * 2 - 2);

        // A PDF named like an Illustrator file, as modern .ai files are.
        let ai = dir.path().join("art.ai");
        let made = std::process::Command::new("magick")
            .args(["-size", "300x200", "xc:blue"])
            .arg(format!("pdf:{}", ai.display()))
            .status()
            .is_ok_and(|s| s.success());
        if made {
            assert!(!from_magick(&ai, "ai", THUMB_SIZE).unwrap().is_empty());
        }
    }
}
