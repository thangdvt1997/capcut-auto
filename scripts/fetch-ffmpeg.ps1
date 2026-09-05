#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads the pinned, checksum-verified FFmpeg/FFprobe Windows binaries
    this project ships, and places them at the exact path
    `ffmpeg::binaries`'s sidecar-resolution logic (and `tauri.windows.conf.json`'s
    `bundle.externalBin`) already expect.

.DESCRIPTION
    Phase 12 (Windows packaging) resolves Phase 3's deferred "which FFmpeg
    build/license" decision (see `docs/architecture-audit.md` §6 risk #7 and
    `src-tauri/src/ffmpeg/binaries.rs`'s own module doc comment). This
    project does NOT download a random/"latest" binary at build time and
    does NOT commit the binaries to git (multi-hundred-MB, and
    `src-tauri/binaries/` is already `.gitignore`d — see that file's own
    "FFmpeg/FFprobe sidecars" comment). Instead: a human (or a CI Windows
    runner) runs this script ONCE before `tauri build`/`tauri dev` needs a
    real bundled ffmpeg, and it fetches one specific, named, checksum-pinned
    release.

    **Chosen source**: BtbN/FFmpeg-Builds (https://github.com/BtbN/FFmpeg-Builds),
    a well-known, actively-maintained GitHub Actions build pipeline that
    compiles real, tagged upstream FFmpeg release tarballs for Windows/Linux,
    publishing both GPL and LGPL variants per architecture. This project
    pins the **GPL, statically-linked, Windows x64, FFmpeg 9.0.1** build —
    see THIRD_PARTY_NOTICES.md's "FFmpeg / FFprobe" section for the full
    licensing writeup (short version: this app's own `render` engine
    (Phase 6) already hard-depends on the GPL-licensed `libx264`/`libx265`
    software encoders for its default/fallback H.264/H.265 export path, so
    an LGPL build — which BtbN's own LGPL variant deliberately excludes
    those encoders from — would silently break already-shipped, tested
    render functionality; GPL obligations are explicitly accepted instead,
    per `docs/architecture-audit.md`'s own "prefer LGPL unless GPL
    obligations are explicitly accepted" framing). The **static** (not
    "-shared") variant is chosen deliberately: a static build produces a
    single self-contained `ffmpeg.exe`/`ffprobe.exe` with no accompanying
    DLLs to also track/bundle, matching this project's existing
    single-file-sidecar resolution model in `ffmpeg::binaries` exactly —
    the "-shared" variant would need several additional `.dll` files copied
    alongside the exe, which nothing in this codebase's resolution logic
    currently accounts for.

    **Pinned release**: tag `autobuild-2026-09-05-13-10`, asset
    `ffmpeg-n9.0.1-26-g5c8e7e2433-win64-gpl-9.0.zip` (a real tagged FFmpeg
    9.0.1 release build, not a "master"-branch rolling snapshot — chosen
    over BtbN's also-available `master`-branch build for exactly that
    reproducibility reason). A SHA256 checksum is pinned below and verified
    before extraction; the checksum was read from that same release's own
    `checksums.sha256` asset at the time this script was written — **if
    this script ever refuses with a checksum mismatch, do not bypass the
    check**: either the pin is stale (upstream re-cut the same tag, which
    BtbN's automation is not expected to do for a dated `autobuild-*` tag,
    but verify at https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-09-05-13-10
    before assuming otherwise) or the download was corrupted/tampered
    with — re-verify manually before updating the pin.

    **Upgrading this pin later**: pick a newer BtbN release tag + the
    `win64-gpl-<version>.zip` (static) asset for the FFmpeg version you
    want, download its `checksums.sha256` from the same release, copy the
    matching line's hash into `$Sha256` below, and update `$ReleaseTag`/
    `$AssetName`/`$FfmpegSourceVersion`. Never point `$ReleaseTag` at the
    literal `latest` alias — it is a rolling, mutable pointer BtbN
    re-tags on every automated build, which defeats the entire point of a
    pinned, reproducible, checksum-verified dependency.

.NOTES
    Run from the repository root or from `scripts/` — this script resolves
    paths relative to its own location either way. Requires internet
    access; this is the ONE step in this project's build process that
    reaches the network for a shipping artifact (matches master prompt §59
    "Do not download random binaries at runtime" — this downloads one
    specific, named, checksummed release, once, at build-prep time, never
    at app runtime).
#>

param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

# --- Pinned release (see .DESCRIPTION above for how to change this) --------
$ReleaseTag = 'autobuild-2026-09-05-13-10'
$AssetName = 'ffmpeg-n9.0.1-26-g5c8e7e2433-win64-gpl-9.0.zip'
$Sha256 = 'a8ebbaf7a99185f5abc3a2d3a657521c38d7966f06b70468d7ab29a67fe8654f'
$FfmpegSourceVersion = 'FFmpeg 9.0.1 (BtbN/FFmpeg-Builds win64-gpl static build)'
$DownloadUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$ReleaseTag/$AssetName"

# Must match `ffmpeg::binaries::TARGET_TRIPLE`'s `x86_64-pc-windows-msvc`
# branch — this project's real Windows dev toolchain (see `HANDOFF.md`
# "Build/test environment") targets MSVC, not GNU, so the sidecar filename
# suffix must match that, independent of whichever toolchain BtbN itself
# used to compile the ffmpeg.exe binary (an ordinary Win32 PE executable
# either way — the suffix is purely this project's own sidecar-naming
# convention, not a real ABI requirement on the ffmpeg binary itself).
$TargetTriple = 'x86_64-pc-windows-msvc'

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$BinariesDir = Join-Path $RepoRoot 'src-tauri\binaries'
$FfmpegDest = Join-Path $BinariesDir "ffmpeg-$TargetTriple.exe"
$FfprobeDest = Join-Path $BinariesDir "ffprobe-$TargetTriple.exe"

if ((Test-Path $FfmpegDest) -and (Test-Path $FfprobeDest) -and -not $Force) {
    Write-Host "Already present: $FfmpegDest" -ForegroundColor Yellow
    Write-Host "Already present: $FfprobeDest" -ForegroundColor Yellow
    Write-Host 'Pass -Force to re-download and overwrite.' -ForegroundColor Yellow
    exit 0
}

New-Item -ItemType Directory -Force -Path $BinariesDir | Out-Null

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ave-ffmpeg-fetch-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
$ZipPath = Join-Path $TempDir $AssetName

try {
    Write-Host "Downloading $AssetName from BtbN/FFmpeg-Builds ($ReleaseTag)..." -ForegroundColor Cyan
    Write-Host "  $DownloadUrl"
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing

    Write-Host 'Verifying SHA256 checksum...' -ForegroundColor Cyan
    $actualHash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash
    if ($actualHash.ToLowerInvariant() -ne $Sha256.ToLowerInvariant()) {
        throw "Checksum mismatch for $AssetName!`n  expected: $Sha256`n  actual:   $actualHash`nDo NOT use this file - see this script's .DESCRIPTION for what to do next."
    }
    Write-Host "  OK: $actualHash" -ForegroundColor Green

    Write-Host 'Extracting...' -ForegroundColor Cyan
    $ExtractDir = Join-Path $TempDir 'extracted'
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force

    # BtbN zips contain one top-level folder (named after the build) with a
    # `bin\` subfolder holding ffmpeg.exe/ffprobe.exe/ffplay.exe (+ a
    # LICENSE.txt this project already accounts for separately in
    # THIRD_PARTY_NOTICES.md) - search rather than hardcode the top-level
    # folder name, since it's derived from the exact git-describe build
    # string and would otherwise need updating every time the pin changes.
    $foundFfmpeg = Get-ChildItem -Path $ExtractDir -Recurse -Filter 'ffmpeg.exe' | Select-Object -First 1
    $foundFfprobe = Get-ChildItem -Path $ExtractDir -Recurse -Filter 'ffprobe.exe' | Select-Object -First 1
    if (-not $foundFfmpeg) { throw "ffmpeg.exe not found inside extracted archive $ZipPath" }
    if (-not $foundFfprobe) { throw "ffprobe.exe not found inside extracted archive $ZipPath" }

    Copy-Item -Path $foundFfmpeg.FullName -Destination $FfmpegDest -Force
    Copy-Item -Path $foundFfprobe.FullName -Destination $FfprobeDest -Force

    Write-Host "Placed: $FfmpegDest" -ForegroundColor Green
    Write-Host "Placed: $FfprobeDest" -ForegroundColor Green
    Write-Host "Source: $FfmpegSourceVersion" -ForegroundColor Green
    Write-Host ''
    Write-Host 'These files are picked up automatically by:' -ForegroundColor Cyan
    Write-Host '  - `cargo run`/`cargo test` in src-tauri/ (dev fallback path, see ffmpeg::binaries)'
    Write-Host '  - `tauri build` (bundled as a sidecar via tauri.windows.conf.json''s bundle.externalBin)'
}
finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
