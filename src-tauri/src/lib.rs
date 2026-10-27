mod config;
mod db;
mod dupes;
mod jobs;
mod media_kind;
mod meta;
mod paths;
mod query;
mod scanner;
mod thumbs;
mod tools;
mod video;
mod volumes;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

struct Catalog {
    dir: PathBuf,
    conn: Connection,
}

#[derive(Clone)]
struct Mounted {
    path: PathBuf,
    disk: String,
    rotational: bool,
}

#[derive(Default)]
struct AppState {
    catalog: Mutex<Option<Catalog>>,
    /// Cancel flags of running scans, by volume id.
    scans: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// Where each catalogued volume is mounted right now (refreshed by `list_volumes`).
    mounts: Mutex<HashMap<i64, Mounted>>,
    jobs: Arc<jobs::Jobs>,
    /// Local HTTP server for the video player; started once at launch.
    media_server: OnceLock<video::MediaServer>,
    /// Cancel flag of the running duplicate search, if any.
    dupe_search: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    fn load() -> Self {
        let state = Self::default();
        if let Some(dir) = config::load().catalog_dir {
            match db::open(&dir) {
                Ok(conn) => *state.catalog.lock().unwrap() = Some(Catalog { dir, conn }),
                Err(e) => eprintln!("cannot open catalog: {e:#}"),
            }
        }
        state
    }

    fn catalog(&self) -> CmdResult<MutexGuard<'_, Option<Catalog>>> {
        let guard = self.catalog.lock().map_err(err)?;
        if guard.is_none() {
            return Err("catalog location is not set up yet".into());
        }
        Ok(guard)
    }

    fn catalog_dir(&self) -> Option<PathBuf> {
        self.catalog.lock().ok()?.as_ref().map(|c| c.dir.clone())
    }

    fn mount_of(&self, volume_id: i64) -> Option<PathBuf> {
        self.mounts.lock().ok()?.get(&volume_id).map(|m| m.path.clone())
    }
}

