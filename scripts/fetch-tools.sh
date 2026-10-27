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

echo "Downloading ImageMagick (official AppImage)…"
curl -fsSL https://imagemagick.org/archive/binaries/magick -o "$tmp/magick.AppImage"
chmod +x "$tmp/magick.AppImage"
(cd "$tmp" && ./magick.AppImage --appimage-extract >/dev/null)
mv "$tmp/squashfs-root" "$dest/magick"

chmod +x "$dest/ffmpeg" "$dest/ffprobe" "$dest/magick/AppRun"
"$dest/ffmpeg" -version | head -1
"$dest/magick/AppRun" -version | head -1
du -sh "$dest"
