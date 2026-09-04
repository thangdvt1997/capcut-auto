#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the frontend and the Rust binary (debug-check level, no
    installer). Use scripts/package.ps1 for a distributable installer.

.DESCRIPTION
    For a Windows dev machine with the full toolchain installed. Verified
    on this project's remote Ubuntu build server instead (frontend `pnpm
    run build` and Rust `cargo build`/`cargo check` both succeeded there —
    see IMPLEMENTATION_PLAN.md Phase 2). Written for, not executed from,
    d:\work-out.
#>

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $RepoRoot
try {
    Write-Host 'Installing frontend dependencies...' -ForegroundColor Cyan
    pnpm install
    if ($LASTEXITCODE -ne 0) { throw 'pnpm install failed.' }

    Write-Host 'Building frontend (vite build)...' -ForegroundColor Cyan
    pnpm run build
    if ($LASTEXITCODE -ne 0) { throw 'pnpm run build failed.' }

    Write-Host 'Building Rust core (cargo build)...' -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot 'src-tauri')
    try {
        cargo build
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed.' }
    }
    finally {
        Pop-Location
    }

    Write-Host 'Build complete.' -ForegroundColor Green
}
finally {
    Pop-Location
}
