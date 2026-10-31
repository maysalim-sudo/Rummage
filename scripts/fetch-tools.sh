#!/usr/bin/env bash
# Downloads the tools bundled with Linux release builds into tools/linux:
# static ffmpeg + ffprobe (BtbN's build), and ImageMagick (its official AppImage, extracted so it runs
# without FUSE). Used by the release workflow; run it locally before a local release build.
set -euo pipefail

dest="$(cd "$(dirname "$0")/.." && pwd)/tools/linux"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
rm -rf "$dest"
mkdir -p "$dest"

auth=()
[[ -n "${GITHUB_TOKEN:-}" ]] && auth=(-H "Authorization: Bearer $GITHUB_TOKEN")

# johnvansickle.com doesn't serve GitHub's runners, so take BtbN's static GPL build of
# the newest FFmpeg release from GitHub.
echo "Downloading ffmpeg (static build by BtbN, from its GitHub release)…"
ff_url="$(curl -fsSL "${auth[@]}" https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest \
  | grep -o '"browser_download_url": *"[^"]*/ffmpeg-n[0-9.]*-latest-linux64-gpl-[0-9.]*\.tar\.xz"' \
  | sed 's/.*"\(https[^"]*\)"/\1/' | sort -V | tail -1)"
[[ -n "$ff_url" ]] || { echo "No ffmpeg linux64 build found in BtbN's latest release" >&2; exit 1; }
echo "  $ff_url"
curl -fsSL "$ff_url" -o "$tmp/ffmpeg.tar.xz"
tar -xJf "$tmp/ffmpeg.tar.xz" -C "$tmp"
ff_dir="$(find "$tmp" -maxdepth 1 -type d -name 'ffmpeg-*-linux64-gpl-*' | head -1)"
cp "$ff_dir/bin/ffmpeg" "$ff_dir/bin/ffprobe" "$dest/"
mkdir -p "$dest/licenses"
cp "$ff_dir/LICENSE.txt" "$dest/licenses/ffmpeg-LICENSE.txt" 2>/dev/null || true

echo "Downloading ImageMagick (official AppImage from its latest GitHub release)…"
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
