use serde::{Deserialize, Serialize};

/// The categories of files the catalog tracks. Stored in the database as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
    Raw,
    /// Illustrator, EPS, PDF and Figma files.
    Design,
}

const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif",
    "avif", "jxl", "psd", "svg", "ico", "tga", "exr", "hdr",
];

const VIDEO_EXTS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "avi", "webm", "wmv", "flv", "mts", "m2ts", "ts", "3gp", "mpg",
    "mpeg", "mxf", "vob", "ogv",
];

const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "rw2", "pef", "dng",
    "srw", "x3f", "3fr", "iiq", "erf", "mef", "mos", "kdc", "rwl",
];

const DESIGN_EXTS: &[&str] = &["ai", "eps", "pdf", "fig"];

/// Extensions also used by non-media files (TypeScript `.ts`, `.ai` domain names saved as
/// files…); their content is checked with `sniff` before they are catalogued.
pub const SNIFFED_EXTS: &[&str] = &["ts", "mts", "m2ts", "ai", "eps", "pdf", "fig"];

impl MediaKind {
    /// Classifies a lowercase file extension (without the dot).
    pub fn from_ext(ext: &str) -> Option<Self> {
        if RAW_EXTS.contains(&ext) {
            Some(Self::Raw)
        } else if IMAGE_EXTS.contains(&ext) {
            Some(Self::Image)
        } else if VIDEO_EXTS.contains(&ext) {
            Some(Self::Video)
        } else if DESIGN_EXTS.contains(&ext) {
            Some(Self::Design)
        } else {
            None
        }
    }

    pub fn as_i64(self) -> i64 {
        match self {
            Self::Image => 1,
            Self::Video => 2,
            Self::Raw => 3,
            Self::Design => 4,
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::Image),
            2 => Some(Self::Video),
            3 => Some(Self::Raw),
            4 => Some(Self::Design),
            _ => None,
        }
    }
}

/// Checks the first bytes of a file whose extension is in `SNIFFED_EXTS`.
pub fn sniff(ext: &str, head: &[u8]) -> bool {
    let at = |offset: usize, magic: &[u8]| head.get(offset..offset + magic.len()) == Some(magic);
    match ext {
        // MPEG-TS packets start with sync byte 0x47; Blu-ray/AVCHD put a 4-byte timestamp first.
        "ts" | "mts" | "m2ts" => at(0, &[0x47]) || at(4, &[0x47]),
        // Illustrator files are PDF (CS and later) or PostScript (older versions).
        "ai" => at(0, b"%PDF") || at(0, b"%!PS"),
        // Plain or DOS-binary EPS.
        "eps" => at(0, b"%!PS") || at(0, &[0xC5, 0xD0, 0xD3, 0xC6]),
        // Some PDF writers put a few junk bytes before the header.
        "pdf" => head.windows(5).take(1024).any(|w| w == b"%PDF-"),
        // Figma: a zip archive (current) or the raw "fig-kiwi" format.
        "fig" => at(0, b"PK\x03\x04") || at(0, b"fig-kiwi"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_extensions() {
        assert_eq!(MediaKind::from_ext("jpg"), Some(MediaKind::Image));
        assert_eq!(MediaKind::from_ext("mov"), Some(MediaKind::Video));
        assert_eq!(MediaKind::from_ext("dng"), Some(MediaKind::Raw));
        assert_eq!(MediaKind::from_ext("txt"), None);
    }

    #[test]
    fn integer_round_trip() {
        for kind in [MediaKind::Image, MediaKind::Video, MediaKind::Raw, MediaKind::Design] {
            assert_eq!(MediaKind::from_i64(kind.as_i64()), Some(kind));
        }
    }

    #[test]
    fn sniffs_shared_extensions() {
        assert!(sniff("ts", &[0x47, 0x40, 0, 0x10, 0]));
        assert!(sniff("mts", &[0, 0, 0, 0, 0x47]));
        assert!(!sniff("ts", b"export const x = 1;"));
        assert!(sniff("ai", b"%PDF-1.6\n"));
        assert!(sniff("ai", b"%!PS-Adobe-3.0"));
        assert!(!sniff("ai", b"From: someone@maiolabs.ai"));
        assert!(sniff("pdf", b"\xEF\xBB\xBF%PDF-1.4"));
        assert!(!sniff("pdf", b"<html>"));
        assert!(sniff("eps", &[0xC5, 0xD0, 0xD3, 0xC6, 0]));
        assert!(sniff("fig", b"PK\x03\x04rest"));
        assert!(!sniff("fig", b"figure 1"));
        assert!(sniff("jpg", b"anything"));
    }
}
