//! Phase 2 build script: just the standard Tauri config/asset processing.
//!
//! FFmpeg/FFprobe sidecar fetching (autocut's build.rs does this — see
//! vendor/autocut/src-tauri/build.rs for the pattern) is deliberately NOT
//! wired up here yet. `tauri.conf.json` declares no `externalBin` in Phase 2
//! because we have not yet made the binary-provenance/checksum decision
//! flagged in docs/architecture-audit.md §6 risk #7 — that lands in Phase 3.
//! Adding a fake/empty sidecar fetch now would be exactly the kind of
//! "looks done but isn't" scaffolding master prompt §75 forbids.

fn main() {
    tauri_build::build();
}
