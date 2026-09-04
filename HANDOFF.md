# HANDOFF — AI Video Editor (capcut-auto)

Written 2026-09-04 so work can resume on a different machine/agent session without losing context. Read this first, then follow the pointers below — do not re-derive decisions that are already made and documented.

## What this project is

Building "AI Video Editor": a Windows 10/11 x64 desktop app (Tauri 2 + Rust + Svelte 5) that unifies two open-source repos into one coherent product — silence/speech auto-cutting, CapCut/Jianying draft export, transcription, AI semantic editing, captions, shorts generation, etc.

**The spec is** `MASTER PROMPT — BUILD AI VIDEO EDITOR FOR WINDOWS.md` at the repo root — 90 sections, read it, it is the actual product requirements, not a summary of them. It explicitly instructs: audit first, then implement phase by phase, compiling/testing/committing at the end of each phase, never faking a feature or claiming something works when it wasn't verified (§75, §90-93).

## Where things stand right now

**Repo**: `https://github.com/thangdvt1997/capcut-auto` (pushed, `main` branch, up to date as of this commit).

**Read `IMPLEMENTATION_PLAN.md` for the authoritative, live checklist** — it is kept in sync with reality after every phase; don't assume phase status from this handoff alone if the two ever disagree, trust `IMPLEMENTATION_PLAN.md`.

