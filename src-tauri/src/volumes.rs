//! Drive detection and stable drive identity.
//!
//! A drive is identified by the contents of a `.mediacat-id` marker file at its
//! root when present, otherwise by its filesystem UUID. When a drive is added to
//! the catalog the marker is written with that UUID, so Linux and Windows (which
//! report NTFS serials differently) agree on the same identity.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use sysinfo::{DiskKind, Disks};

pub const MARKER_FILE: &str = ".mediacat-id";

#[derive(Clone, Debug, Serialize)]
pub struct DetectedVolume {
    /// None when neither a marker nor a filesystem UUID is available yet.
    pub uuid: Option<String>,
    pub label: String,
    pub mount: PathBuf,
    pub fs_type: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub rotational: bool,
    pub removable: bool,
    /// The physical disk holding this volume (partitions of one disk share it).
    pub disk: String,
}

/// Lists mounted drives that can hold media.
pub fn detect() -> Vec<DetectedVolume> {
    let disks = Disks::new_with_refreshed_list();
    let mut found: Vec<(String, DetectedVolume)> = Vec::new();

    for disk in disks.list() {
        let mount = disk.mount_point().to_path_buf();
        let fs_type = disk.file_system().to_string_lossy().to_lowercase();
        let device = disk.name().to_string_lossy().to_string();
        if !is_media_mount(&mount, &fs_type) {
            continue;
        }

        let platform = platform::identity(&device, &mount);
        let uuid = read_marker(&mount).or(platform.uuid);
        let label = platform.label.unwrap_or_else(|| default_label(&mount));
        let volume = DetectedVolume {
            uuid,
            label,
            disk: platform.disk.unwrap_or_else(|| device.clone()),
            mount,
            fs_type,
            total_bytes: disk.total_space(),
            free_bytes: disk.available_space(),
            rotational: matches!(disk.kind(), DiskKind::HDD),
            removable: disk.is_removable(),
        };

        // The same device can be mounted several times (btrfs subvolumes, bind
        // mounts); keep the shortest mount path. On Windows the name is only the
        // volume label, which unlabeled drives share, so each drive letter is its own.
        let key = if cfg!(windows) { volume.mount.to_string_lossy().to_string() } else { device };
        match found.iter_mut().find(|(dev, _)| *dev == key) {
            Some((_, existing)) => {
                if volume.mount.as_os_str().len() < existing.mount.as_os_str().len() {
                    *existing = volume;
                }
            }
            None => found.push((key, volume)),
        }
    }

    let mut list: Vec<DetectedVolume> = found.into_iter().map(|(_, v)| v).collect();
    list.sort_by(|a, b| a.mount.cmp(&b.mount));
    list
}

/// Writes the identity marker at the drive root. Fails if the drive is read-only.
pub fn write_marker(mount: &Path, uuid: &str) -> Result<()> {
    std::fs::write(mount.join(MARKER_FILE), format!("{uuid}\n"))
        .with_context(|| format!("cannot write {MARKER_FILE} on {}", mount.display()))
}

pub fn read_marker(mount: &Path) -> Option<String> {
    let text = std::fs::read_to_string(mount.join(MARKER_FILE)).ok()?;
    let id = text.trim();
    (!id.is_empty() && id.len() <= 128 && !id.contains(['/', '\\'])).then(|| id.to_string())
}

/// Free bytes on the drive that holds `path` (the longest matching mount point).
pub fn free_space_for(path: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn default_label(mount: &Path) -> String {
    mount
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| mount.to_string_lossy().to_string())
}

fn is_media_mount(mount: &Path, fs_type: &str) -> bool {
    const SKIP_FS: &[&str] = &[
        "squashfs", "overlay", "tmpfs", "devtmpfs", "proc", "sysfs", "efivarfs", "iso9660",
    ];
    if SKIP_FS.contains(&fs_type) {
        return false;
    }
    if cfg!(unix) {
        const SKIP_PREFIXES: &[&str] = &["/snap", "/boot", "/var/snap", "/var/lib", "/proc", "/sys"];
        let m = mount.to_string_lossy();
        if SKIP_PREFIXES.iter().any(|p| m == *p || m.starts_with(&format!("{p}/"))) {
            return false;
        }
    }
    true
}

