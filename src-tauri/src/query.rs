//! Catalog queries for the file browser.

use crate::media_kind::MediaKind;
use anyhow::Result;
use rusqlite::{params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Name,
    Largest,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Landscape,
    Portrait,
    Square,
}

/// A numeric range: `min` inclusive, `max` exclusive; either end may be open.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Range {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

/// File filters. Each list filter matches any of its values; an empty list means "any".
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileQuery {
    pub text: Option<String>,
    pub volume_id: Option<i64>,
    pub kinds: Vec<MediaKind>,
    pub include_missing: bool,
    pub sort: Sort,
    /// Ranges for the longer side, in pixels.
    pub side_ranges: Vec<Range>,
    pub orientations: Vec<Orientation>,
    /// Unix seconds, inclusive; compared with the date taken (or modified).
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub cameras: Vec<String>,
    pub duration_ranges: Vec<Range>,
    /// Volume-relative folder ('' = drive root); only used together with `volume_id`.
    pub folder: Option<String>,
    /// Only files directly in `folder`, not in its subfolders.
    pub direct_only: bool,
}

/// One subfolder in the folder tree.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub name: String,
    /// Volume-relative path of the folder.
    pub path: String,
    /// Media files in this folder and all its subfolders.
    pub count: i64,
    pub has_children: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub id: i64,
    pub volume_id: i64,
    pub rel_path: String,
    pub dir: String,
    pub name: String,
    pub ext: String,
    pub kind: Option<MediaKind>,
    pub size: i64,
    pub mtime: i64,
    pub missing: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// Date taken, or the modification time when unknown.
    pub date: i64,
    pub duration_ms: Option<i64>,
    pub camera: Option<String>,
    /// Thumbnail version for cache-busting URLs; None when there is no thumbnail.
    pub thumb: Option<i64>,
    /// Absolute path when the drive is plugged in; filled in by the command layer.
    pub abs_path: Option<String>,
}

/// Everything known about one file, for the inspector panel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetails {
    #[serde(flatten)]
    pub row: FileRow,
    pub taken_at: Option<i64>,
    pub lens: Option<String>,
    pub iso: Option<i64>,
    pub f_number: Option<f64>,
    pub exposure: Option<String>,
    pub focal_mm: Option<f64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
    pub processed: bool,
    pub process_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CameraCount {
    pub camera: String,
    pub count: i64,
}

pub(crate) const ROW_COLUMNS: &str = "id, volume_id, rel_path, dir, name, ext, kind, size, mtime, missing,
    width, height, sort_date, duration_ms, camera, CASE WHEN thumb_ok THEN processed_scan END";

