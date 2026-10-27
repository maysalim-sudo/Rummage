//! Photo and video details: EXIF for images (from bytes already in memory),
//! ffprobe for videos.

use crate::tools;
use anyhow::{Context, Result};
use exif::{In, Tag, Value};
use serde_json::Value as Json;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaInfo {
    /// Display size, after rotation.
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub taken_at: Option<i64>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<i64>,
    pub f_number: Option<f64>,
    pub exposure: Option<String>,
    pub focal_mm: Option<f64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub duration_ms: Option<i64>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
    /// EXIF orientation (1–8), needed to rotate the thumbnail.
    pub orientation: Option<u32>,
}

/// Reads EXIF and pixel size from an image file's bytes. Never fails: missing data stays None.
pub fn image_info(bytes: &[u8]) -> MediaInfo {
    let mut info = MediaInfo::default();
    let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)).ok();

    if let Some(exif) = &exif {
        let field = |tag| exif.get_field(tag, In::PRIMARY);
        let text = |tag| field(tag).and_then(|f| ascii(&f.value));
        let rational = |tag| match field(tag).map(|f| &f.value) {
            Some(Value::Rational(v)) if !v.is_empty() && v[0].denom != 0 => Some(v[0].to_f64()),
            _ => None,
        };

        info.taken_at = text(Tag::DateTimeOriginal)
            .or_else(|| text(Tag::DateTimeDigitized))
            .or_else(|| text(Tag::DateTime))
            .and_then(|s| parse_datetime(&s));
        info.camera = camera_name(text(Tag::Make), text(Tag::Model));
        info.lens = text(Tag::LensModel);
        info.iso = field(Tag::PhotographicSensitivity).and_then(|f| f.value.get_uint(0)).map(i64::from);
        info.f_number = rational(Tag::FNumber);
        info.focal_mm = rational(Tag::FocalLength);
        info.exposure = rational(Tag::ExposureTime).map(format_exposure);
        info.orientation = field(Tag::Orientation).and_then(|f| f.value.get_uint(0));
        info.gps_lat = gps_coord(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S");
        info.gps_lon = gps_coord(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W");

        let dim = |tag| field(tag).and_then(|f| f.value.get_uint(0)).map(i64::from);
        info.width = dim(Tag::PixelXDimension);
        info.height = dim(Tag::PixelYDimension);
    }

    // The header size is more reliable than EXIF (which survives crops/resizes).
    if let Ok(size) = imagesize::blob_size(bytes) {
        info.width = Some(size.width as i64);
        info.height = Some(size.height as i64);
    }
    if matches!(info.orientation, Some(5..=8)) {
        std::mem::swap(&mut info.width, &mut info.height);
    }
    info
}

/// Reads video details with ffprobe.
pub fn video_info(path: &Path) -> Result<MediaInfo> {
    let mut cmd = tools::command("ffprobe");
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
    let out = tools::run(cmd, Duration::from_secs(30)).context("ffprobe failed")?;
    let json: Json = serde_json::from_slice(&out).context("ffprobe returned invalid JSON")?;
    Ok(parse_ffprobe(&json))
}

fn parse_ffprobe(json: &Json) -> MediaInfo {
    let mut info = MediaInfo::default();
    let format = &json["format"];
    let tags = &format["tags"];
    let tag = |key: &str| tags[key].as_str().map(str::to_string);

    info.duration_ms = format["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .map(|d| (d * 1000.0).round() as i64);
    info.taken_at = tag("com.apple.quicktime.creationdate")
        .or_else(|| tag("creation_time"))
        .and_then(|s| parse_datetime(&s));
    info.camera = camera_name(tag("com.apple.quicktime.make"), tag("com.apple.quicktime.model"));

    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    if let Some(video) = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1) {
        info.width = video["width"].as_i64();
        info.height = video["height"].as_i64();
        info.codec = video["codec_name"].as_str().map(str::to_string);
        info.fps = video["avg_frame_rate"].as_str().and_then(parse_ratio).filter(|f| *f > 0.0);

        let rotation = video["tags"]["rotate"]
            .as_str()
            .and_then(|r| r.parse::<i64>().ok())
            .or_else(|| {
                video["side_data_list"]
                    .as_array()?
                    .iter()
                    .find_map(|d| d["rotation"].as_i64())
            })
            .unwrap_or(0);
        if rotation.rem_euclid(180) == 90 {
            std::mem::swap(&mut info.width, &mut info.height);
        }
    }
    info
}

fn ascii(value: &Value) -> Option<String> {
    match value {
        Value::Ascii(parts) => {
            let text: String = parts
                .iter()
                .map(|p| String::from_utf8_lossy(p).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

/// "Canon" + "Canon EOS R5" → "Canon EOS R5"; "NIKON CORPORATION" + "NIKON Z 6" → "NIKON Z 6".
fn camera_name(make: Option<String>, model: Option<String>) -> Option<String> {
    match (make, model) {
        (Some(make), Some(model)) => {
            let brand = make.split_whitespace().next().unwrap_or("").to_lowercase();
            if model.to_lowercase().starts_with(&brand) {
                Some(model)
            } else {
                Some(format!("{make} {model}"))
            }
        }
        (None, Some(model)) => Some(model),
        (Some(make), None) => Some(make),
        (None, None) => None,
    }
}

fn format_exposure(seconds: f64) -> String {
    if seconds > 0.0 && seconds < 1.0 {
        format!("1/{}", (1.0 / seconds).round())
    } else {
        format!("{seconds}s")
    }
}

fn gps_coord(exif: &exif::Exif, tag: Tag, ref_tag: Tag, negative: &str) -> Option<f64> {
    let Value::Rational(parts) = &exif.get_field(tag, In::PRIMARY)?.value else {
        return None;
    };
    if parts.len() < 3 || parts.iter().any(|p| p.denom == 0) {
        return None;
    }
    let value = parts[0].to_f64() + parts[1].to_f64() / 60.0 + parts[2].to_f64() / 3600.0;
    let reference = exif.get_field(ref_tag, In::PRIMARY).and_then(|f| ascii(&f.value)).unwrap_or_default();
    Some(if reference.eq_ignore_ascii_case(negative) { -value } else { value })
}

fn parse_ratio(text: &str) -> Option<f64> {
    let (num, den) = text.split_once('/')?;
    let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
    (den != 0.0).then(|| num / den)
}

/// Parses "2024:05:01 12:34:56" (EXIF) or "2024-05-01T12:34:56.000Z" (video tags) to a
/// unix timestamp. The time zone is ignored: times are kept as the clock showed them.
pub fn parse_datetime(text: &str) -> Option<i64> {
    let b = text.trim().as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        let s = std::str::from_utf8(&b[range]).ok()?;
        s.bytes().all(|c| c.is_ascii_digit()).then(|| s.parse().ok())?
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if year < 1900 || !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_and_iso_dates() {
        assert_eq!(parse_datetime("1970:01:01 00:00:00"), Some(0));
        assert_eq!(parse_datetime("2024:05:01 12:34:56"), Some(1_714_566_896));
        assert_eq!(parse_datetime("2024-05-01T12:34:56.000000Z"), Some(1_714_566_896));
        assert_eq!(parse_datetime("2024-05-01T12:34:56+0200"), Some(1_714_566_896));
        assert_eq!(parse_datetime("0000:00:00 00:00:00"), None);
        assert_eq!(parse_datetime("    :  :     :  :  "), None);
        assert_eq!(parse_datetime("2024"), None);
    }

    #[test]
    fn camera_names() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(camera_name(s("Canon"), s("Canon EOS R5")), s("Canon EOS R5"));
        assert_eq!(camera_name(s("NIKON CORPORATION"), s("NIKON Z 6")), s("NIKON Z 6"));
        assert_eq!(camera_name(s("SONY"), s("ILCE-7M3")), s("SONY ILCE-7M3"));
        assert_eq!(camera_name(None, None), None);
    }

    #[test]
    fn exposure_format() {
        assert_eq!(format_exposure(0.004), "1/250");
        assert_eq!(format_exposure(2.0), "2s");
    }

    #[test]
    fn png_size_without_exif() {
        let mut bytes = Vec::new();
        image::RgbImage::new(40, 20)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let info = image_info(&bytes);
        assert_eq!((info.width, info.height), (Some(40), Some(20)));
        assert_eq!(info.taken_at, None);
    }

    #[test]
    fn ffprobe_json_with_rotation() {
        let json: Json = serde_json::from_str(
            r#"{
              "streams": [
                {"codec_type": "audio", "codec_name": "aac"},
                {"codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080,
                 "avg_frame_rate": "30000/1001", "disposition": {"attached_pic": 0},
                 "side_data_list": [{"rotation": -90}]}
              ],
              "format": {"duration": "12.345",
                         "tags": {"creation_time": "2023-07-04T18:00:00.000000Z",
                                  "com.apple.quicktime.make": "Apple",
                                  "com.apple.quicktime.model": "iPhone 15 Pro"}}
            }"#,
        )
        .unwrap();
        let info = parse_ffprobe(&json);
        assert_eq!((info.width, info.height), (Some(1080), Some(1920)));
        assert_eq!(info.duration_ms, Some(12_345));
        assert_eq!(info.codec.as_deref(), Some("hevc"));
        assert!((info.fps.unwrap() - 29.97).abs() < 0.01);
        assert_eq!(info.camera.as_deref(), Some("Apple iPhone 15 Pro"));
        assert_eq!(info.taken_at, parse_datetime("2023-07-04 18:00:00"));
    }
}
