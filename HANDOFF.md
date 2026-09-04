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
- **i18n (master prompt §47) retrofitted, 2026-09-04**, not its own numbered phase but a real gap closed out-of-band: `src/lib/i18n.svelte.ts` (Svelte-5-runes `t()`/`setLocale()`/`currentLocale()`, localStorage-persisted, English+Vietnamese, both written by hand — not machine-transliterated) plus `src/locales/{en,vi}.json`. All Phase 2/3/4 UI strings retrofitted through it, plus an EN/Tiếng Việt switcher in `TopBar`. **Any new UI string added from here on must go through `t()` with real keys in both JSON files** — don't reintroduce hardcoded strings.
- **Phase 4 (timeline) — done, 2026-09-04.** Backend (`src-tauri/src/timeline/{ops,command,clipboard,session,error}.rs`, Tauri commands in `commands/timeline.rs`): split/trim/move/delete/duplicate, command-based undo/redo (bounded 100-entry history, never whole-project-copy), `SyncGroup` propagation, track lock/hide/mute/solo + effective-mute query, copy/paste, snap — 45 new Rust tests, 108/108 total passing. Frontend (`src/components/timeline/{Timeline,TrackHeader,ClipView,Ruler,Waveform,Markers}.svelte`, `src/timeline/algebra.ts`, `src/stores/timeline.svelte.ts`): zoom/scroll/multi-select/playhead/ruler/real waveform+thumbnail-strip/markers/virtualized rendering/full §49 keyboard shortcuts, visually verified running in the real GUI (see below). Two known, documented gaps, not silently papered over: **markers are frontend-only/session-local** (`ProjectV1` has no `markers` field yet — needs a schema decision in a later phase, they don't survive reload), and **there is no backend `insert_clip` command** (only `duplicate_clip`/edit-primitives/whole-project `load_timeline_project` exist) — getting Media Library items onto a fresh timeline is bridged client-side only (`stores/timeline.svelte.ts`'s `addMediaAsClip`, not undo-able) pending a real Project Manager phase. See `IMPLEMENTATION_PLAN.md` Phase 4 notes for the full detail.
- **Phase 5 onward (autocut/silence integration, rendering, transcription, captions, CapCut adapter, AI, shorts, packaging, testing)** — not started. Follow `IMPLEMENTATION_PLAN.md`'s phase order; each phase's exact task list is written out there already, don't re-plan from the master prompt each time — the plan file already breaks it down.

## Decisions already made — don't re-litigate these