/// The plugged-in catalogued drives.
fn online_drives(state: &AppState) -> Vec<jobs::Target> {
    state
        .mounts
        .lock()
        .map(|m| {
            m.iter()
                .map(|(&volume_id, m)| jobs::Target {
                    volume_id,
                    mount: m.path.clone(),
                    disk: m.disk.clone(),
                    rotational: m.rotational,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Starts background processing for every plugged-in drive that is not being scanned.
fn start_jobs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(catalog_dir) = state.catalog_dir() else { return };
    let scanning: Vec<i64> = state.scans.lock().map(|s| s.keys().copied().collect()).unwrap_or_default();
    let targets: Vec<jobs::Target> =
        online_drives(&state).into_iter().filter(|t| !scanning.contains(&t.volume_id)).collect();
    let handle = app.clone();
    let emit: jobs::Emit = Arc::new(move |status| {
        let _ = handle.emit("jobs-progress", status);
    });
    state.jobs.start(&catalog_dir, targets, emit);
}

// ---------- setup ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInfo {
    catalog_dir: Option<String>,
    default_dir: String,
    default_free_bytes: Option<u64>,
}

#[tauri::command]
fn get_setup(state: State<AppState>) -> SetupInfo {
    let default_dir = config::default_catalog_dir();
    let catalog_dir = state
        .catalog
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|c| c.dir.to_string_lossy().to_string()));
    SetupInfo {
        catalog_dir,
        default_free_bytes: free_space(default_dir.to_string_lossy().to_string()),
        default_dir: default_dir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn free_space(path: String) -> Option<u64> {
    // The folder may not exist yet; measure the nearest existing ancestor.
    let mut path = PathBuf::from(path);
    while !path.exists() && path.pop() {}
    volumes::free_space_for(&path)
}

#[tauri::command]
fn set_catalog_dir(state: State<AppState>, path: String) -> CmdResult<()> {
    let dir = PathBuf::from(path);
    let conn = db::open(&dir).map_err(err)?;
    config::save(&config::Config { catalog_dir: Some(dir.clone()) }).map_err(err)?;
    *state.catalog.lock().map_err(err)? = Some(Catalog { dir, conn });
    Ok(())
}

// ---------- volumes ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeView {
    /// None for a detected drive that is not in the catalog yet.
    id: Option<i64>,
    label: String,
    mount: Option<String>,
    online: bool,
    fs_type: String,
    total_bytes: u64,
    free_bytes: u64,
    rotational: bool,
    file_count: i64,
    last_scan_at: Option<i64>,
    scanning: bool,
    roots: Vec<String>,
}

#[tauri::command]
fn list_volumes(app: AppHandle, state: State<AppState>) -> CmdResult<Vec<VolumeView>> {
    let views = refresh_volumes(&state)?;
    // Called every few seconds by the UI, so this also resumes processing when a drive returns.
    start_jobs(&app);
    Ok(views)
}

fn refresh_volumes(state: &AppState) -> CmdResult<Vec<VolumeView>> {
    let detected = volumes::detect();
    let guard = state.catalog()?;
    let conn = &guard.as_ref().unwrap().conn;
    let scanning: Vec<i64> = state.scans.lock().map_err(err)?.keys().copied().collect();

    let counts: HashMap<i64, i64> = conn
        .prepare("SELECT volume_id, COUNT(*) FROM files WHERE missing = 0 GROUP BY volume_id")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(err)?;

    struct Row {
        id: i64,
        uuid: String,
        label: String,
        fs_type: String,
        last_mount: Option<String>,
        total: i64,
        free: i64,
        rotational: bool,
        last_scan_at: Option<i64>,
    }
    let rows: Vec<Row> = conn
        .prepare(
            "SELECT id, uuid, label, fs_type, last_mount, total_bytes, free_bytes, rotational, last_scan_at
             FROM volumes ORDER BY label COLLATE NOCASE",
        )
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    uuid: r.get(1)?,
                    label: r.get(2)?,
                    fs_type: r.get(3)?,
                    last_mount: r.get(4)?,
                    total: r.get(5)?,
                    free: r.get(6)?,
                    rotational: r.get::<_, i64>(7)? != 0,
                    last_scan_at: r.get(8)?,
                })
            })?
            .collect()
        })
        .map_err(err)?;

    let mut mounts = HashMap::new();
    let mut views = Vec::new();
    for row in &rows {
        let live = detected.iter().find(|d| d.uuid.as_deref() == Some(row.uuid.as_str()));
        if let Some(d) = live {
            mounts.insert(
                row.id,
                Mounted { path: d.mount.clone(), disk: d.disk.clone(), rotational: d.rotational },
            );
            conn.execute(
                "UPDATE volumes SET last_mount = ?2, fs_type = ?3, total_bytes = ?4, free_bytes = ?5,
                        rotational = ?6, last_seen_at = ?7 WHERE id = ?1",
                params![
                    row.id,
                    d.mount.to_string_lossy(),
                    d.fs_type,
                    d.total_bytes as i64,
                    d.free_bytes as i64,
                    d.rotational,
                    db::now()
                ],
            )
            .map_err(err)?;
        }
        views.push(VolumeView {
            id: Some(row.id),
            label: row.label.clone(),
            mount: live.map(|d| d.mount.to_string_lossy().to_string()).or(row.last_mount.clone()),
            online: live.is_some(),
            fs_type: live.map(|d| d.fs_type.clone()).unwrap_or(row.fs_type.clone()),
            total_bytes: live.map(|d| d.total_bytes).unwrap_or(row.total as u64),
            free_bytes: live.map(|d| d.free_bytes).unwrap_or(row.free as u64),
            rotational: live.map(|d| d.rotational).unwrap_or(row.rotational),
            file_count: counts.get(&row.id).copied().unwrap_or(0),
            last_scan_at: row.last_scan_at,
            scanning: scanning.contains(&row.id),
            roots: scan_roots(conn, row.id).map_err(err)?,
        });
    }
    *state.mounts.lock().map_err(err)? = mounts;

    for d in detected {
        let known = d.uuid.as_ref().is_some_and(|u| rows.iter().any(|r| &r.uuid == u));
        if !known {
            views.push(VolumeView {
                id: None,
                label: d.label,
                mount: Some(d.mount.to_string_lossy().to_string()),
                online: true,
                fs_type: d.fs_type,
                total_bytes: d.total_bytes,
                free_bytes: d.free_bytes,
                rotational: d.rotational,
                file_count: 0,
                last_scan_at: None,
                scanning: false,
                roots: Vec::new(),
            });
        }
    }
    Ok(views)
}

