# Downloads the tools bundled with Windows release builds into tools\windows:
# ffmpeg + ffprobe (Gyan's "essentials" build), portable ImageMagick and Ghostscript.
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

# ImageMagick renders PDF, AI and EPS through Ghostscript, which it finds next to magick.exe.
Write-Host "Downloading Ghostscript (from its latest GitHub release)..."
$release = Invoke-RestMethod "https://api.github.com/repos/ArtifexSoftware/ghostpdl-downloads/releases/latest" -Headers $headers
$asset = $release.assets | Where-Object { $_.name -match "^gs\d+w64\.exe$" } | Select-Object -First 1
$source = $release.assets | Where-Object { $_.name -match "^ghostscript-.*\.tar\.gz$" } | Select-Object -First 1
if (-not $asset -or -not $source) { throw "No Windows build or source in release $($release.tag_name)" }
Write-Host "  $($asset.browser_download_url)"
Invoke-WebRequest $asset.browser_download_url -OutFile "$tmp\gs.exe"
# Its installer needs admin rights to run, so unpack it instead. Only full 7-Zip can open it;
# without one installed, take a portable copy, which the tar.exe built into Windows can unpack.
$sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
if (-not $sevenZip) {
    $release7z = Invoke-RestMethod "https://api.github.com/repos/ip7z/7zip/releases/latest" -Headers $headers
    $asset7z = $release7z.assets | Where-Object { $_.name -match "^7z\d+-x64\.exe$" } | Select-Object -First 1
    if (-not $asset7z) { throw "No 7-Zip build in release $($release7z.tag_name)" }
    Invoke-WebRequest $asset7z.browser_download_url -OutFile "$tmp\7zip.exe"
    New-Item -ItemType Directory -Force "$tmp\7zip" | Out-Null
    tar -xf "$tmp\7zip.exe" -C "$tmp\7zip"
    if ($LASTEXITCODE) { throw "Cannot extract $tmp\7zip.exe" }
    $sevenZip = "$tmp\7zip\7z.exe"
}
& $sevenZip x "$tmp\gs.exe" "-o$tmp\gs" -y | Out-Null
if ($LASTEXITCODE) { throw "Cannot extract $tmp\gs.exe" }
Copy-Item "$tmp\gs\bin\gswin64c.exe", "$tmp\gs\bin\gsdll64.dll" "$dest\magick"
# Ghostscript needs the Visual C++ runtime, which not every PC has.
foreach ($dll in "msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
    Copy-Item "$env:SystemRoot\System32\$dll" "$dest\magick"
}
Copy-Item "$tmp\gs\doc\COPYING" "$dest\licenses\ghostscript-LICENSE.txt"
@"
Ghostscript is a separate program under the GNU Affero General Public License 3.0
(ghostscript-LICENSE.txt). This copy is the unmodified Windows build of $($release.name).
Its complete source code: $($source.browser_download_url)
"@ | Set-Content -Encoding utf8 "$dest\licenses\ghostscript-source.txt"

& "$dest\ffmpeg.exe" -version | Select-Object -First 1
& "$dest\magick\magick.exe" -version | Select-Object -First 1
& "$dest\magick\gswin64c.exe" -version | Select-Object -First 1
