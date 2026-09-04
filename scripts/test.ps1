#Requires -Version 5.1
<#
.SYNOPSIS
    Runs the full test suite: Rust unit tests + frontend typecheck/lint.

.DESCRIPTION
    For a Windows dev machine with the full toolchain installed. This
    repo's Phase 2 scaffold was instead verified on a remote Ubuntu build
    server (`cargo test`, `pnpm run check`, `pnpm run lint`, `pnpm run
    build` all passed there — see IMPLEMENTATION_PLAN.md). This script
    mirrors that same command sequence for a real Windows box; it has not
    itself been executed here.

.NOTES
    There is no frontend unit-test runner yet (no vitest suite exists as of
    Phase 2 — nothing meaningful to unit-test above the placeholder panels).
    Add one here when Phase 4+ introduces real frontend logic (timeline
    cut-algebra, stores, etc.).
#>

$ErrorActionPreference = 'Stop'
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $RepoRoot
try {
    Write-Host 'Rust: cargo test (src-tauri)...' -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot 'src-tauri')
    try {
        cargo test
        if ($LASTEXITCODE -ne 0) { throw 'cargo test failed.' }

        Write-Host 'Rust: cargo clippy...' -ForegroundColor Cyan
        cargo clippy --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'cargo clippy failed.' }

        Write-Host 'Rust: cargo fmt --check...' -ForegroundColor Cyan
        cargo fmt --check
        if ($LASTEXITCODE -ne 0) { throw 'cargo fmt --check failed (run `cargo fmt` to fix).' }
    }
    finally {
        Pop-Location
    }

    Write-Host 'Frontend: pnpm run lint...' -ForegroundColor Cyan
    pnpm run lint
    if ($LASTEXITCODE -ne 0) { throw 'pnpm run lint failed.' }

    Write-Host 'Frontend: pnpm run check (svelte-check + tsc)...' -ForegroundColor Cyan
    pnpm run check
    if ($LASTEXITCODE -ne 0) { throw 'pnpm run check failed.' }
}
finally {
    Pop-Location
}
