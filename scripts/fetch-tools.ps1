# Downloads the tools bundled with Windows release builds into tools\windows:
# ffmpeg + ffprobe (Gyan's "essentials" build) and portable ImageMagick.
# Used by the release workflow; run it locally before a local release build.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$dest = Join-Path (Split-Path $PSScriptRoot -Parent) "tools\windows"
$tmp = Join-Path ([IO.Path]::GetTempPath()) "rummage-tools"
Remove-Item -Recurse -Force $dest, $tmp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dest, $tmp | Out-Null

$headers = @{}
if ($env:GITHUB_TOKEN) { $headers.Authorization = "Bearer $env:GITHUB_TOKEN" }

# Gyan's GitHub copy of the builds: www.gyan.dev can throttle downloads to a few KB/s.
Write-Host "Downloading ffmpeg (Gyan's essentials build, from its latest GitHub release)..."
$release = Invoke-RestMethod "https://api.github.com/repos/GyanD/codexffmpeg/releases/latest" -Headers $headers
$asset = $release.assets | Where-Object { $_.name -match "essentials_build\.zip$" } | Select-Object -First 1
if (-not $asset) { throw "No essentials ffmpeg build in release $($release.tag_name)" }
Write-Host "  $($asset.browser_download_url)"
Invoke-WebRequest $asset.browser_download_url -OutFile "$tmp\ffmpeg.zip"
Expand-Archive "$tmp\ffmpeg.zip" "$tmp\ffmpeg"
foreach ($exe in "ffmpeg.exe", "ffprobe.exe") {
    Copy-Item (Get-ChildItem "$tmp\ffmpeg" -Recurse -Filter $exe | Select-Object -First 1).FullName $dest
}
New-Item -ItemType Directory -Force "$dest\licenses" | Out-Null
Get-ChildItem "$tmp\ffmpeg" -Recurse -Filter "LICENSE*" | Select-Object -First 1 | Copy-Item -Destination "$dest\licenses\ffmpeg-LICENSE.txt"

Write-Host "Downloading ImageMagick (portable, from its latest GitHub release)..."
$release = Invoke-RestMethod "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest" -Headers $headers
$asset = $release.assets | Where-Object { $_.name -match "portable-Q16-HDRI-x64\.7z$" } | Select-Object -First 1
if (-not $asset) { throw "No portable ImageMagick build in release $($release.tag_name)" }
Write-Host "  $($asset.browser_download_url)"
Invoke-WebRequest $asset.browser_download_url -OutFile "$tmp\magick.7z"
New-Item -ItemType Directory -Force "$tmp\magick" | Out-Null
# 7-Zip when installed (GitHub runners have it), else the tar.exe built into Windows 10+.
if (Get-Command 7z -ErrorAction SilentlyContinue) { 7z x "$tmp\magick.7z" "-o$tmp\magick" -y | Out-Null }
else { tar -xf "$tmp\magick.7z" -C "$tmp\magick" }
if ($LASTEXITCODE) { throw "Cannot extract $tmp\magick.7z" }
# The archive may or may not have a top-level folder; keep the folder holding magick.exe.
$exe = Get-ChildItem "$tmp\magick" -Recurse -Filter "magick.exe" | Select-Object -First 1
Move-Item $exe.Directory.FullName "$dest\magick"

& "$dest\ffmpeg.exe" -version | Select-Object -First 1
& "$dest\magick\magick.exe" -version | Select-Object -First 1
