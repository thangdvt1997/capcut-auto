# AI Video Editor

A Windows 10/11 x64 desktop video editor (Tauri 2 + Rust + Svelte 5) built around one idea: **AI helps you edit, it never edits for you.** Every AI suggestion — a silence cut, a filler-word removal, a semantic edit recommendation, a generated short — is a proposed change you preview and explicitly approve before it touches your timeline. Nothing renders or mutates a project without a click.

It unifies the editing model of [autocut](https://github.com/cobanov/autocut) (silence/speech-based cutting) with the CapCut/Jianying draft-export approach of [capcut-mate](https://github.com/Hommy-master/capcut-mate) into one coherent, non-destructive editor — plus real local transcription, captions, an AI edit-plan pipeline, and a short-form video generator, none of which either upstream project had.

> **Status**: actively developed against the full spec in [`MASTER PROMPT — BUILD AI VIDEO EDITOR FOR WINDOWS.md`](./MASTER%20PROMPT%20—%20BUILD%20AI%20VIDEO%20EDITOR%20FOR%20WINDOWS.md). The Rust backend and its Svelte frontend are functionally complete through Phase 11 (see [`IMPLEMENTATION_PLAN.md`](./IMPLEMENTATION_PLAN.md) for the live, per-phase checklist and what's verified vs. still open). Windows packaging (Phase 12) and a final testing/performance/security pass (Phase 13) are in progress. This is not yet a signed, publicly-distributed release.

## Screenshots

*(placeholder — screenshots will be added once a real signed build exists to capture from; see [Troubleshooting](#troubleshooting) for why development builds can't currently produce one on the reference machine)*

## Features

- **Non-destructive timeline** — multi-track video/audio/caption/overlay editing, split/trim/move/duplicate, command-based undo/redo, track lock/hide/mute/solo, snapping, sync groups for multi-camera/multi-mic setups.
- **Silence & filler-word removal** — real Silero VAD-based speech detection and English/Vietnamese filler-word detection, both as a review-then-apply candidate workflow, never an automatic cut.
- **Local transcription** — real `whisper.cpp`-backed transcription (CPU by default, optional CUDA build), word-level timestamps, a Model Manager for the standard Whisper model sizes.
- **Captions** — generation from transcript, six built-in style templates (Minimal/TikTok/Podcast/News/Gaming/Karaoke), efficient karaoke-style active-word highlighting, split/merge/retime/find-replace/bulk-style correction tools.
- **Rendering** — a real FFmpeg-based render engine with N-track compositing, hardware-encoder detection (NVENC/QSV/AMF with a software fallback), the master prompt's export presets, and an FCPXML exporter for Premiere/Resolve/Final Cut.
- **CapCut/Jianying export** — a from-scratch Rust port of the draft-format class model (not an HTTP call into another app), mapping this app's captions/clips/keyframes onto a real CapCut draft folder.
- **AI edit-plan pipeline** — a provider-agnostic AI layer (OpenAI-compatible/Anthropic/Gemini/Ollama/custom endpoint) that only ever produces a strictly-schema-validated `EditPlan`, never raw executable output; Smart Edit semantic-editing suggestions, a natural-language command box, and AI-assisted highlight detection all build on it.
- **Short Video Generator** — Transcription → Highlight Detection → Candidate Ranking → Clip Extraction → Auto-Reframe → Captions → Optional Zoom → a real, still-editable project per generated short.
- **Batch processing** — a real job queue running a configurable multi-stage pipeline across many source files, with pause/resume/cancel/retry and live progress.
- **Bilingual UI** — English and Vietnamese, hand-written (not machine-translated), switchable at runtime.

See [`docs/feature-matrix.md`](./docs/feature-matrix.md) for exactly which features transfer through which export path (Internal/FFmpeg/CapCut/FCPXML) — it's read by the app's own export-warning UI, not just documentation.

## Windows requirements

- Windows 10 or 11, x64.
- A GPU with up-to-date drivers is recommended for hardware-accelerated encoding (NVENC/QSV/AMF); the app falls back to software (`libx264`) encoding automatically when none is detected.
- No separate FFmpeg/CapCut install is required — FFmpeg ships bundled (see [FFmpeg information](#ffmpeg-information)); CapCut/Jianying integration is optional and only activates if one of those apps is already installed.

## Installation

An installer (`AI-Video-Editor-Setup-x64.exe`, NSIS-based) is produced by Phase 12's Windows packaging work — see `IMPLEMENTATION_PLAN.md`'s Phase 12 section for the exact current state (installer configuration, code-signing, and auto-update hosting are tracked there, with any unsigned/placeholder status called out explicitly). Until a signed release is published, running from source (below) is the supported path.

## Development setup

Prerequisites:

- [Rust](https://rustup.rs/) (stable, matches `src-tauri/Cargo.toml`'s `rust-version = "1.77"` floor)
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)
- The [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for Windows (WebView2, MSVC Build Tools)
- FFmpeg + FFprobe available on `PATH` for local development (a bundled sidecar is only resolved in a packaged build — see [FFmpeg information](#ffmpeg-information))

```sh
pnpm install
pnpm run dev        # frontend-only Vite dev server (no Tauri/Rust involved — useful for pure UI/layout iteration)
cargo tauri dev      # full app, from src-tauri/ (requires the Tauri CLI: cargo install tauri-cli --version "^2.0.0")
```

Rust-only iteration (no need to relaunch the whole app):

```sh
cd src-tauri
cargo test --lib          # the real, non-mocked-where-avoidable test suite
cargo clippy --all-targets -- -D warnings
cargo run --bin export_bindings   # regenerate src/types/bindings.ts after touching any #[tauri::command]
```

Frontend-only checks:

```sh
pnpm run check   # svelte-check, strict TS
pnpm run lint    # eslint
pnpm run build   # production Vite build
```

**Never hand-edit `src/types/bindings.ts`** — it's generated by `specta`/`tauri-specta` from the real Rust command signatures; regenerate it via `cargo run --bin export_bindings` after any backend change.

## Build instructions

```sh
cargo tauri build
```

Produces a release binary plus (once Phase 12's installer configuration is finalized) an NSIS installer under `src-tauri/target/release/bundle/`. See `IMPLEMENTATION_PLAN.md`'s Phase 12 section for the current, exact state of installer/signing/auto-update configuration before relying on this for a distributable build.

## Architecture overview

- **Backend** (`src-tauri/src/`): a Rust core organized by domain — `project/` (the `ProjectV1` schema, i64-microsecond timebase everywhere), `timeline/` (command-based edit primitives + undo/redo), `media/`/`ffmpeg/`/`audio/`/`render/` (probing, thumbnails, proxies, the FFmpeg filter-graph render engine), `vad/` + `transcription/` + `captions/` (speech detection, Whisper, caption generation/styling), `capcut/` (the CapCut/Jianying draft adapter) and `fcpxml/` (the other export adapter), `ai/` (the provider-agnostic AI layer + EditPlan schema), `highlights/`/`reframe/`/`zoom/`/`broll/`/`templates`/`shorts/`/`batch/` (the Short Video Generator's subsystems), `db/` (a separate SQLite local media index — deliberately not part of `project.json`).
- **Frontend** (`src/`): Svelte 5 (runes-based), TypeScript strict. `stores/` hold Svelte-5-runes application state talking to the backend via the generated `commands` object; `components/` are organized by feature area matching the backend's own domains.
- Full component-level design and the reasoning behind major decisions (the i64-µs timebase, non-destructive editing, separate media-index database, provider-abstraction pattern used throughout) is in [`docs/architecture.md`](./docs/architecture.md). The on-disk project schema is documented in [`docs/project-format.md`](./docs/project-format.md). Where code originates from `autocut`/`capcut-mate` is tracked module-by-module in [`docs/upstream.md`](./docs/upstream.md).

## AI configuration

AI features (Smart Edit, the natural-language command box, AI-assisted highlight scoring, B-roll/tag suggestions) are entirely optional and off by default. Configure a provider from the app's AI Settings dialog: OpenAI-compatible (also covers Ollama and any custom OpenAI-compatible endpoint), Anthropic, or Google Gemini — provider, base URL, model, temperature, and timeout. **API keys are never stored in `project.json`** — they're written to the real Windows Credential Manager and never read back to the frontend; only an opaque reference is kept in app settings. Every AI response is parsed into a closed, strictly-typed schema before anything happens with it — a malformed or unexpected response is rejected outright, never partially trusted.

## Transcription models

Transcription runs entirely locally via a real, vendored build of `whisper.cpp` (no audio is ever sent anywhere). Open the Model Manager to download one of the standard model sizes (tiny/base/small/medium/large — larger models are more accurate but slower and use more disk space); downloads are resumable and only counted as installed once fully verified. CPU inference is the default and the only path currently verified end-to-end; a `cuda` Cargo build feature exists for GPU inference but has not been built or verified on a CUDA-toolkit-equipped machine — don't rely on it until that's done for real.

## CapCut integration

The app detects an installed CapCut or Jianying Pro on Windows (both the China-region and international folder conventions) and can export a project as a real CapCut/Jianying draft — a from-scratch Rust port of the draft-format class model, not a call into CapCut itself. See [`docs/feature-matrix.md`](./docs/feature-matrix.md) for exactly which features transfer (captions, transform, and speed all map for real; effects/filters/transitions do not, since this app has no catalog for them yet). **Draft compatibility has not yet been validated against a real, installed CapCut build** — that verification needs a human with CapCut actually installed and is tracked as an open item in `IMPLEMENTATION_PLAN.md`'s Phase 9 section, not something to assume works.

## FFmpeg information

FFmpeg/FFprobe are bundled as sidecar binaries rather than requiring a separate system install. See `IMPLEMENTATION_PLAN.md`'s Phase 12 section and `THIRD_PARTY_NOTICES.md` for the exact chosen build source, version, and license variant (an LGPL build is preferred specifically so GPL-only codecs never trigger a copyleft obligation on the rest of this app) — that section is the authoritative, current source of truth for exactly what's bundled and how it was verified, rather than restating version numbers here where they'd go stale.

## Troubleshooting

- **The app won't launch a freshly-built binary during development on some machines**: Windows Smart App Control can silently start blocking execution of newly-compiled, unsigned binaries mid-project on a given machine (this happened during this project's own development — see `HANDOFF.md`'s "Build/test environment" section for the full story and the WSL2-based Rust workflow used to work around it for backend iteration). A **signed** release build is not affected by this the way an ad-hoc local dev build can be.
- **A generated CapCut draft doesn't look right in CapCut**: see the [CapCut integration](#capcut-integration) note above — real-CapCut validation is an open item, not a solved problem yet. Please report specifics (CapCut vs. Jianying, version, what looked wrong) as an issue.
- **Transcription is slow**: CPU inference is the only currently-verified path; try a smaller model size in the Model Manager first.
- **Logs**: structured logs are written to `%LOCALAPPDATA%\AI Video Editor\logs\` (see `IMPLEMENTATION_PLAN.md`'s Phase 12 section for the exact crash-handling/logging setup) — include the relevant log file when reporting a bug.

## License

This project's own license has not yet been finalized/added (no `LICENSE` file exists in the repo root as of this writing) — that decision belongs to the project owner and is tracked as an open item ahead of any public release. Do not assume any particular license applies until a `LICENSE` file is added here.

## Third-party notices

See [`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md) for full attribution and license text for `pyJianYingDraft`/`capcut-mate` (Apache-2.0), the `capcut-mate` desktop-client draft-path-detection code (MIT), `autocut` (used with the author's direct permission — see that file for the exact status), the Silero VAD model/crate, FFmpeg/FFprobe, and every other bundled dependency.
