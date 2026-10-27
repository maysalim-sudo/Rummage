//! Incremental drive scanner.
//!
//! Walks the scan roots of one volume and upserts every media file into the
//! catalog. Unchanged files (same size and mtime) only get their `seen_scan`
//! bumped; afterwards, files under the roots that were not seen are flagged
//! `missing` (kept, not deleted, so tags and history survive a move).

use crate::db;
use crate::media_kind::MediaKind;
use crate::paths;
use anyhow::{bail, Result};
use jwalk::{Parallelism, WalkDir};
use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

const BATCH_SIZE: usize = 2000;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Folder names never worth scanning (trash, OS internals, tooling).
const SKIP_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    ".trashes",
    "lost+found",
    ".cache",
    "node_modules",
    ".git",
];

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ScanStats {
    pub files_seen: u64,
    pub added: u64,
    pub changed: u64,
    pub removed: u64,
    /// Files that could not be read or have non-UTF-8 names.
    pub skipped: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    Done,
    Cancelled,
}

struct Row {
    rel: String,
    kind: MediaKind,
    size: i64,
    mtime: i64,
}

/// Scans one volume mounted at `mount`. `on_progress` is called at most every
/// 250 ms with the running stats and the folder being read.
pub fn scan_volume(
    conn: &mut Connection,
    volume_id: i64,
    mount: &Path,
    roots: &[String],
    rotational: bool,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(&ScanStats, &str),
) -> Result<(ScanStatus, ScanStats)> {
    if !mount.is_dir() {
        bail!("drive is not mounted at {}", mount.display());
    }
    conn.execute(
        "INSERT INTO scans (volume_id, started_at, status) VALUES (?1, ?2, 'running')",
        params![volume_id, db::now()],
    )?;
    let scan_id = conn.last_insert_rowid();

    let result = walk_roots(conn, volume_id, scan_id, mount, roots, rotational, cancel, &mut on_progress);
    let (status_text, error) = match &result {
        Ok((ScanStatus::Done, _)) => ("done", None),
        Ok((ScanStatus::Cancelled, _)) => ("cancelled", None),
        Err(e) => ("failed", Some(format!("{e:#}"))),
    };
    let stats = result.as_ref().map(|(_, s)| s.clone()).unwrap_or_default();
    conn.execute(
        "UPDATE scans SET finished_at = ?2, status = ?3, files_seen = ?4, added = ?5,
                changed = ?6, removed = ?7, error = ?8 WHERE id = ?1",
        params![
            scan_id,
            db::now(),
            status_text,
            stats.files_seen as i64,
            stats.added as i64,
            stats.changed as i64,
            stats.removed as i64,
            error
        ],
    )?;
    if matches!(result, Ok((ScanStatus::Done, _))) {
        conn.execute("UPDATE volumes SET last_scan_at = ?2 WHERE id = ?1", params![volume_id, db::now()])?;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn walk_roots(
    conn: &mut Connection,
    volume_id: i64,
    scan_id: i64,
    mount: &Path,
    roots: &[String],
    rotational: bool,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(&ScanStats, &str),
) -> Result<(ScanStatus, ScanStats)> {
    let mut stats = ScanStats::default();
    let mut batch: Vec<Row> = Vec::with_capacity(BATCH_SIZE);
    let mut last_report = Instant::now();
    let mut complete_roots: Vec<&str> = Vec::new();

    for root in roots {
        let abs_root = paths::from_rel(mount, root);
        if !abs_root.is_dir() {
            // Leave files under a vanished root alone instead of flagging them all missing.
            continue;
        }
        // Spinning disks are fastest when read by a single thread.
        let parallelism = if rotational {
            Parallelism::Serial
        } else {
            Parallelism::RayonNewPool(4)
        };
        let walker = WalkDir::new(&abs_root)
            .skip_hidden(false)
            .follow_links(false)
            .parallelism(parallelism)
            .process_read_dir(|_, _, _, children| {
                children.retain(|entry| match entry {
                    Ok(e) => !(e.file_type().is_dir() && should_skip_dir(&e.file_name().to_string_lossy())),
                    Err(_) => true,
                });
            });

        for entry in walker {
            if cancel.load(Ordering::Relaxed) {
                flush(conn, volume_id, scan_id, &mut batch, &mut stats)?;
                return Ok((ScanStatus::Cancelled, stats));
            }
            let Ok(entry) = entry else {
                stats.skipped += 1;
                continue;
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let Some(kind) = MediaKind::from_ext(&paths::ext_lower(&name)) else {
                continue;
            };
            let path = entry.path();
            let (Some(rel), Ok(meta)) = (paths::to_rel(mount, &path), entry.metadata()) else {
                stats.skipped += 1;
                continue;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            batch.push(Row { rel, kind, size: meta.len() as i64, mtime });
            stats.files_seen += 1;

            if batch.len() >= BATCH_SIZE {
                flush(conn, volume_id, scan_id, &mut batch, &mut stats)?;
            }
            if last_report.elapsed() >= PROGRESS_INTERVAL {
                last_report = Instant::now();
                let dir = entry.parent_path().to_string_lossy().to_string();
                on_progress(&stats, &dir);
            }
        }
        complete_roots.push(root);
    }
    flush(conn, volume_id, scan_id, &mut batch, &mut stats)?;
    stats.removed = mark_missing(conn, volume_id, scan_id, &complete_roots)?;
    on_progress(&stats, "");
    Ok((ScanStatus::Done, stats))
}

fn should_skip_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    SKIP_DIRS.contains(&lower.as_str()) || lower.starts_with(".trash-")
}

fn flush(conn: &mut Connection, volume_id: i64, scan_id: i64, batch: &mut Vec<Row>, stats: &mut ScanStats) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    upsert_rows(&tx, volume_id, scan_id, batch, stats)?;
    tx.commit()?;
    batch.clear();
    Ok(())
}

fn upsert_rows(tx: &Transaction, volume_id: i64, scan_id: i64, rows: &[Row], stats: &mut ScanStats) -> Result<()> {
    // SET expressions see the row's old values, so the CASE compares old vs new.
    let mut stmt = tx.prepare_cached(
        "INSERT INTO files (volume_id, rel_path, dir, name, ext, kind, size, mtime,
                            first_scan, changed_scan, seen_scan, missing)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9, 0)
         ON CONFLICT (volume_id, rel_path) DO UPDATE SET
             changed_scan = CASE WHEN files.size <> excluded.size OR files.mtime <> excluded.mtime
                                 THEN excluded.seen_scan ELSE files.changed_scan END,
             size = excluded.size, mtime = excluded.mtime,
             kind = excluded.kind, ext = excluded.ext,
             seen_scan = excluded.seen_scan, missing = 0
         RETURNING first_scan, changed_scan",
    )?;
    for row in rows {
        let (dir, name) = paths::split_dir_name(&row.rel);
        let (first, changed): (i64, i64) = stmt.query_row(
            params![
                volume_id,
                row.rel,
                dir,
                name,
                paths::ext_lower(name),
                row.kind.as_i64(),
                row.size,
                row.mtime,
                scan_id
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if first == scan_id {
            stats.added += 1;
        } else if changed == scan_id {
            stats.changed += 1;
        }
    }
    Ok(())
}

/// Flags files under the fully scanned roots that this scan did not see.
fn mark_missing(conn: &Connection, volume_id: i64, scan_id: i64, roots: &[&str]) -> Result<u64> {
    let mut removed = 0;
    for root in roots {
        removed += if root.is_empty() {
            conn.execute(
                "UPDATE files SET missing = 1
                 WHERE volume_id = ?1 AND seen_scan < ?2 AND missing = 0",
                params![volume_id, scan_id],
            )?
        } else {
            conn.execute(
                "UPDATE files SET missing = 1
                 WHERE volume_id = ?1 AND seen_scan < ?2 AND missing = 0
                   AND (rel_path = ?3 OR substr(rel_path, 1, length(?3) + 1) = ?3 || '/')",
                params![volume_id, scan_id, root],
            )?
        };
    }
    Ok(removed as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Connection) {
        let catalog = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let conn = db::open(catalog.path()).unwrap();
        conn.execute(
            "INSERT INTO volumes (id, uuid, label, added_at) VALUES (1, 'test', 'Test', 0)",
            [],
        )
        .unwrap();
        (catalog, drive, conn)
    }

    fn scan(conn: &mut Connection, drive: &Path, roots: &[&str]) -> (ScanStatus, ScanStats) {
        let roots: Vec<String> = roots.iter().map(|s| s.to_string()).collect();
        scan_volume(conn, 1, drive, &roots, false, &AtomicBool::new(false), |_, _| {}).unwrap()
    }

    #[test]
    fn incremental_rescan() {
        let (_c, drive, mut conn) = setup();
        let d = drive.path();
        fs::create_dir_all(d.join("Photos/2024")).unwrap();
        fs::create_dir_all(d.join("$RECYCLE.BIN")).unwrap();
        fs::write(d.join("Photos/2024/a.jpg"), b"aaa").unwrap();
        fs::write(d.join("Photos/2024/b.CR2"), b"bbb").unwrap();
        fs::write(d.join("clip.mp4"), b"ccc").unwrap();
        fs::write(d.join("notes.txt"), b"ignored").unwrap();
        fs::write(d.join("$RECYCLE.BIN/old.jpg"), b"trash").unwrap();

        let (status, stats) = scan(&mut conn, d, &[""]);
        assert_eq!(status, ScanStatus::Done);
        assert_eq!((stats.files_seen, stats.added, stats.changed, stats.removed), (3, 3, 0, 0));

        let kind: i64 = conn
            .query_row("SELECT kind FROM files WHERE name = 'b.CR2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(MediaKind::from_i64(kind), Some(MediaKind::Raw));

        // One changed, one new, one deleted.
        fs::write(d.join("Photos/2024/a.jpg"), b"aaaa-longer").unwrap();
        fs::write(d.join("Photos/new.png"), b"n").unwrap();
        fs::remove_file(d.join("clip.mp4")).unwrap();
        let (_, stats) = scan(&mut conn, d, &[""]);
        assert_eq!((stats.files_seen, stats.added, stats.changed, stats.removed), (3, 1, 1, 1));

        let missing: i64 = conn
            .query_row("SELECT missing FROM files WHERE name = 'clip.mp4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 1);

        // A file that comes back is no longer missing.
        fs::write(d.join("clip.mp4"), b"ccc").unwrap();
        scan(&mut conn, d, &[""]);
        let missing: i64 = conn
            .query_row("SELECT missing FROM files WHERE name = 'clip.mp4'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(missing, 0);
    }

    #[test]
    fn root_limits_missing_marking() {
        let (_c, drive, mut conn) = setup();
        let d = drive.path();
        fs::create_dir_all(d.join("A")).unwrap();
        fs::create_dir_all(d.join("B")).unwrap();
        fs::write(d.join("A/a.jpg"), b"a").unwrap();
        fs::write(d.join("B/b.jpg"), b"b").unwrap();
        scan(&mut conn, d, &[""]);

        // Scanning only A must not flag B's files as missing.
        let (_, stats) = scan(&mut conn, d, &["A"]);
        assert_eq!((stats.files_seen, stats.removed), (1, 0));
    }

    #[test]
    fn cancelled_scan_does_not_mark_missing() {
        let (_c, drive, mut conn) = setup();
        let d = drive.path();
        fs::write(d.join("a.jpg"), b"a").unwrap();
        scan(&mut conn, d, &[""]);

        let cancel = AtomicBool::new(true);
        let (status, _) =
            scan_volume(&mut conn, 1, d, &["".to_string()], false, &cancel, |_, _| {}).unwrap();
        assert_eq!(status, ScanStatus::Cancelled);
        let missing: i64 = conn.query_row("SELECT SUM(missing) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(missing, 0);
    }

    #[test]
    fn full_text_search_finds_names() {
        let (_c, drive, mut conn) = setup();
        fs::write(drive.path().join("IMG_1234.jpg"), b"a").unwrap();
        scan(&mut conn, drive.path(), &[""]);
        let hits: i64 = conn
            .query_row("SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH '\"1234\"*'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1);
    }
}