fn scan_roots(conn: &Connection, volume_id: i64) -> rusqlite::Result<Vec<String>> {
    conn.prepare("SELECT rel_path FROM scan_roots WHERE volume_id = ?1 ORDER BY rel_path")?
        .query_map([volume_id], |r| r.get(0))?
        .collect()
}

/// Adds the drive mounted at `mount` to the catalog and returns its id.
#[tauri::command]
fn add_volume(state: State<AppState>, mount: String) -> CmdResult<i64> {
    let mount = PathBuf::from(mount);
    let detected = volumes::detect()
        .into_iter()
        .find(|d| d.mount == mount)
        .ok_or("that drive is no longer mounted")?;

    let uuid = match &detected.uuid {
        Some(uuid) => {
            // Best effort: the marker lets Windows recognise the drive by the same id.
            if volumes::read_marker(&mount).is_none() {
                let _ = volumes::write_marker(&mount, uuid);
            }
            uuid.clone()
        }
        None => {
            let uuid = uuid::Uuid::new_v4().to_string();
            volumes::write_marker(&mount, &uuid).map_err(err)?;
            uuid
        }
    };

    // On the system drive, scan only the home folder by default.
    let root = dirs::home_dir()
        .filter(|home| home.starts_with(&mount) && home != &mount)
        .and_then(|home| paths::to_rel(&mount, &home))
        .unwrap_or_default();

    let guard = state.catalog()?;
    let conn = &guard.as_ref().unwrap().conn;
    conn.execute(
        "INSERT INTO volumes (uuid, label, fs_type, last_mount, total_bytes, free_bytes, rotational, added_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            uuid,
            detected.label,
            detected.fs_type,
            mount.to_string_lossy(),
            detected.total_bytes as i64,
            detected.free_bytes as i64,
            detected.rotational,
            db::now()
        ],
    )
    .map_err(err)?;
    let id = conn.last_insert_rowid();
    conn.execute("INSERT INTO scan_roots (volume_id, rel_path) VALUES (?1, ?2)", params![id, root])
        .map_err(err)?;
    state.mounts.lock().map_err(err)?.insert(
        id,
        Mounted { path: mount, disk: detected.disk, rotational: detected.rotational },
    );
    Ok(id)
}

#[tauri::command]
fn rename_volume(state: State<AppState>, id: i64, label: String) -> CmdResult<()> {
    let label = label.trim();
    if label.is_empty() {
        return Err("the name cannot be empty".into());
    }
    let guard = state.catalog()?;
    guard
        .as_ref()
        .unwrap()
        .conn
        .execute("UPDATE volumes SET label = ?2 WHERE id = ?1", params![id, label])
        .map_err(err)?;
    Ok(())
}

/// Forgets a drive and everything catalogued on it. Files on the drive are untouched.
#[tauri::command]
fn remove_volume(state: State<AppState>, id: i64) -> CmdResult<()> {
    if let Some(cancel) = state.scans.lock().map_err(err)?.get(&id) {
        cancel.store(true, Ordering::Relaxed);
    }
    let guard = state.catalog()?;
    guard.as_ref().unwrap().conn.execute("DELETE FROM volumes WHERE id = ?1", [id]).map_err(err)?;
    state.mounts.lock().map_err(err)?.remove(&id);
    Ok(())
}

// ---------- scanning ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    volume_id: i64,
    stats: scanner::ScanStats,
    current_dir: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanFinished {
    volume_id: i64,
    status: Option<scanner::ScanStatus>,
    stats: Option<scanner::ScanStats>,
    error: Option<String>,
}

