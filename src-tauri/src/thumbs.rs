//! Thumbnail generation and the on-disk thumbnail cache.
//!
//! Thumbnails are JPEGs (long edge `THUMB_SIZE`) in `<catalog>/thumbs/<xx>/<file id>.jpg`,
//! sharded by the low byte of the id so no folder gets huge.

use crate::tools;
use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbImage};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const THUMB_SIZE: u32 = 384;
const JPEG_QUALITY: u8 = 80;

pub fn thumb_path(catalog_dir: &Path, file_id: i64) -> PathBuf {
    catalog_dir
        .join("thumbs")
        .join(format!("{:02x}", file_id & 0xff))
        .join(format!("{file_id}.jpg"))
}

pub fn save(catalog_dir: &Path, file_id: i64, jpeg: &[u8]) -> Result<()> {
    let path = thumb_path(catalog_dir, file_id);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, jpeg).with_context(|| format!("cannot write {}", path.display()))
}

/// Decodes an image in memory and returns a thumbnail JPEG, rotated upright.
/// Fails with an unsupported-format error for formats the decoder lacks (HEIC, AVIF…).
pub fn from_image_bytes(bytes: &[u8], orientation: Option<u32>) -> Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)?;
    let small = if image.width() > THUMB_SIZE || image.height() > THUMB_SIZE {
        // A cheap pre-shrink keeps the quality filter fast on 50-megapixel photos.
        let pre = image.thumbnail(THUMB_SIZE * 2, THUMB_SIZE * 2);
        pre.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle)
    } else {
        image
    };
    encode_jpeg(&orient(small, orientation.unwrap_or(1)))
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

    #[test]
    fn shrinks_and_rotates() {
        let jpeg = from_image_bytes(&png(1200, 600), Some(6)).unwrap();
        let thumb = image::load_from_memory(&jpeg).unwrap();
        // Rotated 90°: portrait, long edge THUMB_SIZE.
        assert_eq!((thumb.width(), thumb.height()), (THUMB_SIZE / 2, THUMB_SIZE));
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let jpeg = from_image_bytes(&png(100, 50), None).unwrap();
        let thumb = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (100, 50));
    }

    #[test]
    fn rejects_garbage() {
        assert!(from_image_bytes(b"not an image", None).is_err());
    }

    #[test]
    fn sharded_paths() {
        let p = thumb_path(Path::new("/c"), 0x1234);
        assert_eq!(p, Path::new("/c/thumbs/34/4660.jpg"));
    }
}
