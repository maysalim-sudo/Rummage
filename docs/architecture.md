# Rummage — architecture plan

## Context
User keeps images/videos on ~6 HDDs/SSDs (mixed NTFS/exFAT/ext4, some shared with Windows) and wastes time hunting through them. Goal: one desktop app (Kubuntu first, Windows too) that indexes every drive into a single searchable catalog — browsable even when a drive is unplugged — with filters, categories, move/copy, duplicates, tags, RAW support and local AI "search by description". Scale: **>1M files**, so speed and incremental work are design drivers.

## Stack
- **Tauri 2** shell, **Rust** core, **Svelte + TypeScript** UI (light, fast; virtualized grid via TanStack Virtual).
- **SQLite** (rusqlite, WAL mode, FTS5 for filename/path/tag text search) — one catalog DB in the user data dir.
- **sqlite-vec** for AI embedding vectors (same DB, no extra server).
- Bundled **ffmpeg/ffprobe** (LGPL build) for video metadata + frame grabs.
- Rust crates: `walkdir`/`jwalk` (parallel scan), `blake3` (hashing), `image` + `fast_image_resize` + `webp` (thumbs), `nom-exif` (EXIF), `rawler` (RAW decode/embedded preview), `image_hasher` (perceptual hash), `ort` (ONNX Runtime for CLIP), `notify` (file watching), `sysinfo` + platform APIs (volume IDs).

## Core architecture
```
UI (Svelte) ──Tauri commands/events──► Rust core
                                        ├─ Volume manager   (detect drives, stable IDs, online/offline)
                                        ├─ Scanner          (incremental walk → catalog)
                                        ├─ Job queue        (priority, pause/resume, per-disk throttling)
                                        │   ├─ metadata  ├─ thumbnails  ├─ hashing
                                        │   ├─ phash     └─ AI embeddings
                                        ├─ Query engine     (filters → SQL, FTS, vector search)
                                        ├─ File ops         (move/copy queue, verify, undo log)
                                        └─ Watcher          (live updates for mounted drives)
                                    SQLite catalog + thumbnail cache on disk
```

### Key decisions
1. **Drive identity**: identify volumes by filesystem UUID (Linux `/dev/disk/by-uuid`; Windows volume GUID/serial) **plus** a small marker file `.mediacat-id` at the drive root as fallback. Paths stored *relative to the volume*, so a drive works whether it's `/media/x/Disk2` or `E:\`. Handle NTFS/exFAT case-insensitivity and separators in a path-normalization layer.
2. **Offline-first catalog**: everything (metadata, thumbs, tags, embeddings) lives locally; offline files show greyed with "on Disk2 — plug in".
3. **Incremental scanning**: skip files whose (size, mtime) are unchanged; detect moves by matching size+partial hash. Expect a first full scan of 1M files in minutes (walk) with thumbnails/hashing/AI filling in afterwards in background.
4. **Per-physical-disk throttling**: one I/O worker per HDD (spinning disks hate parallel random reads), more for SSDs; CPU-bound work (thumb resize, CLIP) on a separate pool. User can pause all background work.
5. **Thumbnails**: WebP, 2 sizes (256px grid, 1024px preview), sharded cache dir keyed by file ID. Videos: poster frame + ~8-frame strip for hover-scrub. RAW: use embedded JPEG preview (fast) instead of full decode.
6. **Safe file ops**: copy → verify BLAKE3 → (for move) delete source only after verification; same-volume moves use rename. Every op journaled → undo. Conflict policy (skip/rename/overwrite) asked per batch. Never silent delete; "delete" goes to system trash.
7. **Duplicates**: staged — group by size → hash first 64KB → full BLAKE3 for exact; perceptual hash (dHash/pHash) + Hamming distance for near-duplicates of images (BK-tree in memory). Review UI to choose keepers.
8. **AI search**: CLIP-family model (e.g. OpenCLIP ViT-B/32 or SigLIP, ONNX, ~150–350MB, downloaded on first enable) runs locally via `ort`; GPU (CUDA/DirectML) if available, CPU fallback. Image embeddings computed in background; text query → vector → nearest neighbours in sqlite-vec. Videos: embed a few keyframes. Also gives "find similar to this image".
9. **Organization**: tags (hierarchical), 0–5 stars, color labels, collections (virtual albums), saved searches (smart collections). Stored in DB and mirrored to XMP sidecars (see Decisions); collections stay DB-only.

## UI
- Left sidebar: Drives (online/offline + usage), Folders, Categories (Images/Videos/RAW, by year/month, by camera), Collections, Tags, Saved searches.
- Center: virtualized grid / list toggle, group-by headers, multi-select, drag to collection.
- Top: search bar (filename/tag text + "describe" AI mode), filter chips: type, extension, date range, resolution, orientation, duration, size, camera/lens, rating, tag, drive.
- Right: inspector (preview, metadata, tags, location of all duplicates).
- Bottom: job/progress panel (scan, thumbs, AI, file ops).

## Build order (each milestone usable on its own)
1. Project scaffold, volume manager, scanner, catalog schema, plain list UI.
2. Metadata extraction (EXIF, ffprobe) + thumbnail pipeline + virtualized grid + filters.
3. File ops (copy/move queue, verify, undo).
4. Tags / ratings / collections / saved searches.
5. Duplicate finder (exact → near).
6. RAW support.
7. AI search (model download, embedding job, text + similar search).
8. Watcher, polish, Windows build + installers (GitHub Actions: `.deb`/AppImage + `.msi`).

## Decisions (settled)
- **Project folder**: `/run/media/torinou/Disk2/Data From Desktop/Coding/media-catalog` (session moves there before coding). Disk2 must be mounted to work on it; if it is NTFS/exFAT, put Cargo's build output on the system SSD (`CARGO_TARGET_DIR`) for speed and to avoid permission quirks.
- **Catalog location**: user-chosen in Settings. First-run wizard asks (default `~/.local/share/media-catalog` / `%LOCALAPPDATA%`), shows free space, and can relocate the catalog + thumbnail cache later (copy → verify → switch).
- **XMP sidecars**: tags, ratings, color labels written to sidecars as well as the DB.
  - Naming configurable: `photo.jpg.xmp` (digiKam/darktable) or `photo.xmp` (Lightroom); default digiKam style.
  - On scan, read existing sidecars and import their tags/ratings (two-way, newest-wins by mtime).
  - Writes go through the job queue; offline or read-only drives queue the write until the drive returns, DB stays source of truth meanwhile.
  - Sidecars move/copy together with their media file in file ops and are excluded from the media grid.

## Verification
- Unit tests: path normalization (NTFS/ext4), incremental-scan diffing, copy-verify-move, undo journal, duplicate grouping.
- Fixture folder with mixed JPEG/PNG/HEIC/RAW/MP4/MOV + deliberate duplicates; assert catalog results.
- Real-world run on one HDD: measure scan throughput, verify unplug → offline browsing still works → replug → re-detected at new mount path.
- AI search sanity: query set ("dog", "night city") on fixture images, check top results.
- Windows: CI build + manual test on an NTFS drive shared from Kubuntu.
