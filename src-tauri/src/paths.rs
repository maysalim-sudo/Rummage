//! Volume-relative paths.
//!
//! Files are stored relative to their volume root, always with `/` separators,
//! so the same catalog entry works whether a drive is mounted at
//! `/run/media/me/Disk2` on Linux or `E:\` on Windows. Case is preserved as-is:
//! NTFS and exFAT keep the original case, so exact paths match on both systems.

use std::path::{Component, Path, PathBuf};

/// Converts an absolute path under `mount` to a volume-relative path.
/// Returns `None` if the path is outside the mount or not valid UTF-8.
pub fn to_rel(mount: &Path, abs: &Path) -> Option<String> {
    let rest = abs.strip_prefix(mount).ok()?;
    let mut parts = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// Converts a volume-relative path back to an absolute path under `mount`.
pub fn from_rel(mount: &Path, rel: &str) -> PathBuf {
    let mut path = mount.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path
}

/// Splits a relative path into its directory and file name.
pub fn split_dir_name(rel: &str) -> (&str, &str) {
    match rel.rfind('/') {
        Some(i) => (&rel[..i], &rel[i + 1..]),
        None => ("", rel),
    }
}

/// Lowercase extension of a file name, without the dot ("" if none).
pub fn ext_lower(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_round_trip() {
        let mount = Path::new("/mnt/disk");
        let abs = Path::new("/mnt/disk/Photos/2024/IMG_0001.JPG");
        let rel = to_rel(mount, abs).unwrap();
        assert_eq!(rel, "Photos/2024/IMG_0001.JPG");
        assert_eq!(from_rel(mount, &rel), abs);
    }

    #[test]
    fn mount_itself_is_empty_rel() {
        assert_eq!(to_rel(Path::new("/mnt/disk"), Path::new("/mnt/disk")).unwrap(), "");
    }

    #[test]
    fn outside_mount_is_rejected() {
        assert!(to_rel(Path::new("/mnt/disk"), Path::new("/mnt/other/a.jpg")).is_none());
    }

    #[test]
    fn rel_uses_forward_slashes_on_any_mount() {
        // A Windows-style mount on Windows yields the same rel path as on Linux.
        let mount = PathBuf::from("drive");
        let abs = mount.join("Photos").join("a.jpg");
        assert_eq!(to_rel(&mount, &abs).unwrap(), "Photos/a.jpg");
    }

    #[test]
    fn splits_and_extensions() {
        assert_eq!(split_dir_name("a/b/c.jpg"), ("a/b", "c.jpg"));
        assert_eq!(split_dir_name("c.jpg"), ("", "c.jpg"));
        assert_eq!(ext_lower("IMG.JPG"), "jpg");
        assert_eq!(ext_lower(".hidden"), "");
        assert_eq!(ext_lower("noext"), "");
    }
}