struct PlatformIdentity {
    uuid: Option<String>,
    label: Option<String>,
    disk: Option<String>,
}

#[cfg(target_os = "linux")]
mod platform {
    use super::PlatformIdentity;
    use std::path::Path;

    /// Looks the device up in /dev/disk/by-uuid and /dev/disk/by-label.
    pub fn identity(device: &str, _mount: &Path) -> PlatformIdentity {
        let device = std::fs::canonicalize(device).ok();
        let lookup = |dir: &str| -> Option<String> {
            let device = device.as_ref()?;
            std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
                let target = std::fs::canonicalize(entry.path()).ok()?;
                (&target == device).then(|| unescape(&entry.file_name().to_string_lossy()))
            })
        };
        PlatformIdentity {
            uuid: lookup("/dev/disk/by-uuid"),
            label: lookup("/dev/disk/by-label"),
            disk: device.as_deref().and_then(parent_disk),
        }
    }

    /// "/dev/sda1" → "sda", "/dev/nvme0n1p2" → "nvme0n1": a partition's sysfs entry
    /// lives inside its disk's entry.
    fn parent_disk(device: &Path) -> Option<String> {
        let name = device.file_name()?.to_str()?;
        let sys = std::fs::canonicalize(format!("/sys/class/block/{name}")).ok()?;
        if sys.join("partition").exists() {
            Some(sys.parent()?.file_name()?.to_str()?.to_string())
        } else {
            Some(name.to_string())
        }
    }

    /// udev escapes characters in link names as `\xNN` (e.g. spaces as `\x20`).
    fn unescape(name: &str) -> String {
        let bytes = name.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
                if let Ok(b) = u8::from_str_radix(&name[i + 2..i + 4], 16) {
                    out.push(b);
                    i += 4;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).to_string()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn unescapes_udev_names() {
            assert_eq!(super::unescape(r"My\x20Drive"), "My Drive");
            assert_eq!(super::unescape("Plain"), "Plain");
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::PlatformIdentity;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    /// Uses the volume serial number (formatted like Linux shows FAT/exFAT UUIDs)
    /// and the volume label.
    pub fn identity(_device: &str, mount: &Path) -> PlatformIdentity {
        let mut root: Vec<u16> = mount.as_os_str().encode_wide().collect();
        if root.last() != Some(&(b'\\' as u16)) {
            root.push(b'\\' as u16);
        }
        root.push(0);
        let mut label = [0u16; 261];
        let mut serial: u32 = 0;
        // SAFETY: all buffers are valid for the lengths passed; unused outputs are null.
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            return PlatformIdentity { uuid: None, label: None, disk: None };
        }
        let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
        let label = String::from_utf16_lossy(&label[..len]);
        PlatformIdentity {
            uuid: Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)),
            label: (!label.is_empty()).then_some(label),
            // Each drive letter gets its own worker on Windows for now.
            disk: Some(mount.to_string_lossy().to_string()),
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::PlatformIdentity;
    use std::path::Path;

    pub fn identity(_device: &str, _mount: &Path) -> PlatformIdentity {
        PlatformIdentity { uuid: None, label: None, disk: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_marker(dir.path()), None);
        write_marker(dir.path(), "F0F8C99EF8C96406").unwrap();
        assert_eq!(read_marker(dir.path()).as_deref(), Some("F0F8C99EF8C96406"));
    }

    /// Manual check on a real machine: `cargo test detect_on_this_machine -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn detect_on_this_machine() {
        for v in detect() {
            println!(
                "{:?} {:?} {} {} rotational={} disk={}",
                v.uuid, v.label, v.mount.display(), v.fs_type, v.rotational, v.disk
            );
        }
    }

    #[test]
    fn rejects_bad_marker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MARKER_FILE), "../etc").unwrap();
        assert_eq!(read_marker(dir.path()), None);
    }
}
