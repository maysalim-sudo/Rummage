//! Duplicate finder.
//!
//! **Exact copies** are found in stages so most files are never read: only files sharing a
//! size with another file are candidates; a quick hash (size + first and last 64 KB) rules
//! most of those out; only files whose quick hashes still collide get a full BLAKE3 hash.
//! Hashes are kept in the catalog (valid while `hash_scan = changed_scan`), so results
//! survive restarts and files on unplugged drives still count once hashed.
//!
//! **Similar images** (resized, re-exported, recompressed) are found from a 64-bit
//! difference hash of each cached thumbnail, so no drive is read. Candidate pairs come from
//! 5 bands of the hash: two hashes at most 4 bits apart must agree exactly on at least one
//! band, which keeps the comparison fast for hundreds of thousands of images.

use crate::jobs::Target;
use crate::media_kind::MediaKind;
use crate::query::{self, FileRow};
use crate::{db, paths, thumbs};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

const QUICK_CHUNK: u64 = 64 * 1024;
/// Similar images: at most this many differing bits out of 64.
const MAX_DISTANCE: u32 = 4;
const BANDS: [(u32, u32); 5] = [(0, 13), (13, 13), (26, 13), (39, 13), (52, 12)];
/// Buckets this full are flat or generic images (screenshots of a blank page…); skip them.
const MAX_BUCKET: usize = 500;
/// Images smaller than this (icons, UI assets) are left out of the similar search.
const MIN_SIMILAR_SIDE: i64 = 200;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    QuickHash,
    FullHash,
    Fingerprints,
    Comparing,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub exact_groups: i64,
    /// Bytes that removing all but one copy of every exact group would free.
    pub exact_wasted: i64,
    pub similar_groups: i64,
    /// Candidates on unplugged drives that could not be hashed yet.
    pub unchecked_offline: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Full hash (exact) or group number (similar).
    pub key: String,
    pub files: Vec<FileRow>,
    /// Bytes freed by keeping only the first file.
    pub wasted: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPage {
    pub total: i64,
    pub groups: Vec<Group>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    Exact,
    Similar,
}

/// Runs the whole search. `drives` are the plugged-in drives; files elsewhere use stored hashes.
pub fn run(catalog_dir: &Path, drives: &[Target], cancel: &AtomicBool, progress: &(dyn Fn(&Progress) + Sync)) -> Result<Summary> {
    let mut conn = db::open(catalog_dir)?;
    let online: HashMap<i64, &Target> = drives.iter().map(|d| (d.volume_id, d)).collect();

    // 1. Quick hashes for files that share their size with another file.
    let pending = pending_rows(
        &conn,
        "size > 0 AND NOT (hash_scan = changed_scan AND quick_hash IS NOT NULL)
         AND size IN (SELECT size FROM files WHERE missing = 0 AND size > 0 GROUP BY size HAVING COUNT(*) > 1)",
    )?;
    hash_rows(&mut conn, &online, pending, Stage::QuickHash, cancel, progress)?;

    // 2. Full hashes where size and quick hash both collide.
    let pending = pending_rows(
        &conn,
        "hash_scan = changed_scan AND quick_hash IS NOT NULL AND full_hash IS NULL
         AND (size, quick_hash) IN (SELECT size, quick_hash FROM files
                                    WHERE missing = 0 AND hash_scan = changed_scan AND quick_hash IS NOT NULL
                                    GROUP BY size, quick_hash HAVING COUNT(*) > 1)",
    )?;
    hash_rows(&mut conn, &online, pending, Stage::FullHash, cancel, progress)?;

    // 3. Fingerprints for thumbnails made before they were computed with each thumbnail.
    backfill_fingerprints(&mut conn, catalog_dir, cancel, progress)?;

    // 4. Group similar images.
    if !cancel.load(Ordering::Relaxed) {
        group_similar(&mut conn, progress)?;
    }
    summary(&conn)
}

struct Pending {
    id: i64,
    volume_id: i64,
    rel: String,
    size: i64,
    changed_scan: i64,
}

