# Rummage — handoff for the Windows session

Written 2026-10-28 at the end of the Kubuntu session that built Rummage so far. Give this file to
the next session (Claude Code on Windows) together with [STATUS.md](STATUS.md) and
[architecture.md](architecture.md). The goal there is to make the Windows side work and
ship the first release, `v0.1.0`.

## Working with the user

- Always answer in English (US), whatever language the request is in.
- End every response with one status line, `success` or `failed`, lowercase, on its own line
  (from the user's `Coding/CONVENTIONS.md`).
- Follow the code's existing conventions and keep diffs small.
- Ask before anything outward-facing: pushing, tagging, publishing a release, enabling Pages,
  and choosing licenses or third-party bundles.
- The user is a photographer/video maker with 6 drives on 3 physical disks (NTFS, exFAT,
  ext4; some shared with Windows) and about 415k media files.

## What Rummage is

A desktop app that indexes the images, RAW photos and videos on every drive into one
SQLite catalog. The catalog can be browsed even when a drive is unplugged. Stack: Tauri 2,
a Rust core (`src-tauri/src`), and Svelte 5 + TypeScript (`src/`). Repo:
https://github.com/maysalim-sudo/Rummage (public, MIT License). The project folder is still
named `media-catalog`.

### Built and working (tested on Kubuntu with the real drives)

- **Drives**:
  - Drives are identified by filesystem UUID plus a `.mediacat-id` marker file at the drive root.
  - Paths are stored relative to the drive, so a drive is recognized at any mount path or drive letter.
  - Drives can be renamed, and offline drives stay browsable.
- **Scanner**:
  - The scan is incremental: unchanged files are skipped.
  - Deleted files are flagged as missing, not forgotten.
  - Hidden folders and `._` files are skipped.
  - Shared extensions (`.ts`, `.ai`, `.pdf`, `.fig`) are checked by content.
- **Background jobs**:
  - One worker per physical disk extracts EXIF and ffprobe details and makes a 384 px JPEG thumbnail.
  - A 64-bit difference hash is stored per photo for the similar-photo search.
  - The work can be paused.
- **Previews**:
  - ImageMagick handles HEIC, AVIF, PSD, SVG, TGA, EXR and AI/EPS/PDF; Figma files use the thumbnail inside them.
  - Large previews are 1600 px, made on demand.
- **Video**:
  - A token-protected local HTTP server on 127.0.0.1 streams files with byte ranges, so the side panel can play and seek.
  - There's also an Open in VLC button.
- **UI**: grid and list views (virtualized), folder tree with counts, breadcrumbs, full-text
  search, multi-select filters (type, resolution, orientation, camera, length), inspector,
  progress panel per drive (including queued drives), and an update banner.
- **Duplicates**:
  - Exact copies are found in stages: same size → quick BLAKE3 of the first/last 64 KB → full BLAKE3.
  - Similar photos are found by thumbnail dHash with a banded search.
  - A review screen moves extra copies to the system trash, with last-copy protection.
- **Updater**: Tauri updater plugin. It checks
  `https://github.com/maysalim-sudo/Rummage/releases/latest/download/latest.json` and
  verifies the signature with the public key in `src-tauri/tauri.conf.json`.
- **Landing page** (`site/index.html`): an interactive demo with made-up data, deployed by
  `.github/workflows/pages.yml`. The user wants it published **together with the first
  release**. GitHub Pages isn't enabled yet.

### Release pipeline (done in this session)

- `scripts/fetch-tools.sh` (Linux) and `scripts/fetch-tools.ps1` (Windows) download the tools that are
  bundled in release builds into `tools/<os>/` (git-ignored):
  - Linux: static ffmpeg + ffprobe, plus ImageMagick's AppImage from its latest GitHub release, packed as
    `tools/linux/magick.tar.gz`.
  - Windows: ffmpeg + ffprobe (Gyan's essentials build, from its GitHub release) and ImageMagick's
    `portable-Q16-HDRI-x64.7z` from its latest GitHub release, extracted to `tools/windows/magick/`.
- `src-tauri/tauri.release.linux.json` (AppImage with GStreamer bundled, plus .deb) and
  `src-tauri/tauri.release.windows.json` (NSIS installer, per-user install). Each bundles `tools/<os>/` as
  the `tools/` resource folder.
- `src-tauri/src/tools.rs` finds the tools: it uses the bundled copy if there is one, else the PATH.
  - Windows: `tools/ffmpeg.exe`, `tools/ffprobe.exe`, `tools/magick/magick.exe`.
  - Linux: it unpacks `magick.tar.gz` once into the cache folder. On Linux an installed `magick` wins over the bundled one,
    because the official Linux build's libheif can't read recent iPhone HEICs.
- `.github/workflows/ci.yml`: svelte-check + frontend build, and clippy + `cargo test` on
  Ubuntu **and Windows**. **Green on both** (run 40622162947).
- `.github/workflows/release.yml` runs on a `v*` tag:
  1. Checks the tag matches the version in `tauri.conf.json`.
  2. Creates a **draft** release.
  3. Builds Linux (on Ubuntu 22.04) and Windows with tauri-action.
  4. Uploads the signed installers and `latest.json`.
  Nothing reaches users until the draft is published. **It has never run.**
- The Linux release build was made locally and tested by the user: the AppImage runs, and
  videos, thumbnails and previews work.

Problems already solved (don't undo these):
- Left unpacked, the bundled ImageMagick made the AppImage crash (`g_dir_unref`), because
  the AppImage packager copied ImageMagick's old libraries over the app's. That's why
  ImageMagick is shipped packed on Linux.
- The PDF thumbnail test skips its PDF part when Ghostscript isn't installed (the Windows
  CI runner has none).

## Git state when this was written

- Branch `main`. Its last pushed commit is `13fa178`; the local commits after it are the STATUS.md update and this
  file. Push them (with the user's OK) so a Windows clone gets them.
- The old feature branches (`m2-metadata-thumbnails`, `folder-tree`,
  `design-previews-video`, `rename-and-duplicates`) are all merged into `main`.
- No tags and no releases yet.

## Setting up on Windows

**Clone fresh from GitHub** into a folder on the Windows drive (for example `C:\dev\Rummage`).
Don't build in the shared `Disk2\...\Coding\media-catalog` folder:
- its `node_modules` holds Linux-only native binaries;
- its git-ignored `src-tauri/.cargo/config.toml` points Cargo at a Linux path
  (`/home/torinou/.cache/media-catalog-target`);
- git there shows CRLF/LF noise.

Requirements: Rust stable (MSVC toolchain + Visual Studio Build Tools with the C++
workload), Node 24, pnpm 12, WebView2 (already part of Windows 11).

```powershell
pnpm install
pnpm tauri dev                     # debug run, tools from the PATH
cd src-tauri; cargo test; cd ..    # Rust tests
pwsh scripts/fetch-tools.ps1       # tools for a release build -> tools\windows
pnpm tauri build --config src-tauri/tauri.release.windows.json
```

For a dev run without bundled tools, put `ffmpeg`, `ffprobe` and ImageMagick 7 `magick` on the PATH
(plus Ghostscript for PDF/AI/EPS). Ghostscript comes from https://ghostscript.com.

**Signing key:**
- The updater's private key is `~/.tauri/rummage.key` on the Kubuntu machine, with an empty password.
- It's needed for the GitHub secret and for signed local builds (set `TAURI_SIGNING_PRIVATE_KEY` to the file's contents).
- Copy it to Windows privately (USB drive or password manager). **Never commit it, paste it into chat, or put it in a file in the repo.**
- If it's lost, existing installs can't receive updates, so the user should keep a backup.

**Catalog:** Windows settings live in `%APPDATA%\rummage\config.json`. The Kubuntu catalog
is at `catalog.sqlite` in the root of the drive mounted as `/run/media/torinou/F0F8C99EF8C96406`.
Sharing one catalog between Linux and Windows is the design goal (drive-relative paths and
marker files), but it **has never been tried**. Start Windows with its own new catalog
folder. Only try opening the Linux catalog after backing up `catalog.sqlite` and the
`thumbs/` and `previews/` folders next to it.

## To do on Windows, in order

1. **Run the Windows app for real.** CI only compiled it and ran the unit tests; the app has never
   been opened on Windows. Check:
   - drive detection, drive letters, and the Windows drive identity (`volumes.rs`, `#[cfg(windows)]` at line ~192);
   - a scan of a drive, thumbnails, and the HEIC/PSD/SVG previews;
   - video playback in the side panel (WebView2) and Open in VLC (`vlc_binary()` in `lib.rs` looks in Program Files);
   - the duplicate finder, and moving files to the Recycle Bin (`trash` crate);
   - with the user's OK, whether the drives that Kubuntu already catalogued keep their identity (the marker file).
2. **Test `scripts/fetch-tools.ps1`.** Its ImageMagick part is new and has never run. It uses
   `7z` if installed, else Windows' built-in `tar`. Then make a local release build and install it.
   Check that the installed app uses the bundled `tools\` copies, for example with
   Process Explorer, or by temporarily removing ffmpeg and magick from the PATH.
3. **Ghostscript on Windows.** PDF, Illustrator (.ai) and EPS previews need Ghostscript, which
   the installer doesn't include. Choose with the user:
   - **(a)** show a clear message in the inspector ("Install Ghostscript to preview PDF/AI/EPS files", with a link);
   - **(b)** bundle it. Ghostscript is AGPL, so bundling it next to an MIT app is allowed as a separate
     program, but its license and a source offer must ship with it. Ask the user before bundling.
   - Either way, ImageMagick must find it: it looks for `gswin64c.exe`, and the `MAGICK_GHOSTSCRIPT_PATH`
     environment variable can point it at a bundled copy.
4. **First release** (each step needs the user's go-ahead):
   1. Push the local commits on `main`. CI must be green.
   2. In GitHub → Settings → Secrets and variables → Actions, add `TAURI_SIGNING_PRIVATE_KEY` (contents of the
      key file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty).
   3. Tag and push: `git tag v0.1.0` then `git push origin v0.1.0`. The tag must match `version` in
      `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` (all `0.1.0`).
   4. Watch the Release workflow. The Windows job and `fetch-tools.ps1` run on GitHub for the first time
      here, so expect a fix or two.
   5. Download the draft's files and try them: the AppImage and .deb on Kubuntu, the setup .exe on Windows.
      Then publish the draft.
   6. Turn on GitHub Pages (Settings → Pages → Source: GitHub Actions) and re-run `pages.yml`
      so the landing page goes live with the release. The landing page's download buttons
      point at `releases/latest`.
   7. To test the updater: bump the version to 0.1.1, tag, publish, and check the update banner appears in 0.1.0.
5. **Small CI cleanup (optional):** `pnpm/action-setup@v4` still warns about Node 20. The code isn't
   `cargo fmt`-clean, so CI doesn't check formatting. Running `cargo fmt` once is a large
   but harmless diff; ask the user first.

## Later (roadmap, not for the first release)

- **Moving and copying files through Squeue**, the user's own crash-safe copy manager:
  `Coding/TeraCopy`, https://github.com/maysalim-sudo/Squeue, .NET 10, which already runs on Windows.
  - Rummage will call `squeue --to <folder> <files…>` with a new flag that writes a JSON report.
  - A move updates the catalog path and keeps the details and thumbnail; a copy adds a catalog entry.
  - Squeue still needs moves and the report flag, and a Linux port for Kubuntu.
  - Don't build a separate copy engine in Rummage.
- Tags, ratings, collections and XMP sidecars (`photo.jpg.xmp`, two-way, written through the
  job queue).
- RAW previews from the embedded JPEG, and AI search by description (CLIP via ONNX, local).
- Smaller items: remember Pause across restarts, move the catalog from Settings, video hover-scrub.

## Where things are

| Path | What |
|---|---|
| `src-tauri/src/lib.rs` | App state, all Tauri commands, `media://` scheme (thumbnails/previews/originals), startup |
| `volumes.rs` / `scanner.rs` / `jobs.rs` | Drives, scanning, background work per disk |
| `thumbs.rs` / `meta.rs` / `media_kind.rs` / `tools.rs` | Thumbnails and previews, EXIF/ffprobe, file types, external tools |
| `query.rs` / `db.rs` / `dupes.rs` / `video.rs` | Filters → SQL, schema + migrations (v5), duplicates, video server |
| `src/App.svelte`, `src/lib/*.svelte` | UI: grid/list, drive list + folder tree, inspector, duplicates, update banner |
| `docs/STATUS.md` | Living status; keep it updated |
| `site/` | Landing page |
| `.github/workflows/` | `ci.yml`, `release.yml`, `pages.yml` |
