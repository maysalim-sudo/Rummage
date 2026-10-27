# Downloads the tools bundled with Windows release builds into tools\windows:
# ffmpeg + ffprobe (gyan.dev "essentials" build) and portable ImageMagick.
# Used by the release workflow; run it locally before a local release build.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$dest = Join-Path (Split-Path $PSScriptRoot -Parent) "tools\windows"
$tmp = Join-Path ([IO.Path]::GetTempPath()) "rummage-tools"
Remove-Item -Recurse -Force $dest, $tmp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dest, $tmp | Out-Null

Write-Host "Downloading ffmpeg (gyan.dev essentials build)..."
Invoke-WebRequest "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile "$tmp\ffmpeg.zip"
Expand-Archive "$tmp\ffmpeg.zip" "$tmp\ffmpeg"
foreach ($exe in "ffmpeg.exe", "ffprobe.exe") {
    Copy-Item (Get-ChildItem "$tmp\ffmpeg" -Recurse -Filter $exe | Select-Object -First 1).FullName $dest
}
New-Item -ItemType Directory -Force "$dest\licenses" | Out-Null
Get-ChildItem "$tmp\ffmpeg" -Recurse -Filter "LICENSE*" | Select-Object -First 1 | Copy-Item -Destination "$dest\licenses\ffmpeg-LICENSE.txt"

Write-Host "Downloading ImageMagick (portable)..."
$base = "https://imagemagick.org/archive/binaries/"
$zip = (Invoke-WebRequest $base -UseBasicParsing).Links.href |
    Where-Object { $_ -match "portable-Q16-HDRI-x64\.zip$" } | Select-Object -Last 1
if (-not $zip) { throw "No portable ImageMagick build found at $base" }
Invoke-WebRequest "$base$zip" -OutFile "$tmp\magick.zip"
Expand-Archive "$tmp\magick.zip" "$tmp\magick"
# The archive may or may not have a top-level folder; keep the folder holding magick.exe.
$exe = Get-ChildItem "$tmp\magick" -Recurse -Filter "magick.exe" | Select-Object -First 1
Move-Item $exe.Directory.FullName "$dest\magick"

& "$dest\ffmpeg.exe" -version | Select-Object -First 1
& "$dest\magick\magick.exe" -version | Select-Object -First 1
