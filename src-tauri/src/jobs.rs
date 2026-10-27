//! Background processing: reads photo/video details and makes thumbnails for files
//! that are new or changed since they were last processed.
//!
//! One worker thread runs per physical disk (so two partitions of one HDD don't fight
//! over the disk head), using a few threads for SSDs and two for HDDs. Each file is
//! read once; EXIF, size and thumbnail all come from the same bytes.

use crate::media_kind::MediaKind;
use crate::meta::{self, MediaInfo};
use crate::{db, paths, thumbs};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BATCH_SIZE: i64 = 64;
/// Bigger images are thumbnailed through ffmpeg instead of being read into memory.
const MAX_IN_MEMORY: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Target {
    pub volume_id: i64,
    pub mount: PathBuf,
    pub disk: String,
    pub rotational: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub volume_id: i64,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub paused: bool,
    pub volumes: Vec<Progress>,
}

pub type Emit = Arc<dyn Fn(&Status) + Send + Sync>;

#[derive(Default)]
pub struct Jobs {
    paused: AtomicBool,
    busy_disks: Mutex<HashSet<String>>,
    /// Volumes with nothing left to process; cleared when a volume is rescanned.
    idle: Mutex<HashSet<i64>>,
    progress: Mutex<HashMap<i64, Progress>>,
}

impl Jobs {
    pub fn status(&self) -> Status {
        let mut volumes: Vec<Progress> = self.progress.lock().unwrap().values().cloned().collect();
        volumes.sort_by_key(|p| p.volume_id);
        Status { paused: self.paused.load(Ordering::Relaxed), volumes }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Call after a scan so the volume's new or changed files get processed.
    pub fn mark_dirty(&self, volume_id: i64) {
        self.idle.lock().unwrap().remove(&volume_id);
    }

    /// Starts workers for disks that have pending volumes and no worker yet. Cheap to call often.
    pub fn start(self: &Arc<Self>, catalog_dir: &Path, targets: Vec<Target>, emit: Emit) {
        let idle = self.idle.lock().unwrap().clone();
        let mut by_disk: HashMap<String, Vec<Target>> = HashMap::new();
        for t in targets.into_iter().filter(|t| !idle.contains(&t.volume_id)) {
            by_disk.entry(t.disk.clone()).or_default().push(t);
        }
        for (disk, targets) in by_disk {
            if !self.busy_disks.lock().unwrap().insert(disk.clone()) {
                continue; // a worker is already on this disk; it is picked up next time
            }
            let jobs = self.clone();
            let catalog_dir = catalog_dir.to_path_buf();
            let emit = emit.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs.run_disk(&catalog_dir, &targets, &emit) {
                    eprintln!("background processing on {disk} stopped: {e:#}");
                }
                jobs.busy_disks.lock().unwrap().remove(&disk);
            });
        }
    }

    fn run_disk(&self, catalog_dir: &Path, targets: &[Target], emit: &Emit) -> Result<()> {
        let conn = db::open(catalog_dir)?;
        for target in targets {
            let result = self.run_volume(&conn, catalog_dir, target, emit);
            self.progress.lock().unwrap().remove(&target.volume_id);
            emit(&self.status());
            result?;
        }
        Ok(())
    }

    fn run_volume(&self, conn: &Connection, catalog_dir: &Path, target: &Target, emit: &Emit) -> Result<()> {
        let threads = if target.rotational {
            2
        } else {
            std::thread::available_parallelism().map_or(2, |n| (n.get() / 4).clamp(2, 4))
        };
        let pending_sql = "FROM files WHERE volume_id = ?1 AND missing = 0 AND processed_scan < changed_scan";

        // Re-count after finishing: a rescan may have queued more files meanwhile.
        loop {
            let total: i64 = conn.query_row(&format!("SELECT COUNT(*) {pending_sql}"), [target.volume_id], |r| r.get(0))?;
            if total == 0 {
                self.idle.lock().unwrap().insert(target.volume_id);
                return Ok(());
            }
            let mut progress = Progress { volume_id: target.volume_id, done: 0, total: total as u64 };
            self.progress.lock().unwrap().insert(target.volume_id, progress.clone());
            emit(&self.status());

            let mut cursor = 0i64;
            loop {
                while self.paused.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                }
                if !target.mount.is_dir() {
                    return Ok(()); // drive unplugged; resumes when it is back
                }
                let rows: Vec<Pending> = conn
                    .prepare_cached(&format!(
                        "SELECT id, rel_path, kind, changed_scan {pending_sql} AND id > ?2 ORDER BY id LIMIT ?3"
                    ))?
                    .query_map(params![target.volume_id, cursor, BATCH_SIZE], |r| {
                        Ok(Pending { id: r.get(0)?, rel: r.get(1)?, kind: r.get(2)?, changed_scan: r.get(3)? })
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                let Some(last) = rows.last() else { break };
                cursor = last.id;

                let results = process_batch(catalog_dir, &target.mount, &rows, threads);
                if !target.mount.is_dir() {
                    return Ok(()); // unplugged mid-batch: the errors are not real, don't store them
                }
                write_results(conn, &results)?;

                progress.done = (progress.done + rows.len() as u64).min(progress.total);
                self.progress.lock().unwrap().insert(target.volume_id, progress.clone());
                emit(&self.status());
            }
        }
    }
}

struct Pending {
    id: i64,
    rel: String,
    kind: i64,
    changed_scan: i64,
}

#[derive(Debug, Default)]
struct Outcome {
    info: MediaInfo,
    thumb_ok: bool,
    error: Option<String>,
}