fn pending_rows(conn: &Connection, condition: &str) -> Result<Vec<Pending>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT id, volume_id, rel_path, size, changed_scan FROM files WHERE missing = 0 AND {condition}
         ORDER BY volume_id, rel_path"
    ))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Pending { id: r.get(0)?, volume_id: r.get(1)?, rel: r.get(2)?, size: r.get(3)?, changed_scan: r.get(4)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// One physical disk's files to hash, the next index to take, and how many threads read it.
type DiskWork<'a> = (Vec<(&'a Pending, &'a Path)>, AtomicU64, usize);

/// Hashes the rows on plugged-in drives: one worker per physical disk (single-threaded on
/// HDDs, which hate parallel reads), results written as they arrive.
fn hash_rows(
    conn: &mut Connection,
    online: &HashMap<i64, &Target>,
    rows: Vec<Pending>,
    stage: Stage,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&Progress) + Sync),
) -> Result<()> {
    let mut by_disk: HashMap<&str, Vec<(&Pending, &Path)>> = HashMap::new();
    for row in &rows {
        if let Some(drive) = online.get(&row.volume_id) {
            by_disk.entry(drive.disk.as_str()).or_default().push((row, drive.mount.as_path()));
        }
    }
    let rotational: HashMap<&str, bool> = online.values().map(|d| (d.disk.as_str(), d.rotational)).collect();
    let disks: Vec<DiskWork> = by_disk
        .into_iter()
        .map(|(disk, work)| (work, AtomicU64::new(0), if rotational[disk] { 1 } else { 4 }))
        .collect();
    let total: u64 = disks.iter().map(|d| d.0.len() as u64).sum();
    let done = AtomicU64::new(0);
    progress(&Progress { stage, done: 0, total });
    if total == 0 {
        return Ok(());
    }

    let (tx, rx) = mpsc::channel::<(i64, i64, Option<String>)>();
    std::thread::scope(|scope| -> Result<()> {
        for (work, next, threads) in &disks {
            for _ in 0..*threads {
                let (tx, done) = (tx.clone(), &done);
                scope.spawn(move || loop {
                    let i = next.fetch_add(1, Ordering::Relaxed) as usize;
                    let Some((row, mount)) = work.get(i) else { break };
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let path = paths::from_rel(mount, &row.rel);
                    let hash = match stage {
                        Stage::QuickHash => quick_hash(&path, row.size as u64).ok(),
                        _ => full_hash(&path, row.size as u64, cancel).ok(),
                    };
                    done.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.send((row.id, row.changed_scan, hash));
                });
            }
        }
        drop(tx);

        // Write on this thread (the connection isn't shared), in batches.
        let by_id: HashMap<i64, i64> = rows.iter().map(|r| (r.id, r.size)).collect();
        let mut batch = Vec::new();
        let mut flush = |batch: &mut Vec<(i64, i64, Option<String>)>| -> Result<()> {
            let tx = conn.transaction()?;
            for (id, changed_scan, hash) in batch.drain(..) {
                let Some(hash) = hash else { continue }; // unreadable now; try again next run
                if stage == Stage::QuickHash {
                    // A small file's quick hash covers all of it, so it is also its full hash.
                    let whole = by_id.get(&id).is_some_and(|&size| size as u64 <= 2 * QUICK_CHUNK);
                    tx.execute(
                        "UPDATE files SET quick_hash = ?2,
                                full_hash = CASE WHEN ?4 THEN ?2 WHEN hash_scan = ?3 THEN full_hash END,
                                hash_scan = ?3
                         WHERE id = ?1",
                        params![id, hash, changed_scan, whole],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE files SET full_hash = ?2 WHERE id = ?1 AND hash_scan = ?3",
                        params![id, hash, changed_scan],
                    )?;
                }
            }
            tx.commit()?;
            progress(&Progress { stage, done: done.load(Ordering::Relaxed), total });
            Ok(())
        };
        for item in rx {
            batch.push(item);
            if batch.len() >= 200 {
                flush(&mut batch)?;
            }
        }
        flush(&mut batch)
    })
}

/// BLAKE3 of the size and the first and last 64 KB (the whole file when it is small).
fn quick_hash(path: &Path, expected_size: u64) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    anyhow::ensure!(size == expected_size, "file changed since it was catalogued");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());
    let mut buf = vec![0u8; QUICK_CHUNK as usize];
    if size <= 2 * QUICK_CHUNK {
        let mut all = Vec::with_capacity(size as usize);
        file.read_to_end(&mut all)?;
        hasher.update(&all);
    } else {
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
        file.seek(SeekFrom::Start(size - QUICK_CHUNK))?;
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    }
    Ok(hasher.finalize().to_hex()[..32].to_string())
}

