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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileQuery {
    pub text: Option<String>,
    pub volume_id: Option<i64>,
    pub kind: Option<MediaKind>,
    pub include_missing: bool,
    pub sort: Sort,
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
    /// Absolute path when the drive is plugged in; filled in by the command layer.
    pub abs_path: Option<String>,
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
    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let order_sql = match q.sort {
        Sort::Newest => "mtime DESC, id DESC",
        Sort::Oldest => "mtime ASC, id ASC",
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
        "SELECT id, volume_id, rel_path, dir, name, ext, kind, size, mtime, missing
         FROM files {where_sql} ORDER BY {order_sql} LIMIT ? OFFSET ?"
    ))?;
    let rows = stmt
        .query_map(params_from_iter(page_args.iter()), |r| {
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
                abs_path: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Page { total, rows })
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
}
