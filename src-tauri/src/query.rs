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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileQuery {
    pub text: Option<String>,
    pub volume_id: Option<i64>,
    pub kind: Option<MediaKind>,
    pub include_missing: bool,
    pub sort: Sort,
    /// Minimum length of the longer side, in pixels.
    pub min_side: Option<i64>,
    pub orientation: Option<Orientation>,
    /// Unix seconds, inclusive; compared with the date taken (or modified).
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub camera: Option<String>,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
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

const ROW_COLUMNS: &str = "id, volume_id, rel_path, dir, name, ext, kind, size, mtime, missing,
    width, height, sort_date, duration_ms, camera, CASE WHEN thumb_ok THEN processed_scan END";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<FileRow> {
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

pub fn query(conn: &Connection, q: &FileQuery, offset: i64, limit: i64) -> Result<Page> {
    let mut conditions: Vec<&str> = Vec::new();
    let mut args: Vec<Value> = Vec::new();

    if !q.include_missing {
        conditions.push("missing = 0");
    }
    if let Some(volume_id) = q.volume_id {
        conditions.push("volume_id = ?");
        args.push(volume_id.into());
    }
    if let Some(kind) = q.kind {
        conditions.push("kind = ?");
        args.push(kind.as_i64().into());
    }
    if let Some(fts) = q.text.as_deref().and_then(fts_query) {
        conditions.push("id IN (SELECT rowid FROM files_fts WHERE files_fts MATCH ?)");
        args.push(fts.into());
    }
    if let Some(min_side) = q.min_side {
        conditions.push("max(width, height) >= ?");
        args.push(min_side.into());
    }
    match q.orientation {
        Some(Orientation::Landscape) => conditions.push("width > height"),
        Some(Orientation::Portrait) => conditions.push("height > width"),
        Some(Orientation::Square) => conditions.push("width = height"),
        None => {}
    }
    if let Some(from) = q.date_from {
        conditions.push("sort_date >= ?");
        args.push(from.into());
    }
    if let Some(to) = q.date_to {
        conditions.push("sort_date <= ?");
        args.push(to.into());
    }
    if let Some(camera) = q.camera.as_ref().filter(|c| !c.is_empty()) {
        conditions.push("camera = ?");
        args.push(camera.clone().into());
    }
    if let Some(min) = q.min_duration_ms {
        conditions.push("duration_ms >= ?");
        args.push(min.into());
    }
    if let Some(max) = q.max_duration_ms {
        conditions.push("duration_ms <= ?");
        args.push(max.into());
    }
    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
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

/// Cameras in the catalog, most used first.
pub fn cameras(conn: &Connection) -> Result<Vec<CameraCount>> {
    let mut stmt = conn.prepare(
        "SELECT camera, COUNT(*) FROM files WHERE camera IS NOT NULL AND missing = 0
         GROUP BY camera ORDER BY 2 DESC LIMIT 300",
    )?;
    let list = stmt
        .query_map([], |r| Ok(CameraCount { camera: r.get(0)?, count: r.get(1)? }))?
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

        let videos = query(&conn, &FileQuery { kind: Some(MediaKind::Video), ..Default::default() }, 0, 50).unwrap();
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
    fn metadata_filters() {
        let (_d, conn) = catalog();
        conn.execute_batch(
            "UPDATE files SET width = 4000, height = 3000, camera = 'Canon EOS R5', taken_at = 5 WHERE name = 'beach_sunset.jpg';
             UPDATE files SET width = 1080, height = 1920, duration_ms = 45000 WHERE name = 'beach.mp4';
             UPDATE files SET width = 6000, height = 6000, camera = 'NIKON Z 6' WHERE name = 'DSC_0001.NEF';",
        )
        .unwrap();
        let count = |q: FileQuery| query(&conn, &q, 0, 50).unwrap().total;

        assert_eq!(count(FileQuery { min_side: Some(3840), ..Default::default() }), 2);
        assert_eq!(count(FileQuery { orientation: Some(Orientation::Portrait), ..Default::default() }), 1);
        assert_eq!(count(FileQuery { orientation: Some(Orientation::Square), ..Default::default() }), 1);
        assert_eq!(count(FileQuery { camera: Some("Canon EOS R5".into()), ..Default::default() }), 1);
        assert_eq!(count(FileQuery { min_duration_ms: Some(30_000), ..Default::default() }), 1);
        // Date taken (5) wins over mtime (30) for the sunset photo.
        assert_eq!(count(FileQuery { date_to: Some(9), ..Default::default() }), 1);

        let oldest = query(&conn, &FileQuery { sort: Sort::Oldest, ..Default::default() }, 0, 1).unwrap();
        assert_eq!(oldest.rows[0].name, "beach_sunset.jpg");

        let cams = cameras(&conn).unwrap();
        assert_eq!(cams.len(), 2);

        let id = oldest.rows[0].id;
        let d = details(&conn, id).unwrap();
        assert_eq!(d.taken_at, Some(5));
        assert!(!d.processed);
    }
}