/// BLAKE3 of the whole file (with its size, like the quick hash).
fn full_hash(path: &Path, expected_size: u64, cancel: &AtomicBool) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    anyhow::ensure!(size == expected_size, "file changed since it was catalogued");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "cancelled");
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Computes missing thumbnail fingerprints from the thumbnail cache (0 = no usable fingerprint).
fn backfill_fingerprints(conn: &mut Connection, catalog_dir: &Path, cancel: &AtomicBool, progress: &(dyn Fn(&Progress) + Sync)) -> Result<()> {
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM files WHERE missing = 0 AND thumb_ok = 1 AND phash IS NULL AND kind <> ?1 ORDER BY id")?
        .query_map([MediaKind::Video.as_i64()], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let total = ids.len() as u64;
    progress(&Progress { stage: Stage::Fingerprints, done: 0, total });
    let mut done = 0u64;
    for chunk in ids.chunks(512) {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let next = AtomicU64::new(0);
        let results = std::sync::Mutex::new(Vec::with_capacity(chunk.len()));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed) as usize;
                    let Some(&id) = chunk.get(i) else { break };
                    let hash = std::fs::read(thumbs::thumb_path(catalog_dir, id)).ok().map(|jpeg| thumbs::dhash(&jpeg).unwrap_or(0));
                    results.lock().unwrap().push((id, hash));
                });
            }
        });
        let tx = conn.transaction()?;
        for (id, hash) in results.into_inner().unwrap() {
            if let Some(hash) = hash {
                tx.execute("UPDATE files SET phash = ?2 WHERE id = ?1", params![id, hash])?;
            }
        }
        tx.commit()?;
        done += chunk.len() as u64;
        progress(&Progress { stage: Stage::Fingerprints, done, total });
    }
    Ok(())
}

