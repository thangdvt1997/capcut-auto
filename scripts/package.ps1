#Requires -Version 5.1
<#
.SYNOPSIS
    Produces a distributable Windows installer (NSIS + MSI) via
    `tauri build`.

.DESCRIPTION
    For a Windows dev machine with the full toolchain installed, including
    NSIS/WiX tooling that `cargo tauri build` invokes automatically on
    first run. This project's remote build server (Linux) can compile the
    Rust/frontend code but cannot produce a Windows installer — packaging
    itself is UNVERIFIED as of Phase 2 (see IMPLEMENTATION_PLAN.md). Real
    installer output/signing/first-run flow needs a real Windows machine.

.NOTES
    Output lands in src-tauri/target/release/bundle/{nsis,msi}/.
#>

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $RepoRoot
try {
    if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
        throw "'pnpm' was not found on PATH."
    }

    Write-Host 'Installing frontend dependencies...' -ForegroundColor Cyan
    pnpm install
    if ($LASTEXITCODE -ne 0) { throw 'pnpm install failed.' }

    Write-Host 'Running tauri build (NSIS + MSI)...' -ForegroundColor Cyan
    pnpm tauri build
    if ($LASTEXITCODE -ne 0) { throw 'tauri build failed.' }

    Write-Host 'Installer(s) written to src-tauri/target/release/bundle/' -ForegroundColor Green
}
finally {
    Pop-Location
}
