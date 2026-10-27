# Media Catalog

A desktop app (Kubuntu first, Windows too) that indexes the images, RAW photos and
videos on all your drives into one searchable catalog. You can browse it even when a
drive is unplugged.

Built with Tauri 2 (Rust core) and Svelte 5. The full architecture and roadmap are in
[docs/architecture.md](docs/architecture.md).

## Status: milestone 1

- Detects mounted drives and identifies them by filesystem UUID plus a `.mediacat-id`
  marker file at the drive root, so a drive is recognized at any mount path or drive letter.
- Incremental scanner: unchanged files are skipped, deleted files are flagged (not
  forgotten), and HDDs are read single-threaded.
- SQLite catalog with full-text search on file and folder names.
- UI: first-run catalog location picker, drive sidebar with live scan progress, and a
  virtualized file list (search, type filter, sort, open / show in folder).

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

- Settings: `~/.config/media-catalog/config.json` (`%APPDATA%\media-catalog` on Windows)
- Catalog: the folder chosen at first run (default `~/.local/share/media-catalog`)
- On each added drive: a small `.mediacat-id` file at the root