1. **Timebase: `i64` microseconds**, everywhere in the core model. Conversions to FFmpeg (seconds) and FCPXML (rational frames) happen only at those adapters' boundaries. See `docs/architecture.md`'s "Timebase conversion boundaries" table.
2. **autocut code may be copied/ported directly, not just referenced.** The project owner (via chat, not a written grant on file) said permission was obtained from the autocut author, Mert Cobanov, despite the upstream repo carrying no LICENSE file. This is recorded in `docs/upstream.md` and `THIRD_PARTY_NOTICES.md`. If this ever needs to hold up to outside scrutiny, get the grant in writing and link it from those two files — right now it's a verbal/chat-relayed permission, not a documented one.
3. **capcut-mate's draft engine (`pyJianYingDraft`) is a direct Rust port**, Apache-2.0, no licensing concern. Its FastAPI/HTTP/cloud-storage/RPA-rendering layers are explicitly NOT ported (this app is local-only, no HTTP server, no Docker).
4. **No Python runtime shipped.** Everything reusable from capcut-mate is ported to Rust, not run via a Python sidecar.
5. **Bundle identifier**: `dev.aivideoedit.app`.
6. **Windows ffmpeg/ffprobe binary sourcing** (which exact build, checksum, license variant) is explicitly deferred to Phase 12 (master prompt §59) — don't let an earlier phase quietly lock in a choice here; dev/test work so far uses the build server's apt-installed `ffmpeg`/`ffprobe` 4.4.2 via a PATH fallback, which is NOT the shipping decision.
7. **CapCut/Jianying automated rendering via RPA** (capcut-mate's `jianying_controller.py` approach) is deferred to P2-or-later, optional/experimental only — never the primary render path. Local FFmpeg rendering (Phase 6) is the default, always-available path.

## Build/test environment

**Update, 2026-09-04: now building and testing natively on a real Windows 11 x64 machine with admin rights — both of the previously-biggest unverified gaps are resolved.** The remote Ubuntu server below is no longer needed for day-to-day work; keep this section only in case a future machine again lacks admin rights.

What's installed and confirmed working on the current Windows dev machine (all via `winget`, all this session):
- `Rustlang.Rustup` → rustc/cargo 1.98.1, `x86_64-pc-windows-**msvc**` target (not gnu) with rustfmt/clippy components.
- `Microsoft.VisualStudio.2022.BuildTools` with the `Microsoft.VisualStudio.Workload.VCTools` (C++) component — this is what provides `link.exe`; without it MSVC-target Rust builds fail at the link step with `error: linker \`link.exe\` not found`. Installed via `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --silent --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`. `cargo build`/`test`/`clippy` need the MSVC dev environment on `PATH`; easiest is running through `cmd /c "\"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat\" && ... && cargo ..."` since plain PowerShell doesn't have `link.exe` on `PATH` otherwise.
- `OpenJS.NodeJS.LTS` (v24) + `npm install -g pnpm` (pnpm 11.x) — `corepack enable` throws an `EPERM` on this machine's `pnpx` symlink, harmless, ignore it; the global npm install of pnpm itself works fine.
- `Gyan.FFmpeg` (ffmpeg/ffprobe 9.0.1) — needed for the ffmpeg-subprocess-backed Rust tests (`audio::pcm`, `ffmpeg::binaries`, `ffmpeg::command`, `media::proxy`, `media::thumbnail`) to pass; without it those 7 tests fail with "could not locate ffmpeg binary" (everything else still passes). **This is dev/test tooling only, not the Phase 12 shipping-binary decision** (§59) — don't conflate the two.
- **A new PowerShell/cmd session picks up a freshly-`winget`-installed tool's PATH change automatically; an already-open one does not** — if a tool "isn't found" right after installing it, that's almost always a stale-PATH problem in the current shell, not a real missing-tool problem. Re-derive `$env:Path` from the Machine+User registry values in a fresh command rather than assuming an old shell's cached PATH is current.
- **GUI verification now works**: the built exe (`src-tauri/target/debug/ai-video-editor.exe`) genuinely opens a window and renders (confirmed via screenshot, cropped to just the app window's `GetWindowRect` bounds — this machine's desktop has the user's own personal browser/app content visible, so always crop/avoid full-desktop captures and delete screenshot files after use). Windows display-scaling (100–200%) is still unverified — no test at other DPI settings has been done yet.
- Real Windows path semantics (drive letters, backslashes, UNC) are still not specifically exercised beyond what the app's own Windows-native filesystem calls do implicitly — no dedicated UNC/long-path test pass yet.

### Fallback: remote Ubuntu build server (only if a future machine has no admin rights)

- Host `198.204.229.10` (`s251717.nocix.net`), user `root`. **Password is intentionally not written here or anywhere in this repo — ask the project owner for it directly when a build/test session needs it.** SSH host key fingerprint (already trusted from prior sessions): `SHA256:RtwrtsQWYCH+vuxW+TJhVHM3pbubAUKzDJvg2j8hsKY`.
- That server has: rustc 1.98.1, cargo, `x86_64-pc-windows-gnu` rust target, Node v24, pnpm, tauri-cli 2.11.4, Tauri Linux build deps (webkit2gtk-4.1 etc.), mingw-w64/nsis, and `ffmpeg`/`ffprobe` 4.4.2 via apt. Cross-compiling to `x86_64-pc-windows-gnu` from there previously hit a mingw-w64 `ld` "export ordinal too large" link failure — unresolved there, but moot now that native MSVC builds work.
- **Tool gotcha if still using plink/pscp from a Windows agent session**: use the **PowerShell tool**, not a Bash/Git-Bash tool, for any `plink`/`pscp` call containing an absolute path like `/root/...` — Git Bash silently mangles such paths into local Windows paths first, causing confusing failures that look remote-side but aren't.

## How to resume work

1. Read `IMPLEMENTATION_PLAN.md`, find the first unchecked phase, read that phase's checklist section in full.
2. Read the specific `docs/*.md` files that phase references (each phase's tasks cite the relevant doc sections already).
3. Re-clone `vendor/capcut-mate` and `vendor/autocut` if their contents are needed again (they're gitignored, not in this repo's history) — HEAD commit hashes to match are recorded in `docs/architecture-audit.md`.
4. Confirm build/test environment (see above) before starting — don't assume the old remote server is still the right choice if the new machine can build natively.
5. Implement, compile, test, fix, update `docs/upstream.md` provenance table and `IMPLEMENTATION_PLAN.md` checkboxes honestly (checked only for what's actually verified), commit. Push to `origin/main` when the user asks (pushing hasn't been established as an every-commit default — confirm, same as any other repo).