pub(crate) fn map_row(r: &rusqlite::Row) -> rusqlite::Result<FileRow> {
    Ok(FileRow {
        id: r.get(0)?,
        volume_id: r.get(1)?,
        rel_path: r.get(2)?,
        dir: r.get(3)?,
        name: r.get(4)?,
        ext: r.get(5)?,
        kind: MediaKind::from_i64(r.get(6)?),
        size: r.get(7)?,
        mtime: r.get(8)?,
        missing: r.get::<_, i64>(9)? != 0,
        width: r.get(10)?,
        height: r.get(11)?,
        date: r.get(12)?,
        duration_ms: r.get(13)?,
        camera: r.get(14)?,
        thumb: r.get(15)?,
        abs_path: None,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Page {
    pub total: i64,
    pub rows: Vec<FileRow>,
}

/// Turns free text into an FTS5 query: every word must match as a prefix.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Builds the `WHERE …` clause (empty if no filters) and its arguments for a query.
fn filter_sql(q: &FileQuery) -> (String, Vec<Value>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();

    if !q.include_missing {
        conditions.push("missing = 0".into());
    }
    if let Some(volume_id) = q.volume_id {
        conditions.push("volume_id = ?".into());
        args.push(volume_id.into());
        match q.folder.as_deref() {
            Some(folder) if q.direct_only => {
                conditions.push("dir = ?".into());
                args.push(folder.to_string().into());
            }
            Some(folder) if !folder.is_empty() => {
                // '0' is the character right after '/', so this range is "folder/…" and uses the index.
                conditions.push("(dir = ? OR (dir >= ? AND dir < ?))".into());
                args.push(folder.to_string().into());
                args.push(format!("{folder}/").into());
                args.push(format!("{folder}0").into());
            }
            _ => {}
        }
    }
    if !q.kinds.is_empty() {
        conditions.push(in_list("kind", q.kinds.len()));
        args.extend(q.kinds.iter().map(|k| Value::from(k.as_i64())));
    }
    if let Some(fts) = q.text.as_deref().and_then(fts_query) {
        conditions.push("id IN (SELECT rowid FROM files_fts WHERE files_fts MATCH ?)".into());
        args.push(fts.into());
    }
    if let Some(sql) = ranges_sql("max(width, height)", &q.side_ranges, &mut args) {
        conditions.push(sql);
    }
    if !q.orientations.is_empty() {
        let any: Vec<&str> = q
            .orientations
            .iter()
            .map(|o| match o {
                Orientation::Landscape => "width > height",
                Orientation::Portrait => "height > width",
                Orientation::Square => "width = height",
            })
            .collect();
        conditions.push(format!("({})", any.join(" OR ")));
    }
    if let Some(from) = q.date_from {
        conditions.push("sort_date >= ?".into());
        args.push(from.into());
    }
    if let Some(to) = q.date_to {
        conditions.push("sort_date <= ?".into());
        args.push(to.into());
    }
    if !q.cameras.is_empty() {
        conditions.push(in_list("camera", q.cameras.len()));
        args.extend(q.cameras.iter().map(|c| Value::from(c.clone())));
    }
    if let Some(sql) = ranges_sql("duration_ms", &q.duration_ranges, &mut args) {
        conditions.push(sql);
    }

    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_sql, args)
}

fn in_list(column: &str, n: usize) -> String {
    format!("{column} IN ({})", vec!["?"; n].join(", "))
}

/// `(expr in range 1 OR expr in range 2 …)`, or None for no ranges.
fn ranges_sql(expr: &str, ranges: &[Range], args: &mut Vec<Value>) -> Option<String> {
    if ranges.is_empty() {
        return None;
    }
    let any: Vec<String> = ranges
        .iter()
        .map(|r| {
            let mut parts = Vec::new();
            if let Some(min) = r.min {
                parts.push(format!("{expr} >= ?"));
                args.push(min.into());
            }
            if let Some(max) = r.max {
                parts.push(format!("{expr} < ?"));
                args.push(max.into());
            }
            if parts.is_empty() {
                format!("{expr} IS NOT NULL")
            } else {
                format!("({})", parts.join(" AND "))
            }
        })
        .collect();
    Some(format!("({})", any.join(" OR ")))
}

pub fn query(conn: &Connection, q: &FileQuery, offset: i64, limit: i64) -> Result<Page> {
    let (where_sql, args) = filter_sql(q);
    let order_sql = match q.sort {
        Sort::Newest => "sort_date DESC, id DESC",
        Sort::Oldest => "sort_date ASC, id ASC",
        Sort::Name => "name COLLATE NOCASE ASC, id ASC",
        Sort::Largest => "size DESC, id DESC",
    };

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM files {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;

    let mut page_args = args.clone();
    page_args.push(limit.into());
    page_args.push(offset.into());
    let mut stmt = conn.prepare(&format!(
        "SELECT {ROW_COLUMNS} FROM files {where_sql} ORDER BY {order_sql} LIMIT ? OFFSET ?"
    ))?;
    let rows = stmt
        .query_map(params_from_iter(page_args.iter()), map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Page { total, rows })
}

pub fn details(conn: &Connection, id: i64) -> Result<FileDetails> {
    Ok(conn.query_row(
        &format!(
            "SELECT {ROW_COLUMNS}, taken_at, lens, iso, f_number, exposure, focal_mm, gps_lat, gps_lon,
                    codec, fps, processed_scan >= changed_scan, process_error
             FROM files WHERE id = ?1"
        ),
        [id],
        |r| {
            Ok(FileDetails {
                row: map_row(r)?,
                taken_at: r.get(16)?,
                lens: r.get(17)?,
                iso: r.get(18)?,
                f_number: r.get(19)?,
                exposure: r.get(20)?,
                focal_mm: r.get(21)?,
                gps_lat: r.get(22)?,
                gps_lon: r.get(23)?,
                codec: r.get(24)?,
                fps: r.get(25)?,
                processed: r.get(26)?,
                process_error: r.get(27)?,
            })
        },
    )?)
}

/// The direct subfolders of `parent` ('' = drive root) that contain media, with counts.
pub fn folders(conn: &Connection, volume_id: i64, parent: &str, include_missing: bool) -> Result<Vec<Folder>> {
    let missing = if include_missing { "" } else { "AND missing = 0" };
    let (range_sql, args): (&str, Vec<Value>) = if parent.is_empty() {
        ("", vec![volume_id.into()])
    } else {
        (
            "AND dir >= ?2 AND dir < ?3",
            vec![volume_id.into(), format!("{parent}/").into(), format!("{parent}0").into()],
        )
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT dir, COUNT(*) FROM files WHERE volume_id = ?1 {missing} {range_sql} GROUP BY dir"
    ))?;
    let dirs = stmt
        .query_map(params_from_iter(args.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let prefix_len = if parent.is_empty() { 0 } else { parent.len() + 1 };
    let mut children: std::collections::BTreeMap<String, Folder> = Default::default();
    for (dir, count) in dirs {
        let rest = &dir[prefix_len.min(dir.len())..];
        if rest.is_empty() {
            continue; // files directly in `parent`
        }
        let (name, deeper) = match rest.split_once('/') {
            Some((name, _)) => (name, true),
            None => (rest, false),
        };
        let child = children.entry(name.to_lowercase() + "\0" + name).or_insert_with(|| Folder {
            name: name.to_string(),
            path: if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") },
            count: 0,
            has_children: false,
        });
        child.count += count;
        child.has_children |= deeper;
    }
    Ok(children.into_values().collect())
}

/// Cameras among the files matching `q` (ignoring its own camera filter), most used first,
/// so the counts match what selecting a camera will show.
pub fn cameras(conn: &Connection, q: &FileQuery) -> Result<Vec<CameraCount>> {
    let q = FileQuery { cameras: Vec::new(), ..q.clone() };
    let (where_sql, args) = filter_sql(&q);
    let joiner = if where_sql.is_empty() { "WHERE" } else { " AND" };
    // Only files with a camera can count, and they are a small part of the catalog: start
    // from that index, or SQLite may read every row matching e.g. "images" (slow on an HDD).
    let mut stmt = conn.prepare(&format!(
        "SELECT camera, COUNT(*) FROM files INDEXED BY files_camera {where_sql}{joiner} camera IS NOT NULL
         GROUP BY camera ORDER BY 2 DESC, 1 LIMIT 500"
    ))?;
    let list = stmt
        .query_map(params_from_iter(args.iter()), |r| Ok(CameraCount { camera: r.get(0)?, count: r.get(1)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn catalog() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open(dir.path()).unwrap();
        conn.execute_batch(
            "INSERT INTO volumes (id, uuid, label, added_at) VALUES (1, 'a', 'A', 0), (2, 'b', 'B', 0);
             INSERT INTO files (volume_id, rel_path, dir, name, ext, kind, size, mtime, first_scan, changed_scan, seen_scan, missing) VALUES
               (1, 'Trips/beach_sunset.jpg', 'Trips', 'beach_sunset.jpg', 'jpg', 1, 100, 30, 1, 1, 1, 0),
               (1, 'Trips/beach.mp4',        'Trips', 'beach.mp4',        'mp4', 2, 900, 20, 1, 1, 1, 0),
               (2, 'raw/DSC_0001.NEF',       'raw',   'DSC_0001.NEF',     'nef', 3, 500, 10, 1, 1, 1, 0),
               (2, 'gone.jpg',               '',      'gone.jpg',         'jpg', 1, 1,   40, 1, 1, 1, 1);",
        )
        .unwrap();
        (dir, conn)
    }

    #[test]
    fn filters_and_sorts() {
        let (_d, conn) = catalog();
        let all = query(&conn, &FileQuery::default(), 0, 50).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.rows[0].name, "beach_sunset.jpg"); // newest first, missing hidden

        let videos = query(&conn, &FileQuery { kinds: vec![MediaKind::Video], ..Default::default() }, 0, 50).unwrap();
        assert_eq!(videos.total, 1);

        let on_b = query(&conn, &FileQuery { volume_id: Some(2), include_missing: true, ..Default::default() }, 0, 50).unwrap();
        assert_eq!(on_b.total, 2);

        let largest = query(&conn, &FileQuery { sort: Sort::Largest, ..Default::default() }, 0, 1).unwrap();
        assert_eq!(largest.rows[0].name, "beach.mp4");
    }

    #[test]
    fn text_search_matches_prefixes_and_folders() {
        let (_d, conn) = catalog();
        let search = |text: &str| {
            query(&conn, &FileQuery { text: Some(text.into()), ..Default::default() }, 0, 50).unwrap().total
        };
        assert_eq!(search("bea"), 2);
        assert_eq!(search("beach suns"), 1);
        assert_eq!(search("trips"), 2);
        assert_eq!(search("DSC_0001"), 1);
        assert_eq!(search("\"\""), 3); // only quotes: no text filter
    }

    #[test]
    fn folder_tree_and_folder_filter() {
        let (_d, conn) = catalog();
        conn.execute_batch(
            "INSERT INTO files (volume_id, rel_path, dir, name, ext, kind, size, mtime, first_scan, changed_scan, seen_scan) VALUES
               (1, 'Trips/2024/Rome/a.jpg', 'Trips/2024/Rome', 'a.jpg', 'jpg', 1, 1, 1, 1, 1, 1),
               (1, 'Trips/2024/b.jpg',      'Trips/2024',      'b.jpg', 'jpg', 1, 1, 1, 1, 1, 1),
               (1, 'Trips2/c.jpg',          'Trips2',          'c.jpg', 'jpg', 1, 1, 1, 1, 1, 1),
               (1, 'root.jpg',              '',                'root.jpg', 'jpg', 1, 1, 1, 1, 1, 1),
               (1, 'archive/x.jpg',         'archive',         'x.jpg', 'jpg', 1, 1, 1, 1, 1, 1);",
        )
        .unwrap();

        let top = folders(&conn, 1, "", false).unwrap();
        let summary: Vec<(&str, i64, bool)> = top.iter().map(|f| (f.name.as_str(), f.count, f.has_children)).collect();
        // Sorted case-insensitively; Trips holds its 2 own files plus 2 in subfolders.
        assert_eq!(summary, [("archive", 1, false), ("Trips", 4, true), ("Trips2", 1, false)]);

        let inner = folders(&conn, 1, "Trips", false).unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!((inner[0].path.as_str(), inner[0].count, inner[0].has_children), ("Trips/2024", 2, true));

        let count = |folder: &str, direct_only: bool| {
            let q = FileQuery { volume_id: Some(1), folder: Some(folder.into()), direct_only, ..Default::default() };
            query(&conn, &q, 0, 50).unwrap().total
        };
        assert_eq!(count("Trips", false), 4); // not Trips2
        assert_eq!(count("Trips", true), 2);
        assert_eq!(count("Trips/2024", true), 1);
        assert_eq!(count("", true), 1); // files at the drive root
        assert_eq!(count("", false), 7);
    }

    #[test]
    fn metadata_filters() {
        let (_d, conn) = catalog();
        conn.execute_batch(
            "UPDATE files SET width = 4000, height = 3000, camera = 'Canon EOS R5', taken_at = 5 WHERE name = 'beach_sunset.jpg';
             UPDATE files SET width = 1080, height = 1920, duration_ms = 45000 WHERE name = 'beach.mp4';
             UPDATE files SET width = 6000, height = 6000, camera = 'NIKON Z 6' WHERE name = 'DSC_0001.NEF';",
        )
        .unwrap();
        let count = |q: FileQuery| query(&conn, &q, 0, 50).unwrap().total;

        let range = |min, max| Range { min, max };
        assert_eq!(count(FileQuery { side_ranges: vec![range(Some(3840), None)], ..Default::default() }), 2);
        // Several ranges: Full HD-only (1920 to under 3840) or very large (6000+).
        let sides = vec![range(Some(1920), Some(3840)), range(Some(6000), None)];
        assert_eq!(count(FileQuery { side_ranges: sides, ..Default::default() }), 2);
        assert_eq!(count(FileQuery { orientations: vec![Orientation::Portrait], ..Default::default() }), 1);
        let both = vec![Orientation::Portrait, Orientation::Square];
        assert_eq!(count(FileQuery { orientations: both, ..Default::default() }), 2);
        assert_eq!(count(FileQuery { cameras: vec!["Canon EOS R5".into()], ..Default::default() }), 1);
        let cams = vec!["Canon EOS R5".into(), "NIKON Z 6".into()];
        assert_eq!(count(FileQuery { cameras: cams, ..Default::default() }), 2);
        assert_eq!(count(FileQuery { kinds: vec![MediaKind::Video, MediaKind::Raw], ..Default::default() }), 2);
        let long = vec![range(Some(30_000), None)];
        assert_eq!(count(FileQuery { duration_ranges: long, ..Default::default() }), 1);
        let short = vec![range(None, Some(30_000))];
        assert_eq!(count(FileQuery { duration_ranges: short, ..Default::default() }), 0);
        // Date taken (5) wins over mtime (30) for the sunset photo.
        assert_eq!(count(FileQuery { date_to: Some(9), ..Default::default() }), 1);

        let oldest = query(&conn, &FileQuery { sort: Sort::Oldest, ..Default::default() }, 0, 1).unwrap();
        assert_eq!(oldest.rows[0].name, "beach_sunset.jpg");

        let cams = cameras(&conn, &FileQuery::default()).unwrap();
        assert_eq!(cams.len(), 2);
        // Counts follow the other filters (drive 1 has only the Canon), not the camera filter itself.
        let on_drive_1 = FileQuery { volume_id: Some(1), cameras: vec!["NIKON Z 6".into()], ..Default::default() };
        let cams = cameras(&conn, &on_drive_1).unwrap();
        assert_eq!(cams.iter().map(|c| (c.camera.as_str(), c.count)).collect::<Vec<_>>(), [("Canon EOS R5", 1)]);

        let id = oldest.rows[0].id;
        let d = details(&conn, id).unwrap();
        assert_eq!(d.taken_at, Some(5));
        assert!(!d.processed);
    }
}
