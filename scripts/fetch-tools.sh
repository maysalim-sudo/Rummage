#!/usr/bin/env bash
# Downloads the tools bundled with Linux release builds into tools/linux:
# static ffmpeg + ffprobe, and ImageMagick (its official AppImage, extracted so it runs
# without FUSE). Used by the release workflow; run it locally before a local release build.
set -euo pipefail

dest="$(cd "$(dirname "$0")/.." && pwd)/tools/linux"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
rm -rf "$dest"
mkdir -p "$dest"

echo "Downloading ffmpeg (static build by John Van Sickle)…"
curl -fsSL https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz -o "$tmp/ffmpeg.tar.xz"
tar -xJf "$tmp/ffmpeg.tar.xz" -C "$tmp"
ff_dir="$(find "$tmp" -maxdepth 1 -type d -name 'ffmpeg-*-static' | head -1)"
cp "$ff_dir/ffmpeg" "$ff_dir/ffprobe" "$dest/"
mkdir -p "$dest/licenses"
cp "$ff_dir/GPLv3.txt" "$dest/licenses/ffmpeg-GPLv3.txt" 2>/dev/null || true
cp "$ff_dir/readme.txt" "$dest/licenses/ffmpeg-readme.txt" 2>/dev/null || true

echo "Downloading ImageMagick (official AppImage from its latest GitHub release)…"
auth=()
[[ -n "${GITHUB_TOKEN:-}" ]] && auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
im_url="$(curl -fsSL "${auth[@]}" https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest \
  | grep -o '"browser_download_url": *"[^"]*-gcc-x86_64\.AppImage"' | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')"
[[ -n "$im_url" ]] || { echo "No ImageMagick AppImage found in the latest release" >&2; exit 1; }
echo "  $im_url"
curl -fsSL "$im_url" -o "$tmp/magick.AppImage"
chmod +x "$tmp/magick.AppImage"
(cd "$tmp" && ./magick.AppImage --appimage-extract >/dev/null)
"$tmp/squashfs-root/AppRun" -version | head -1
# Packed so the AppImage packager doesn't copy ImageMagick's own libraries over the app's;
# the app unpacks it into its cache folder on first use (see src-tauri/src/tools.rs).
tar -czf "$dest/magick.tar.gz" -C "$tmp/squashfs-root" .

chmod +x "$dest/ffmpeg" "$dest/ffprobe"
"$dest/ffmpeg" -version | head -1
du -sh "$dest"
