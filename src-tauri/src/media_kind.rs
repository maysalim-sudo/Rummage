use serde::{Deserialize, Serialize};

/// The categories of files the catalog tracks. Stored in the database as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
    Raw,
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

impl MediaKind {
    /// Classifies a lowercase file extension (without the dot).
    pub fn from_ext(ext: &str) -> Option<Self> {
        if RAW_EXTS.contains(&ext) {
            Some(Self::Raw)
        } else if IMAGE_EXTS.contains(&ext) {
            Some(Self::Image)
        } else if VIDEO_EXTS.contains(&ext) {
            Some(Self::Video)
        } else {
            None
        }
    }

    pub fn as_i64(self) -> i64 {
        match self {
            Self::Image => 1,
            Self::Video => 2,
            Self::Raw => 3,
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::Image),
            2 => Some(Self::Video),
            3 => Some(Self::Raw),
            _ => None,
        }
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
        for kind in [MediaKind::Image, MediaKind::Video, MediaKind::Raw] {
            assert_eq!(MediaKind::from_i64(kind.as_i64()), Some(kind));
        }
    }
}
