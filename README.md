# Rummage

A desktop app (Kubuntu first, Windows too) that indexes the images, RAW photos and
videos on all your drives into one searchable catalog. You can browse it even when a
drive is unplugged.

Built with Tauri 2 (Rust core) and Svelte 5. The full architecture and roadmap are in
[docs/architecture.md](docs/architecture.md).

## Status: milestone 2

- Detects mounted drives and identifies them by filesystem UUID plus a `.mediacat-id`
  marker file at the drive root, so a drive is recognized at any mount path or drive letter.
  Drives can be renamed.
- Incremental scanner: unchanged files are skipped, deleted files are flagged (not
  forgotten), and HDDs are read single-threaded. Hidden folders, macOS `._` files and
  TypeScript `.ts` files (vs. real MPEG-TS video) are left out.
- Background processing after each scan: EXIF details (camera, lens, exposure, date
  taken, GPS) and ffprobe video details (duration, codec, fps), plus a 384 px JPEG
  thumbnail per file. One worker per physical disk; pausable.
- SQLite catalog with full-text search on file and folder names.
- UI: grid view with adjustable thumbnail size and a list view (both virtualized,
  keyboard navigable), filters (type, resolution, orientation, camera, video length,
  date range), and an inspector with a full-size preview and all details.

- HEIC, AVIF, PSD, SVG, TGA, EXR and design files (AI, EPS, PDF, Figma `.fig`) get
  thumbnails and large previews through ImageMagick (Figma: the thumbnail inside the file).
  Shared extensions (`.ts`, `.ai`, `.pdf`…) are checked by content before cataloguing.
- Videos play in the preview panel (streamed with seeking by a token-protected local HTTP
  server on 127.0.0.1), with an "Open in VLC" button for formats the webview can't play.

Not yet: RAW thumbnails (milestone 6) and video hover-scrub previews (poster frames only).

Runtime tools: `ffmpeg`/`ffprobe` and ImageMagick 7 (`magick`, with Ghostscript for AI/EPS/PDF)
must be on the PATH for now; installers will bundle them (milestone 8).

## Development

Requirements: Rust (stable), Node 24 + pnpm, and the Tauri Linux dependencies
(`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`).

```bash
pnpm install
pnpm tauri dev
```

Rust tests:

```bash
cd src-tauri && cargo test
```

The project lives on an NTFS drive, so the build output is redirected to the system SSD
by `src-tauri/.cargo/config.toml` (git-ignored, machine-specific):

```toml
[build]
target-dir = "/home/<you>/.cache/media-catalog-target"
```

## Where data lives

- Settings: `~/.config/rummage/config.json` (`%APPDATA%\rummage` on Windows)
- Catalog: the folder chosen at first run (default `~/.local/share/rummage`)
- On each added drive: a small `.mediacat-id` file at the root
