# Rummage — where things stand

Last updated 2026-10-27. Architecture and roadmap: [architecture.md](architecture.md).

## Done

| Plan milestone | What works |
|---|---|
| 1. Catalog | Drive detection with stable ids (`.mediacat-id` marker + filesystem UUID), incremental scanner, SQLite catalog with full-text name search, offline browsing |
| 2. Details & thumbnails | EXIF and ffprobe details, 384 px thumbnails, one background worker per physical disk, grid/list views, inspector, filters |
| — Folder tree | Expandable tree per drive with counts, breadcrumb, include-subfolders toggle |
| — Previews & video | ImageMagick for HEIC/PSD/SVG/AVIF/TGA/EXR and AI/EPS/PDF; Figma thumbnails; 1600 px on-demand previews; in-panel video player via a token-protected local HTTP server; Open in VLC |
| — Filters | Multi-select type / resolution / orientation / camera / length; camera counts follow the other filters |
| 5. Duplicates | Exact copies (size → quick hash → full BLAKE3, stored in the catalog) and similar photos (thumbnail difference hash, banded search); review screen; move to the system trash with last-copy protection |

Tested on the author's machine: 6 drives (3 physical disks), ~415k files, Kubuntu 26.04, catalog on an HDD.

## Next

1. **Try the duplicate finder on the real catalog** (first run reads ~290k same-size candidates and fingerprints existing thumbnails).
2. **First release** (milestone 8, early): GitHub Actions builds the Linux AppImage + `.deb` and the Windows installer on a version tag; Tauri updater with signed updates from GitHub Releases ([maysalim-sudo/Rummage](https://github.com/maysalim-sudo/Rummage)); ffmpeg and ImageMagick bundled.
3. **Moving and copying files** (milestone 3) — **through Squeue**, not built here. See below.
4. Tags, ratings, collections, XMP sidecars (milestone 4).
5. RAW previews (milestone 6), AI search by description (milestone 7).

Smaller follow-ups: remember Pause across restarts; let the catalog be moved to another drive from Settings; video hover-scrub previews.

## Moving and copying: plan with Squeue

[Squeue](https://github.com/maysalim-sudo/Squeue) (`../TeraCopy`) is the author's crash-safe, checksummed copy manager (.NET 10). Rummage will use it as its copy engine instead of re-implementing safe copies:

1. Rummage runs `squeue --to <folder> <files…>` with a flag that writes a JSON report (source, destination, outcome per file).
2. When Squeue finishes, Rummage reads the report: a **move** updates the file's path and keeps its tags, details and thumbnail; a **copy** adds the new file to the catalog.

Needed in Squeue first: the Linux port, phases 1–2 (Core + `squeue` CLI; plan in Squeue's `docs/superpowers/plans/2026-10-20-squeue-linux-port.md`, written for CachyOS — use a `.deb` instead of the AUR package on Kubuntu), **moves**, and the **JSON report** flag. On Windows, Squeue already runs, so only the report flag and the Rummage side are needed there.

## Development notes

- Run: `pnpm tauri dev` (debug build — noticeably slower at thumbnails than a release build).
- Tests: `cd src-tauri && cargo test`; manual checks on real files are `#[ignore]` tests (`detect_on_this_machine`, `render_real_files`, `stream_real_video`).
- Runtime tools for now: `ffmpeg`/`ffprobe`, ImageMagick 7 (`magick`) with Ghostscript.
- The project folder is still named `media-catalog`; settings fall back to the old `~/.config/media-catalog` folder.

## Release setup — in progress (branch `rename-and-duplicates`)

Done: updater plugin + "update available" banner, signing key pair (`~/.tauri/rummage.key`, never committed — back it up; without it no further updates can be signed), `tauri.release.linux.json` / `tauri.release.windows.json` (bundle `tools/` as resources), bundled-tool lookup in `tools.rs`, `scripts/fetch-tools.sh` / `.ps1`. Local Linux build works: `pnpm tauri build --config src-tauri/tauri.release.linux.json` (with `TAURI_SIGNING_PRIVATE_KEY` set from the key file) → AppImage (~270 MB, GStreamer bundled; tested by the user 2026-10-28: video player, thumbnails, previews work) and `.deb` in `~/.cache/media-catalog-target/release/bundle/`.

Linux ImageMagick notes: `fetch-tools.sh` takes the AppImage from ImageMagick's latest GitHub release and packs it as `tools/magick.tar.gz`; the app unpacks it to `~/.cache/rummage/tools/magick-<version>` on first use. Left unpacked, the AppImage packager copied ImageMagick's own older libraries (glib…) over the app's and it crashed at startup (`undefined symbol: g_dir_unref`). An installed `magick` on the PATH is preferred over the bundled one: the official Linux build has libheif 1.12, which fails on recent iPhone HEICs ("Too many auxiliary image references").

Workflows (checked with actionlint, not yet run on GitHub): `ci.yml` (svelte-check + frontend build; clippy + tests on Ubuntu and Windows) on every push; `release.yml` on a `v*` tag: checks the tag matches the version, creates a draft release, builds Linux (Ubuntu 22.04: AppImage + .deb) and Windows (NSIS) with tauri-action and uploads the signed files + `latest.json`. Publishing the draft makes the update live. `fetch-tools.ps1` now takes ImageMagick's portable .7z from its GitHub release (untested until the first Windows run).

Left:
1. With the user's OK: push to GitHub (license chosen: MIT, in `LICENSE`), add `TAURI_SIGNING_PRIVATE_KEY` (+ empty `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) as repo secrets, check CI is green (first Windows build), tag `v0.1.0`, review and publish the draft release.
2. Publish the landing page (`site/`) together with that release: enable GitHub Pages (source: GitHub Actions) so `pages.yml` deploys it. Kept local until then.
