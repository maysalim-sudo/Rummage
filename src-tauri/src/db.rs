//! Catalog database: connection setup and schema migrations.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DB_FILE: &str = "catalog.sqlite";

/// Each entry upgrades the schema by one version (tracked in `PRAGMA user_version`).
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE volumes (
    id            INTEGER PRIMARY KEY,
    uuid          TEXT    NOT NULL UNIQUE,
    label         TEXT    NOT NULL,
    fs_type       TEXT    NOT NULL DEFAULT '',
    last_mount    TEXT,
    total_bytes   INTEGER NOT NULL DEFAULT 0,
    free_bytes    INTEGER NOT NULL DEFAULT 0,
    rotational    INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL,
    last_seen_at  INTEGER,
    last_scan_at  INTEGER
);

-- Folders (relative to the volume root) that get scanned; '' means the whole volume.
CREATE TABLE scan_roots (
    id         INTEGER PRIMARY KEY,
    volume_id  INTEGER NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
    rel_path   TEXT    NOT NULL,
    UNIQUE (volume_id, rel_path)
);

CREATE TABLE scans (
    id           INTEGER PRIMARY KEY,
    volume_id    INTEGER NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER,
    status       TEXT    NOT NULL,
    files_seen   INTEGER NOT NULL DEFAULT 0,
    added        INTEGER NOT NULL DEFAULT 0,
    changed      INTEGER NOT NULL DEFAULT 0,
    removed      INTEGER NOT NULL DEFAULT 0,
    error        TEXT
);

-- first_scan / changed_scan / seen_scan hold scan ids: background jobs (metadata,
-- thumbnails, hashes) pick up files whose changed_scan is newer than their own marker.
CREATE TABLE files (
    id            INTEGER PRIMARY KEY,
    volume_id     INTEGER NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
    rel_path      TEXT    NOT NULL,
    dir           TEXT    NOT NULL,
    name          TEXT    NOT NULL,
    ext           TEXT    NOT NULL,
    kind          INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    first_scan    INTEGER NOT NULL,
    changed_scan  INTEGER NOT NULL,
    seen_scan     INTEGER NOT NULL,
    missing       INTEGER NOT NULL DEFAULT 0,
    UNIQUE (volume_id, rel_path)
);
CREATE INDEX files_kind  ON files(kind, mtime);
CREATE INDEX files_mtime ON files(mtime);
CREATE INDEX files_dir   ON files(volume_id, dir);
CREATE INDEX files_seen  ON files(volume_id, seen_scan);

CREATE VIRTUAL TABLE files_fts USING fts5(
    name, dir,
    content = 'files', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER files_fts_ai AFTER INSERT ON files BEGIN
    INSERT INTO files_fts(rowid, name, dir) VALUES (new.id, new.name, new.dir);
END;
CREATE TRIGGER files_fts_ad AFTER DELETE ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, name, dir) VALUES ('delete', old.id, old.name, old.dir);
END;
CREATE TRIGGER files_fts_au AFTER UPDATE OF name, dir ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, name, dir) VALUES ('delete', old.id, old.name, old.dir);
    INSERT INTO files_fts(rowid, name, dir) VALUES (new.id, new.name, new.dir);
END;
"#];

/// Opens (creating if needed) the catalog in `dir` and brings its schema up to date.
pub fn open(dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("cannot create catalog folder {}", dir.display()))?;
    let mut conn = Connection::open(dir.join(DB_FILE)).context("cannot open catalog database")?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("catalog migration {} failed", i + 1))?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_migrates_twice() {
        let dir = tempfile::tempdir().unwrap();
        drop(open(dir.path()).unwrap());
        let conn = open(dir.path()).unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }
}