As of now:
- **Phase 0 (repo audit)** — done. `docs/architecture-audit.md` has the full findings on both upstream repos (`capcut-mate`, `autocut`), cloned read-only into `vendor/` (gitignored, not pushed — re-clone if needed, HEAD hashes are recorded in the audit doc).
- **Phase 1 (architecture + schema)** — done. `docs/architecture.md` (component design), `docs/project-format.md` (the full `project.json`/`ProjectV1` schema — **i64 microseconds is the canonical timebase everywhere**, this is a load-bearing decision, don't reintroduce float seconds/milliseconds into core types), `docs/upstream.md` (per-module provenance), `THIRD_PARTY_NOTICES.md`.
- **Phase 2 (Tauri shell)** — done. Real, compiling Tauri 2 + Rust + Svelte 5 (TS strict) app. `specta`/`tauri-specta` generates `src/types/bindings.ts` — never hand-edit it, regenerate via `cargo run --bin export_bindings` inside `src-tauri/`. `src-tauri/src/project/` is fully implemented (`ProjectV1` struct tree, atomic save/load, migration dispatch, error envelope). Other module folders (`ai/`, `capcut/`, `timeline/`, `fcpxml/`, `jobs/`) were empty placeholders as of Phase 2 — check Phase 3 below, `media/`/`ffmpeg/`/`audio/`/`db/` are no longer placeholders.
- **Phase 3 (media engine)** — done, commit `44f0bd0`. `media/probe.rs` (ffprobe wrapper), `ffmpeg/{binaries,command}.rs` (argument-array builder, no shell string concat), `audio/{pcm,waveform}.rs`, `media/{thumbnail,proxy,import}.rs`, `db/` (SQLite media index, separate from `project.json` per master prompt §35), plus a real `MediaLibrary.svelte` + `VideoPlayer.svelte` frontend.
- **Phase 4 onward (timeline, autocut integration, rendering, transcription, captions, CapCut adapter, AI, shorts, packaging, testing)** — not started. Follow `IMPLEMENTATION_PLAN.md`'s phase order; each phase's exact task list is written out there already, don't re-plan from the master prompt each time — the plan file already breaks it down.

## Decisions already made — don't re-litigate these

1. **Timebase: `i64` microseconds**, everywhere in the core model. Conversions to FFmpeg (seconds) and FCPXML (rational frames) happen only at those adapters' boundaries. See `docs/architecture.md`'s "Timebase conversion boundaries" table.
2. **autocut code may be copied/ported directly, not just referenced.** The project owner (via chat, not a written grant on file) said permission was obtained from the autocut author, Mert Cobanov, despite the upstream repo carrying no LICENSE file. This is recorded in `docs/upstream.md` and `THIRD_PARTY_NOTICES.md`. If this ever needs to hold up to outside scrutiny, get the grant in writing and link it from those two files — right now it's a verbal/chat-relayed permission, not a documented one.
3. **capcut-mate's draft engine (`pyJianYingDraft`) is a direct Rust port**, Apache-2.0, no licensing concern. Its FastAPI/HTTP/cloud-storage/RPA-rendering layers are explicitly NOT ported (this app is local-only, no HTTP server, no Docker).
4. **No Python runtime shipped.** Everything reusable from capcut-mate is ported to Rust, not run via a Python sidecar.
5. **Bundle identifier**: `dev.aivideoedit.app`.
6. **Windows ffmpeg/ffprobe binary sourcing** (which exact build, checksum, license variant) is explicitly deferred to Phase 12 (master prompt §59) — don't let an earlier phase quietly lock in a choice here; dev/test work so far uses the build server's apt-installed `ffmpeg`/`ffprobe` 4.4.2 via a PATH fallback, which is NOT the shipping decision.
7. **CapCut/Jianying automated rendering via RPA** (capcut-mate's `jianying_controller.py` approach) is deferred to P2-or-later, optional/experimental only — never the primary render path. Local FFmpeg rendering (Phase 6) is the default, always-available path.

## Build/test environment — likely machine-specific, re-verify on the new machine

Work so far happened with the local Windows machine having **no admin rights** (couldn't install Rust/Node/pnpm), so all compiling/testing ran on a remote Ubuntu 22.04 build server instead:

- Host `198.204.229.10` (`s251717.nocix.net`), user `root`. **Password is intentionally not written here or anywhere in this repo — ask the project owner for it directly when a build/test session needs it.** SSH host key fingerprint (already trusted from prior sessions): `SHA256:RtwrtsQWYCH+vuxW+TJhVHM3pbubAUKzDJvg2j8hsKY`.
- That server already has: rustc 1.98.1, cargo, `x86_64-pc-windows-gnu` rust target, Node v24, pnpm, tauri-cli 2.11.4, Tauri Linux build deps (webkit2gtk-4.1 etc.), mingw-w64/nsis, and (as of Phase 3) `ffmpeg`/`ffprobe` 4.4.2 via apt.
- **If the new machine is a real Windows box with admin rights**, prefer building/testing natively there instead — it removes the two biggest unverified gaps so far: (a) whether the app's GUI actually opens/renders (never had a display to check), and (b) real Windows linking (`cargo build --target x86_64-pc-windows-gnu` hit a mingw-w64 `ld` "export ordinal too large" limitation on the Linux cross-compile — a native Windows or MSVC toolchain likely doesn't have this problem, but it's unconfirmed).
- **Tool gotcha if still using plink/pscp from a Windows agent session**: use the **PowerShell tool**, not a Bash/Git-Bash tool, for any `plink`/`pscp` call containing an absolute path like `/root/...` — Git Bash silently mangles such paths into local Windows paths first, causing confusing failures that look remote-side but aren't.

## How to resume work

1. Read `IMPLEMENTATION_PLAN.md`, find the first unchecked phase, read that phase's checklist section in full.
2. Read the specific `docs/*.md` files that phase references (each phase's tasks cite the relevant doc sections already).
3. Re-clone `vendor/capcut-mate` and `vendor/autocut` if their contents are needed again (they're gitignored, not in this repo's history) — HEAD commit hashes to match are recorded in `docs/architecture-audit.md`.
4. Confirm build/test environment (see above) before starting — don't assume the old remote server is still the right choice if the new machine can build natively.
5. Implement, compile, test, fix, update `docs/upstream.md` provenance table and `IMPLEMENTATION_PLAN.md` checkboxes honestly (checked only for what's actually verified), commit. Push to `origin/main` when the user asks (pushing hasn't been established as an every-commit default — confirm, same as any other repo).
