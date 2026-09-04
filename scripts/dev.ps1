#Requires -Version 5.1
<#
.SYNOPSIS
    Runs the AI Video Editor in dev mode (hot-reloading Tauri window).

.DESCRIPTION
    For a Windows dev machine with the full toolchain installed: Rust
    (stable, MSVC target), Node.js, pnpm, and the Tauri CLI. This repo's own
    dev box (this session's local machine, d:\work-out) has none of those —
    verification here happened on a remote Ubuntu build server instead (see
    IMPLEMENTATION_PLAN.md Phase 2). This script is written for, but not
    executed from, that environment.

.NOTES
    Equivalent to: pnpm install (if needed) && pnpm tauri dev
#>

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $RepoRoot
try {
    foreach ($tool in @('pnpm', 'cargo')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "'$tool' was not found on PATH. Install the Rust + Node/pnpm toolchain first (see docs/architecture-audit.md / README.md)."
        }
    }

    if (-not (Test-Path (Join-Path $RepoRoot 'node_modules'))) {
        Write-Host 'Installing frontend dependencies (pnpm install)...' -ForegroundColor Cyan
        pnpm install
        if ($LASTEXITCODE -ne 0) { throw 'pnpm install failed.' }
    }

    Write-Host 'Starting Tauri dev (hot reload)...' -ForegroundColor Cyan
    pnpm tauri dev
    if ($LASTEXITCODE -ne 0) { throw 'tauri dev exited with a non-zero status.' }
}
finally {
    Pop-Location
}