#[tauri::command]
fn start_scan(app: AppHandle, state: State<AppState>, id: i64) -> CmdResult<()> {
    let mount = state.mount_of(id).ok_or("drive is not plugged in")?;
    let (catalog_dir, roots, rotational) = {
        let guard = state.catalog()?;
        let catalog = guard.as_ref().unwrap();
        let rotational: bool = catalog
            .conn
            .query_row("SELECT rotational FROM volumes WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .map_err(err)?
            .ok_or("unknown drive")?;
        (catalog.dir.clone(), scan_roots(&catalog.conn, id).map_err(err)?, rotational)
    };

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut scans = state.scans.lock().map_err(err)?;
        if scans.contains_key(&id) {
            return Err("this drive is already being scanned".into());
        }
        scans.insert(id, cancel.clone());
    }

    std::thread::spawn(move || {
        let result = db::open(&catalog_dir).and_then(|mut conn| {
            scanner::scan_volume(&mut conn, id, &mount, &roots, rotational, &cancel, |stats, dir| {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress { volume_id: id, stats: stats.clone(), current_dir: dir.to_string() },
                );
            })
        });
        let finished = match result {
            Ok((status, stats)) => ScanFinished { volume_id: id, status: Some(status), stats: Some(stats), error: None },
            Err(e) => ScanFinished { volume_id: id, status: None, stats: None, error: Some(err(e)) },
        };
        let state = app.state::<AppState>();
        if let Ok(mut scans) = state.scans.lock() {
            scans.remove(&id);
        }
        state.jobs.mark_dirty(id);
        let _ = app.emit("scan-finished", finished);
        start_jobs(&app);
    });
    Ok(())
}

// ---------- duplicates ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DupesFinished {
    summary: Option<dupes::Summary>,
    error: Option<String>,
    cancelled: bool,
}

#[tauri::command]
fn start_dupe_search(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let catalog_dir = state.catalog_dir().ok_or("catalog location is not set up yet")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.dupe_search.lock().map_err(err)?;
        if running.is_some() {
            return Err("a duplicate search is already running".into());
        }
        *running = Some(cancel.clone());
    }
    let drives = online_drives(&state);
    std::thread::spawn(move || {
        let progress = |p: &dupes::Progress| {
            let _ = app.emit("dupes-progress", p);
        };
        let result = dupes::run(&catalog_dir, &drives, &cancel, &progress);
        let cancelled = cancel.load(Ordering::Relaxed);
        if let Ok(mut running) = app.state::<AppState>().dupe_search.lock() {
            *running = None;
        }
        let finished = match result {
            Ok(summary) => DupesFinished { summary: Some(summary), error: None, cancelled },
            Err(e) => DupesFinished { summary: None, error: Some(err(e)), cancelled },
        };
        let _ = app.emit("dupes-finished", finished);
    });
    Ok(())
}