fn process_batch(catalog_dir: &Path, mount: &Path, rows: &[Pending], threads: usize) -> Vec<(i64, i64, Outcome)> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(rows.len()));
    std::thread::scope(|scope| {
        for _ in 0..threads.min(rows.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(row) = rows.get(i) else { break };
                let outcome = process_file(catalog_dir, mount, row);
                results.lock().unwrap().push((row.id, row.changed_scan, outcome));
            });
        }
    });
    results.into_inner().unwrap()
}

fn process_file(catalog_dir: &Path, mount: &Path, row: &Pending) -> Outcome {
    let path = paths::from_rel(mount, &row.rel);
    let mut out = Outcome::default();
    let thumb = match MediaKind::from_i64(row.kind) {
        Some(MediaKind::Video) => {
            match meta::video_info(&path) {
                Ok(info) => out.info = info,
                Err(e) => out.error = Some(format!("{e:#}")),
            }
            // A frame 10% in (at most 10 s) is more representative than the often-black first one.
            let seek = out.info.duration_ms.map(|ms| (ms as f64 / 1000.0 * 0.1).min(10.0));
            thumbs::from_ffmpeg(&path, seek).or_else(|_| thumbs::from_ffmpeg(&path, None))
        }
        Some(kind) => {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size > MAX_IN_MEMORY {
                thumbs::from_ffmpeg(&path, None)
            } else {
                match std::fs::read(&path) {
                    Err(e) => Err(e.into()),
                    Ok(bytes) => {
                        out.info = meta::image_info(&bytes);
                        if kind == MediaKind::Raw {
                            Err(anyhow::anyhow!("RAW previews arrive with RAW support (milestone 6)"))
                        } else {
                            // HEIC, AVIF and similar fall back to ffmpeg.
                            thumbs::from_image_bytes(&bytes, out.info.orientation)
                                .or_else(|_| thumbs::from_ffmpeg(&path, None))
                        }
                    }
                }
            }
        }
        None => Err(anyhow::anyhow!("unknown file type")),
    };
    match thumb.and_then(|jpeg| thumbs::save(catalog_dir, row.id, &jpeg)) {
        Ok(()) => out.thumb_ok = true,
        Err(e) => {
            out.error.get_or_insert_with(|| format!("{e:#}"));
        }
    }
    if let Some(error) = &mut out.error {
        error.truncate(300);
    }
    out
}

fn write_results(conn: &Connection, results: &[(i64, i64, Outcome)]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "UPDATE files SET width = ?2, height = ?3, taken_at = ?4, camera = ?5, lens = ?6, iso = ?7,
                    f_number = ?8, exposure = ?9, focal_mm = ?10, gps_lat = ?11, gps_lon = ?12,
                    duration_ms = ?13, codec = ?14, fps = ?15,
                    processed_scan = ?16, thumb_ok = ?17, process_error = ?18
             WHERE id = ?1",
        )?;
        for (id, changed_scan, o) in results {
            let i = &o.info;
            stmt.execute(params![
                id, i.width, i.height, i.taken_at, i.camera, i.lens, i.iso, i.f_number, i.exposure,
                i.focal_mm, i.gps_lat, i.gps_lon, i.duration_ms, i.codec, i.fps,
                changed_scan, o.thumb_ok, o.error
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    use std::fs;

    fn has_ffmpeg() -> bool {
        std::process::Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn processes_images_and_videos() {
        let catalog = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let d = drive.path();
        let mut conn = db::open(catalog.path()).unwrap();
        conn.execute("INSERT INTO volumes (id, uuid, label, added_at) VALUES (1, 't', 'T', 0)", []).unwrap();

        image::RgbImage::new(640, 480).save(d.join("photo.png")).unwrap();
        fs::write(d.join("broken.jpg"), b"not really a jpeg").unwrap();
        let video = has_ffmpeg()
            && std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10"])
                .arg(d.join("clip.mp4"))
                .status()
                .is_ok_and(|s| s.success());

        scanner::scan_volume(&mut conn, 1, d, &["".into()], false, &AtomicBool::new(false), |_, _| {}).unwrap();

        let jobs = Arc::new(Jobs::default());
        let target = Target { volume_id: 1, mount: d.to_path_buf(), disk: "test".into(), rotational: false };
        jobs.run_volume(&conn, catalog.path(), &target, &(Arc::new(|_: &Status| {}) as Emit)).unwrap();

        let row = |name: &str| -> (Option<i64>, Option<i64>, i64, Option<String>, Option<i64>) {
            conn.query_row(
                "SELECT width, height, thumb_ok, process_error, duration_ms FROM files WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap()
        };
        let (w, h, ok, err, _) = row("photo.png");
        assert_eq!((w, h, ok, err), (Some(640), Some(480), 1, None));

        let (_, _, ok, err, _) = row("broken.jpg");
        assert_eq!(ok, 0);
        assert!(err.is_some());

        if video {
            let (w, h, ok, _, duration) = row("clip.mp4");
            assert_eq!((w, h, ok), (Some(320), Some(240), 1));
            assert_eq!(duration, Some(2000));
        }

        let id: i64 = conn.query_row("SELECT id FROM files WHERE name = 'photo.png'", [], |r| r.get(0)).unwrap();
        assert!(thumbs::thumb_path(catalog.path(), id).is_file());

        // Nothing pending afterwards; the volume is idle until rescanned.
        let pending: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE processed_scan < changed_scan", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 0);
        assert!(jobs.idle.lock().unwrap().contains(&1));
    }
}