fn group_similar(conn: &mut Connection, progress: &(dyn Fn(&Progress) + Sync)) -> Result<()> {
    let items: Vec<(i64, u64, Option<String>)> = conn
        .prepare(
            "SELECT id, phash, CASE WHEN hash_scan = changed_scan THEN full_hash END FROM files
             WHERE missing = 0 AND phash IS NOT NULL AND phash <> 0
               AND (width IS NULL OR max(width, height) >= ?1)",
        )?
        .query_map([MIN_SIMILAR_SIDE], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let total = items.len() as u64;
    progress(&Progress { stage: Stage::Comparing, done: 0, total });

    let mut sets = DisjointSet::new(items.len());
    for (band, (shift, bits)) in BANDS.iter().enumerate() {
        let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, (_, hash, _)) in items.iter().enumerate() {
            buckets.entry((hash >> shift) & ((1 << bits) - 1)).or_default().push(i);
        }
        for members in buckets.values().filter(|m| m.len() >= 2 && m.len() <= MAX_BUCKET) {
            for (a_pos, &a) in members.iter().enumerate() {
                for &b in &members[a_pos + 1..] {
                    let (ia, ib) = (&items[a], &items[b]);
                    // Byte-identical copies are already in the exact list.
                    let identical = ia.2.is_some() && ia.2 == ib.2;
                    if !identical && (ia.1 ^ ib.1).count_ones() <= MAX_DISTANCE {
                        sets.union(a, b);
                    }
                }
            }
        }
        progress(&Progress { stage: Stage::Comparing, done: total * (band as u64 + 1) / BANDS.len() as u64, total });
    }

    let mut groups: HashMap<usize, Vec<i64>> = HashMap::new();
    for (i, (id, _, _)) in items.iter().enumerate() {
        groups.entry(sets.find(i)).or_default().push(*id);
    }
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM similar_members", [])?;
    {
        let mut insert = tx.prepare("INSERT INTO similar_members (group_id, file_id) VALUES (?1, ?2)")?;
        for (group_id, members) in groups.values().filter(|m| m.len() >= 2).enumerate() {
            for id in members {
                insert.execute(params![group_id as i64, id])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

const EXACT_GROUPS: &str = "SELECT full_hash, size, COUNT(*) AS n FROM files
    WHERE missing = 0 AND full_hash IS NOT NULL AND hash_scan = changed_scan
    GROUP BY full_hash HAVING n > 1";

const SIMILAR_GROUPS: &str = "SELECT s.group_id, COUNT(*) AS n FROM similar_members s
    JOIN files f ON f.id = s.file_id AND f.missing = 0 GROUP BY s.group_id HAVING n > 1";

pub fn summary(conn: &Connection) -> Result<Summary> {
    let (exact_groups, exact_wasted): (i64, i64) = conn.query_row(
        &format!("SELECT COUNT(*), COALESCE(SUM(size * (n - 1)), 0) FROM ({EXACT_GROUPS})"),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let similar_groups = conn.query_row(&format!("SELECT COUNT(*) FROM ({SIMILAR_GROUPS})"), [], |r| r.get(0))?;
    // Same-size candidates never quick-hashed are on drives that were unplugged.
    let unchecked_offline = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE missing = 0 AND size > 0 AND NOT (hash_scan = changed_scan AND quick_hash IS NOT NULL)
           AND size IN (SELECT size FROM files WHERE missing = 0 AND size > 0 GROUP BY size HAVING COUNT(*) > 1)",
        [],
        |r| r.get(0),
    )?;
    Ok(Summary { exact_groups, exact_wasted, similar_groups, unchecked_offline })
}

/// A page of groups: exact copies by most space wasted; similar images by group size.
/// Within a group the suggested keeper comes first (oldest copy, or the largest image).
pub fn groups(conn: &Connection, kind: GroupKind, offset: i64, limit: i64) -> Result<GroupPage> {
    let cols = query::ROW_COLUMNS;
    let (count_sql, page_sql, members_sql) = match kind {
        GroupKind::Exact => (
            format!("SELECT COUNT(*) FROM ({EXACT_GROUPS})"),
            format!("{EXACT_GROUPS} ORDER BY size * (n - 1) DESC, full_hash LIMIT ?1 OFFSET ?2"),
            format!(
                "SELECT {cols} FROM files WHERE full_hash = ?1 AND missing = 0 AND hash_scan = changed_scan
                 ORDER BY mtime, id"
            ),
        ),
        GroupKind::Similar => (
            format!("SELECT COUNT(*) FROM ({SIMILAR_GROUPS})"),
            format!("{SIMILAR_GROUPS} ORDER BY n DESC, s.group_id LIMIT ?1 OFFSET ?2"),
            format!(
                "SELECT {cols} FROM files WHERE missing = 0 AND id IN (SELECT file_id FROM similar_members WHERE group_id = ?1)
                 ORDER BY COALESCE(width * height, 0) DESC, size DESC, id"
            ),
        ),
    };
    let total = conn.query_row(&count_sql, [], |r| r.get(0))?;
    let keys: Vec<String> = conn
        .prepare(&page_sql)?
        .query_map(params![limit, offset], |r| {
            Ok(match r.get_ref(0)? {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                v => v.as_str().unwrap_or_default().to_string(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut members = conn.prepare(&members_sql)?;
    let mut groups = Vec::with_capacity(keys.len());
    for key in keys {
        let files: Vec<FileRow> = if kind == GroupKind::Similar {
            members.query_map([key.parse::<i64>().unwrap_or(-1)], query::map_row)?.collect::<rusqlite::Result<_>>()?
        } else {
            members.query_map([&key], query::map_row)?.collect::<rusqlite::Result<_>>()?
        };
        let wasted = files.iter().skip(1).map(|f| f.size).sum();
        groups.push(Group { key, files, wasted });
    }
    Ok(GroupPage { total, groups })
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashReport {
    pub trashed: Vec<i64>,
    pub freed: i64,
    /// (file id, reason) for files that were left alone.
    pub skipped: Vec<(i64, String)>,
}

/// Moves files to the system trash (restorable). A file is left alone if it would be the last
/// one of its group, if its drive is unplugged, or if it changed since it was catalogued.
pub fn trash(conn: &Connection, ids: &[i64], resolve: &dyn Fn(i64) -> Option<PathBuf>) -> Result<TrashReport> {
    let chosen: std::collections::HashSet<i64> = ids.iter().copied().collect();
    let mut report = TrashReport::default();
    for &id in ids {
        let (size, mtime, full_hash): (i64, i64, Option<String>) = conn
            .query_row(
                "SELECT size, mtime, CASE WHEN hash_scan = changed_scan THEN full_hash END FROM files WHERE id = ?1 AND missing = 0",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .with_context(|| format!("file {id} is not in the catalog"))?;

        // Some other member of one of its groups must stay.
        let mut others: Vec<i64> = Vec::new();
        if let Some(hash) = &full_hash {
            others.extend(
                conn.prepare_cached("SELECT id FROM files WHERE full_hash = ?1 AND missing = 0 AND hash_scan = changed_scan AND id <> ?2")?
                    .query_map(params![hash, id], |r| r.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        others.extend(
            conn.prepare_cached(
                "SELECT s2.file_id FROM similar_members s1 JOIN similar_members s2 ON s2.group_id = s1.group_id
                 JOIN files f ON f.id = s2.file_id AND f.missing = 0
                 WHERE s1.file_id = ?1 AND s2.file_id <> ?1",
            )?
            .query_map([id], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        );
        if !others.iter().any(|o| !chosen.contains(o)) {
            report.skipped.push((id, "it would remove the last copy".into()));
            continue;
        }

        let Some(path) = resolve(id) else {
            report.skipped.push((id, "its drive is not plugged in".into()));
            continue;
        };
        let unchanged = std::fs::metadata(&path).is_ok_and(|m| {
            let modified = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
            m.len() as i64 == size && modified == Some(mtime)
        });
        if !unchanged {
            report.skipped.push((id, "it changed since it was catalogued".into()));
            continue;
        }
        match trash::delete(&path) {
            Ok(()) => {
                conn.execute("UPDATE files SET missing = 1 WHERE id = ?1", [id])?;
                report.trashed.push(id);
                report.freed += size;
            }
            Err(e) => report.skipped.push((id, format!("could not move it to the trash: {e}"))),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    use std::fs;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Connection, Target) {
        let catalog = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        let conn = db::open(catalog.path()).unwrap();
        conn.execute("INSERT INTO volumes (id, uuid, label, added_at) VALUES (1, 't', 'T', 0)", []).unwrap();
        let target = Target { volume_id: 1, mount: drive.path().to_path_buf(), disk: "test".into(), rotational: false };
        (catalog, drive, conn, target)
    }

    fn scan(conn: &mut Connection, drive: &Path) {
        scanner::scan_volume(conn, 1, drive, &["".into()], false, &AtomicBool::new(false), |_, _| {}).unwrap();
    }

    fn noop() -> impl Fn(&Progress) + Sync {
        |_: &Progress| {}
    }

    #[test]
    fn finds_exact_copies_in_stages() {
        let (catalog, drive, mut conn, target) = setup();
        let d = drive.path();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut big_other = big.clone();
        big_other[150_000] ^= 1; // same size, same head and tail, different middle
        fs::create_dir_all(d.join("a")).unwrap();
        fs::write(d.join("a/one.jpg"), &big).unwrap();
        fs::write(d.join("two.jpg"), &big).unwrap();
        fs::write(d.join("three.jpg"), &big_other).unwrap();
        fs::write(d.join("small1.png"), b"tiny").unwrap();
        fs::write(d.join("small2.png"), b"tiny").unwrap();
        fs::write(d.join("unique.png"), b"only one of this size!").unwrap();
        scan(&mut conn, d);

        let summary = run(catalog.path(), &[target], &AtomicBool::new(false), &noop()).unwrap();
        assert_eq!(summary.exact_groups, 2);
        assert_eq!(summary.exact_wasted, 300_000 + 4);
        assert_eq!(summary.unchecked_offline, 0);

        // The file with a unique size was never read.
        let unique_hashed: Option<String> =
            conn.query_row("SELECT quick_hash FROM files WHERE name = 'unique.png'", [], |r| r.get(0)).unwrap();
        assert_eq!(unique_hashed, None);

        let page = groups(&conn, GroupKind::Exact, 0, 10).unwrap();
        assert_eq!(page.total, 2);
        let names: Vec<&str> = page.groups[0].files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"one.jpg") && names.contains(&"two.jpg") && !names.contains(&"three.jpg"));
        assert_eq!(page.groups[0].wasted, 300_000);
    }

    #[test]
    fn changed_file_loses_its_hash() {
        let (catalog, drive, mut conn, target) = setup();
        let d = drive.path();
        fs::write(d.join("x.jpg"), b"same").unwrap();
        fs::write(d.join("y.jpg"), b"same").unwrap();
        scan(&mut conn, d);
        assert_eq!(run(catalog.path(), std::slice::from_ref(&target), &AtomicBool::new(false), &noop()).unwrap().exact_groups, 1);

        std::thread::sleep(std::time::Duration::from_millis(1100)); // new mtime second
        fs::write(d.join("y.jpg"), b"diff").unwrap();
        scan(&mut conn, d);
        assert_eq!(summary(&conn).unwrap().exact_groups, 0, "stale hash must not count");
        assert_eq!(run(catalog.path(), &[target], &AtomicBool::new(false), &noop()).unwrap().exact_groups, 0);
    }

    #[test]
    fn offline_files_count_once_hashed() {
        let (catalog, drive, mut conn, target) = setup();
        fs::write(drive.path().join("p.jpg"), b"copy").unwrap();
        fs::write(drive.path().join("q.jpg"), b"copy").unwrap();
        scan(&mut conn, drive.path());

        // Unplugged before any hashing: the candidates are reported as unchecked.
        let s = run(catalog.path(), &[], &AtomicBool::new(false), &noop()).unwrap();
        assert_eq!((s.exact_groups, s.unchecked_offline), (0, 2));
        // Hashed while plugged in, the group stays after unplugging.
        run(catalog.path(), &[target], &AtomicBool::new(false), &noop()).unwrap();
        assert_eq!(run(catalog.path(), &[], &AtomicBool::new(false), &noop()).unwrap().exact_groups, 1);
    }

    #[test]
    fn groups_similar_fingerprints() {
        let (catalog, _drive, mut conn, _target) = setup();
        let insert = |id: i64, phash: i64, w: i64| {
            conn.execute(
                "INSERT INTO files (id, volume_id, rel_path, dir, name, ext, kind, size, mtime, first_scan, changed_scan, seen_scan, width, height, phash)
                 VALUES (?1, 1, ?2, '', ?2, 'jpg', 1, ?3, 0, 1, 1, 1, ?3, ?3, ?4)",
                params![id, format!("f{id}.jpg"), w, phash],
            )
            .unwrap();
        };
        let base: i64 = 0x5A5A_F0F0_1234_ABCD;
        insert(1, base, 4000);
        insert(2, base ^ 0b1011, 1000); // 3 bits apart: a resized copy
        insert(3, base ^ 0x1F, 2000); // 5 bits from 1, but 2 bits from 2: joins through it
        insert(4, !base, 3000); // unrelated
        insert(5, base, 100); // an icon: too small to compare
        group_similar(&mut conn, &noop()).unwrap();

        let page = groups(&conn, GroupKind::Similar, 0, 10).unwrap();
        assert_eq!(page.total, 1);
        let ids: Vec<i64> = page.groups[0].files.iter().map(|f| f.id).collect();
        assert_eq!(ids, [1, 3, 2], "largest image first");
        let _ = catalog;
    }

    /// Manual check that the system trash works on a real drive (e.g. an NTFS one):
    /// `MC_TRASH_DIR=/run/media/me/Disk2/tmp cargo test trash_on_real_drive -- --ignored --nocapture`
    /// Creates a throwaway file there and moves it to the trash; restore or empty it afterwards.
    #[test]
    #[ignore]
    fn trash_on_real_drive() {
        let dir = PathBuf::from(std::env::var("MC_TRASH_DIR").unwrap());
        let file = dir.join("rummage-trash-test.txt");
        fs::write(&file, b"throwaway test file").unwrap();
        trash::delete(&file).unwrap();
        assert!(!file.exists());
        println!("moved {} to the trash", file.display());
    }

    #[test]
    fn trash_keeps_the_last_copy() {
        let (catalog, drive, mut conn, target) = setup();
        let d = drive.path();
        fs::write(d.join("keep.jpg"), b"photo").unwrap();
        fs::write(d.join("extra.jpg"), b"photo").unwrap();
        scan(&mut conn, d);
        run(catalog.path(), &[target], &AtomicBool::new(false), &noop()).unwrap();
        let id = |name: &str| -> i64 { conn.query_row("SELECT id FROM files WHERE name = ?1", [name], |r| r.get(0)).unwrap() };
        let (keep, extra) = (id("keep.jpg"), id("extra.jpg"));
        let mount = d.to_path_buf();
        let resolve = move |file: i64| -> Option<PathBuf> {
            Some(mount.join(if file == keep { "keep.jpg" } else { "extra.jpg" }))
        };

        // Choosing every copy: refused for all of them, nothing deleted.
        let report = trash(&conn, &[keep, extra], &resolve).unwrap();
        assert!(report.trashed.is_empty());
        assert_eq!(report.skipped.len(), 2);
        assert!(d.join("keep.jpg").exists() && d.join("extra.jpg").exists());
    }
}