#[tauri::command]
fn cancel_dupe_search(state: State<AppState>) -> CmdResult<()> {
    if let Some(cancel) = state.dupe_search.lock().map_err(err)?.as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn dupe_summary(state: State<AppState>) -> CmdResult<dupes::Summary> {
    let guard = state.catalog()?;
    dupes::summary(&guard.as_ref().unwrap().conn).map_err(err)
}

#[tauri::command]
fn dupe_groups(state: State<AppState>, kind: dupes::GroupKind, offset: i64, limit: i64) -> CmdResult<dupes::GroupPage> {
    let mut page = {
        let guard = state.catalog()?;
        dupes::groups(&guard.as_ref().unwrap().conn, kind, offset, limit.clamp(1, 200)).map_err(err)?
    };
    let mounts = state.mounts.lock().map_err(err)?;
    for file in page.groups.iter_mut().flat_map(|g| g.files.iter_mut()) {
        fill_abs_path(&mounts, file);
    }
    Ok(page)
}

/// Moves the files to the system trash; the report says which were left alone and why.
#[tauri::command]
fn trash_files(state: State<AppState>, ids: Vec<i64>) -> CmdResult<dupes::TrashReport> {
    let resolve = |id: i64| file_path(&state, id).ok();
    let guard = state.catalog()?;
    dupes::trash(&guard.as_ref().unwrap().conn, &ids, &resolve).map_err(err)
}

#[tauri::command]
fn jobs_status(state: State<AppState>) -> jobs::Status {
    state.jobs.status()
}

#[tauri::command]
fn set_jobs_paused(app: AppHandle, state: State<AppState>, paused: bool) {
    state.jobs.set_paused(paused);
    let _ = app.emit("jobs-progress", state.jobs.status());
}

#[tauri::command]
fn cancel_scan(state: State<AppState>, id: i64) -> CmdResult<()> {
    if let Some(cancel) = state.scans.lock().map_err(err)?.get(&id) {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

// ---------- files ----------

#[tauri::command]
fn query_files(state: State<AppState>, query: query::FileQuery, offset: i64, limit: i64) -> CmdResult<query::Page> {
    let mut page = {
        let guard = state.catalog()?;
        query::query(&guard.as_ref().unwrap().conn, &query, offset, limit.clamp(1, 1000)).map_err(err)?
    };
    let mounts = state.mounts.lock().map_err(err)?;
    for row in &mut page.rows {
        fill_abs_path(&mounts, row);
    }
    Ok(page)
}

fn fill_abs_path(mounts: &HashMap<i64, Mounted>, row: &mut query::FileRow) {
    if let Some(m) = mounts.get(&row.volume_id) {
        row.abs_path = Some(paths::from_rel(&m.path, &row.rel_path).to_string_lossy().to_string());
    }
}

#[tauri::command]
fn file_details(state: State<AppState>, id: i64) -> CmdResult<query::FileDetails> {
    let mut details = {
        let guard = state.catalog()?;
        query::details(&guard.as_ref().unwrap().conn, id).map_err(err)?
    };
    let mounts = state.mounts.lock().map_err(err)?;
    fill_abs_path(&mounts, &mut details.row);
    Ok(details)
}

#[tauri::command]
fn list_folders(
    state: State<AppState>,
    volume_id: i64,
    parent: String,
    include_missing: bool,
) -> CmdResult<Vec<query::Folder>> {
    let guard = state.catalog()?;
    query::folders(&guard.as_ref().unwrap().conn, volume_id, &parent, include_missing).map_err(err)
}

#[tauri::command]
fn list_cameras(state: State<AppState>, query: query::FileQuery) -> CmdResult<Vec<query::CameraCount>> {
    let guard = state.catalog()?;
    query::cameras(&guard.as_ref().unwrap().conn, &query).map_err(err)
}

fn file_path(state: &AppState, id: i64) -> CmdResult<PathBuf> {
    let (volume_id, rel): (i64, String) = {
        let guard = state.catalog()?;
        guard
            .as_ref()
            .unwrap()
            .conn
            .query_row("SELECT volume_id, rel_path FROM files WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
    };
    let mount = state.mount_of(volume_id).ok_or("the drive holding this file is not plugged in")?;
    Ok(paths::from_rel(&mount, &rel))
}

// ---------- media:// protocol ----------

/// Serves `media://localhost/t<id>` (thumbnail from the cache),
/// `media://localhost/p<id>` (large preview for HEIC, PSD, PDF…; made on demand, then cached) and
/// `media://localhost/o<id>` (the original file, for the preview panel; drive must be plugged in).
fn serve_media(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let path = request.uri().path().trim_start_matches('/');
    let (route, id) = path.split_at(path.len().min(1));
    let Ok(id) = id.parse::<i64>() else {
        return status(StatusCode::BAD_REQUEST);
    };
    let (file, mime) = match route {
        "t" => match state.catalog_dir() {
            Some(dir) => (thumbs::thumb_path(&dir, id), "image/jpeg"),
            None => return status(StatusCode::NOT_FOUND),
        },
        "p" => match make_preview(&state, id) {
            Ok(file) => (file, "image/jpeg"),
            Err(code) => return status(code),
        },
        "o" => match file_path(&state, id) {
            Ok(file) => {
                let mime = match paths::ext_lower(&file.to_string_lossy()).as_str() {
                    "jpg" | "jpeg" | "jpe" | "jfif" => "image/jpeg",
                    "png" => "image/png",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    "bmp" => "image/bmp",
                    "avif" => "image/avif",
                    "svg" => "image/svg+xml",
                    _ => return status(StatusCode::UNSUPPORTED_MEDIA_TYPE),
                };
                (file, mime)
            }
            Err(_) => return status(StatusCode::NOT_FOUND),
        },
        _ => return status(StatusCode::NOT_FOUND),
    };
    match std::fs::read(&file) {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, mime)
            // Thumbnail URLs carry a version query, so they can be cached for good.
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .unwrap(),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

/// Returns the cached large preview, rendering it first if needed (drive must be plugged in).
fn make_preview(state: &AppState, id: i64) -> Result<PathBuf, StatusCode> {
    let dir = state.catalog_dir().ok_or(StatusCode::NOT_FOUND)?;
    let cached = thumbs::preview_path(&dir, id);
    if cached.is_file() {
        return Ok(cached);
    }
    let path = file_path(state, id).map_err(|_| StatusCode::NOT_FOUND)?;
    let ext = paths::ext_lower(&path.to_string_lossy());
    let jpeg = if ext == "fig" {
        thumbs::from_fig(&path, thumbs::PREVIEW_SIZE)
    } else if thumbs::MAGICK_EXTS.contains(&ext.as_str()) {
        thumbs::from_magick(&path, &ext, thumbs::PREVIEW_SIZE)
    } else {
        std::fs::read(&path).map_err(Into::into).and_then(|bytes| {
            let orientation = meta::image_info(&bytes).orientation;
            thumbs::from_image_bytes(&bytes, orientation, thumbs::PREVIEW_SIZE)
        })
    }
    .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    thumbs::write_cached(&cached, &jpeg).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(cached)
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(code).body(Vec::new()).unwrap()
}

#[tauri::command]
fn open_file(app: AppHandle, state: State<AppState>, id: i64) -> CmdResult<()> {
    let path = file_path(&state, id)?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(err)
}

#[tauri::command]
fn reveal_file(app: AppHandle, state: State<AppState>, id: i64) -> CmdResult<()> {
    let path = file_path(&state, id)?;
    app.opener().reveal_item_in_dir(path).map_err(err)
}

/// Port and token of the local video server, for building `<video>` URLs.
#[tauri::command]
fn media_server(state: State<AppState>) -> Option<video::MediaServer> {
    state.media_server.get().cloned()
}

#[tauri::command]
fn open_in_vlc(state: State<AppState>, id: i64) -> CmdResult<()> {
    let path = file_path(&state, id)?;
    let mut child = tools::command(&vlc_binary()).arg(&path).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "VLC is not installed (or not on the PATH)".to_string()
        } else {
            format!("cannot start VLC: {e}")
        }
    })?;
    // Reap the process when VLC closes so it doesn't linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn vlc_binary() -> String {
    #[cfg(windows)]
    for dir in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(dir) {
            let exe = PathBuf::from(base).join("VideoLAN").join("VLC").join("vlc.exe");
            if exe.is_file() {
                return exe.to_string_lossy().to_string();
            }
        }
    }
    "vlc".to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::load())
        .setup(|app| {
            if let Ok(resources) = app.path().resource_dir() {
                tools::set_bundled_dir(resources.join("tools"));
            }
            let handle = app.handle().clone();
            let resolve: video::Resolve = Arc::new(move |id| file_path(&handle.state::<AppState>(), id).ok());
            match video::start(resolve) {
                Ok(server) => {
                    let _ = app.state::<AppState>().media_server.set(server);
                }
                Err(e) => eprintln!("{e:#}"),
            }
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(serve_media(&app, &request)));
        })
        .invoke_handler(tauri::generate_handler![
            get_setup,
            free_space,
            set_catalog_dir,
            list_volumes,
            add_volume,
            rename_volume,
            remove_volume,
            start_scan,
            cancel_scan,
            jobs_status,
            set_jobs_paused,
            query_files,
            file_details,
            list_folders,
            list_cameras,
            open_file,
            reveal_file,
            media_server,
            open_in_vlc,
            start_dupe_search,
            cancel_dupe_search,
            dupe_summary,
            dupe_groups,
            trash_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rummage");
}
