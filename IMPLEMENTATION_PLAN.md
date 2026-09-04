# IMPLEMENTATION_PLAN.md — AI Video Editor

Living checklist. Update `[ ]` → `[x]` as work completes. Phases follow the master prompt's §76 "DEVELOPMENT EXECUTION PLAN" (Phase 0–13). At the end of every phase: compile, run tests, fix errors, update docs, commit.

Legend: `[x]` done · `[ ]` not started · `[~]` partially done / blocked

---

## PHASE 0 — Repository audit

- [x] Clone `capcut-mate` into `vendor/capcut-mate` (HEAD `ac0136a751361c22031650782a08abb652254df0`, 2026-09-01)
- [x] Clone `autocut` into `vendor/autocut` (HEAD `a17e9fa4988a4b1f0de8c19ec9296d8940f89e3a`, 2026-08-31)
- [x] Record HEAD commit hashes for both repos
- [x] Read `capcut-mate/LICENSE` (Apache-2.0) and `NOTICE` (pyJianYingDraft © Gary Guan 2024; modifications © Hommy 2026, both Apache-2.0)
- [x] Read `capcut-mate/desktop-client/LICENSE` (MIT, © gogoshine 2025 — separate author/license from main tree)
- [x] Confirm `autocut` has **no LICENSE file, no `license` field, no license grant anywhere** (verified via recursive filesystem search + GitHub API `"license": null`)
- [x] Map `capcut-mate` directory tree (FastAPI app, `src/router`, `src/schemas`, `src/service`, `src/pyJianYingDraft`, `src/utils`, `desktop-client/`, `config/`, `template/`, `tests/`, Docker files)
- [x] Map `autocut` directory tree (`src-tauri/src/*.rs`, `src/components`, `src/lib`, build/config files)
- [x] Identify capcut-mate's API routes (34 endpoints in `src/router/v1.py`), services, schemas, auth/middleware model, HTTP-200-always response envelope
- [x] Identify capcut-mate's draft-format engine (`src/pyJianYingDraft/`: `ScriptFile`, `Track`, segments, materials, animations, keyframes; microsecond timebase confirmed)
- [x] Identify capcut-mate's Windows RPA rendering path (`jianying_controller.py`, `gen_video` pipeline, `video_task_manager.py`) and its China-region-only validation
- [x] Identify capcut-mate's desktop-client Electron wrapper and its draft-path-detection heuristic (`draftPathDetect.js`)
- [x] Identify autocut's Rust backend modules (`commands.rs`, `probe.rs`, `audio.rs`, `vad.rs`, `cutlist.rs`, `timecode.rs`, `sync.rs`, `export_mp4.rs`, `export_fcpxml.rs`, `waveform.rs`, `binaries.rs`) and confirm Silero-V5-via-ONNX VAD implementation
- [x] Identify autocut's frontend structure (`App.svelte`, `lib/types.ts`, `lib/api.ts`, `lib/cuts.ts`, `lib/store.svelte.ts`, components) and IPC contract shape
- [x] Confirm autocut's internal timebase is f64 seconds (not integer microseconds) — a mismatch vs. the target project's mandated timebase
- [x] Assess autocut's multi-track/sync model in depth — determined it does **not** generalize to a full multi-clip timeline (single shared CutList + per-track scalar offset, built for a fixed camera/audio rig)
- [x] Identify Windows support status in both repos (capcut-mate: RPA half is Windows-only/China-region-only; autocut: genuine but weaker-CI-covered Windows x86_64 target, no ARM64)
- [x] Create `docs/architecture-audit.md` with all 9 required sections (capcut-mate architecture, autocut architecture, reusable components, duplicate functionality, incompatible components, technical risks, licensing considerations, integration strategy, proposed final architecture)
- [x] Propose final unified repository structure (documented in `docs/architecture-audit.md` §9)
- [x] Create this file, `IMPLEMENTATION_PLAN.md`, with the full phased checklist
- [x] **Human decision on autocut licensing**: project owner confirmed 2026-09-04 that permission to reuse the code was obtained directly from the author, Mert Cobanov — autocut is treated as directly portable, not reimplement-only. Recorded in `docs/upstream.md` and `THIRD_PARTY_NOTICES.md`.

---

## PHASE 1 — Architecture + unified project schema

- [x] Write `docs/architecture.md` (high-level component diagram, expands on audit §9)
- [x] Write `docs/project-format.md`: finalize `project.json` schema (version, project, canvas, media, tracks, clips, captions, transcript, effects, animations, keyframes, cuts, ai, export) per master-prompt §5
- [x] Define canonical internal timebase: **i64 microseconds**, matching capcut-mate's `pyJianYingDraft::time_util` convention (audit §1); document conversion utilities needed at FFmpeg (seconds), FCPXML (rational frames), and CapCut (µs, no conversion) boundaries — per master-prompt §67
- [x] Design `ProjectV1` schema (JSON shape in `docs/project-format.md`; Rust struct itself lands in Phase 2 scaffold) with stable UUID-based IDs for all timeline entities (no positional array references) per master-prompt §5
- [x] Design migration/versioning layer (`ProjectV1` → `ProjectV2` scaffold, even if V2 doesn't exist yet) — documented in `docs/project-format.md`
- [x] Design the general multi-clip timeline data model (tracks-of-clips, not autocut's single-CutList-plus-offset model — audit §4/§5) — `Track`/`Clip` schema in `docs/project-format.md`
- [x] Design `SyncGroup`/`ClipGroup` concept for linked-track behavior (the one part of autocut's sync model worth generalizing — audit §4) — documented in `docs/project-format.md`
- [x] Design `RenderGraph` intermediate representation (master-prompt §69): Project → RenderGraph → FFmpeg Plan → FFmpeg — documented in `docs/architecture.md`
- [x] Design `CapCutExportGraph` intermediate representation (master-prompt §70): Project → CapCutExportGraph → CapCutAdapter → Draft — documented in `docs/architecture.md`
- [x] Design standardized error model (`MediaError`, `FfmpegError`, `TranscriptionError`, `AiProviderError`, `CapCutError`, `ProjectError`, `RenderError`, `ModelError`) per master-prompt §56 — documented in `docs/project-format.md`
- [x] Write `docs/upstream.md`: table tracking which modules originate from capcut-mate (ported, Apache-2.0), which are reimplemented/ported from autocut (permission obtained from author), and which are wholly new
- [x] Write `THIRD_PARTY_NOTICES.md`: Apache-2.0 NOTICE text (capcut-mate/pyJianYingDraft), MIT notice (desktop-client), autocut attribution note (permission-based, no upstream license text to reproduce), FFmpeg license placeholder (TBD after binary sourcing decision in Phase 12)
- [x] Compile check: N/A (no code yet) — reviewed schema docs for internal consistency instead
- [x] Commit Phase 1 deliverables (bundled with Phase 2 commit)

---

## PHASE 2 — Tauri Windows shell

- [x] Scaffold `src-tauri/` (Tauri 2, Rust 2021 edition) and `src/` (Svelte 5 + TypeScript, Vite) per the structure in `docs/architecture-audit.md` §9. Module folders under `src-tauri/src/` (`commands/`, `media/`, `ffmpeg/`, `audio/`, `vad/`, `timeline/`, `render/`, `project/`, `capcut/`, `ai/`, `fcpxml/`, `jobs/`, `db/`) all exist and compile; all are empty-but-honest placeholders (a one-paragraph doc comment naming which phase implements them) **except `project/`**, which is fully implemented per `docs/project-format.md` — the complete `ProjectV1` struct tree (`project/types.rs`), the `{code,message,details,recoverable,suggested_action}` error envelope for `ProjectError` (`project/error.rs`), and atomic save/load + version-dispatching `migrate_to_latest` (`project/io.rs`), with 7 passing unit tests (schema round-trip, atomic-write round-trip, migration rejection paths, error-payload mapping).
- [x] Configure `tauri.conf.json`: window layout (resizable, 1440×900 default, 1024×640 min, per master-prompt §48), bundle identifier **`dev.aivideoedit.app`** (engineering choice — reverse-DNS under a product-owned-style domain, distinct from autocut's `dev.cobanov.autocut` and capcut-mate's namespace), a real CSP (`default-src 'self'` with narrow, justified exceptions for `asset:`/`ipc:` schemes Tauri itself needs — replacing autocut's flagged `"csp": null`), `assetProtocol` scope restricted to `$APPLOCALDATA/projects/**` (a placeholder pattern — replacing autocut's unscoped `["**/*"]` — flagged for revision once the real project/media directory model lands, likely Phase 3/6).
- [x] Register minimal Tauri 2 capabilities file (`core:default` + `dialog:default` only — no shell, no fs-scope beyond the asset protocol above — matching autocut's least-privilege discipline; expand one permission at a time as later phases need them).
- [x] Set up `specta` + `tauri-specta` (pinned to the current 2.0.0-rc versions actually published on crates.io — specta 2 is still rc-only, confirmed against the live index) so `src/types/bindings.ts` is generated from Rust command/type signatures, not hand-mirrored. Verified end-to-end: `get_shell_info`/`new_project` commands and the full `ProjectV1` type tree generate correct TypeScript (including the `serde_json::Value`→`JsonValue` and `i64`→`number` bigint-policy decisions, documented in `src-tauri/src/lib.rs`). A standalone `cargo run --bin export_bindings` entry point regenerates the file without launching a GUI (works headlessly).
- [x] Implement app shell: `TopBar` (menu labels + a working "New Project" button that calls the real `new_project` command, plus a live shell-info status chip proving the IPC round-trip), resizable panel layout matching master-prompt §48's diagram exactly (Media/Transcript/Templates/AI tabs left, Video Preview center, Inspector/AI Edit/Properties tabs right, Timeline docked bottom with a static V2/V1/A1/CC track mockup), built from a real `ResizableSplit` component (ported from autocut) with `localStorage`-persisted ratios per split. All panel *content* is placeholder, clearly labeled "X — Phase N"; the resizing/persistence mechanism itself is real and not mocked.
- [x] Implement Windows-specific basics: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in `main.rs` (pattern from autocut, verified to compile). **Windows display-scaling (100–200%) is explicitly UNVERIFIED** — this headless Linux build server has no display and no way to test DPI scaling; needs a real Windows machine. Documented here and in `docs/architecture-audit.md`-style follow-up rather than claimed done.
- [x] Implement `scripts/dev.ps1`, `scripts/test.ps1`, `scripts/build.ps1`, `scripts/package.ps1` — real, non-stub PowerShell scripts (tool-presence checks, proper error propagation via `$LASTEXITCODE`, `$ErrorActionPreference = 'Stop'`) that a Windows dev machine with the toolchain installed could run as-is. **Not executed here** (no admin rights / no local Rust+Node) — verification instead happened by running the equivalent commands directly on the remote Ubuntu build server; each script's header comment says so explicitly.
- [x] Set up GitHub Actions CI (`.github/workflows/ci.yml`): a `frontend` job (ubuntu-latest: lint, svelte-check/tsc, vite build) and a `windows` job (**windows-latest**, every PR — not `workflow_dispatch`-only like autocut's gap, audit §6 risk #9: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`, frontend build, `tauri build --no-bundle` as a full-pipeline compile check). **Not run on real GitHub Actions** (no CI trigger available here) — the equivalent command sequence was run and passed on the remote Linux build server (Rust side) and is expected, not guaranteed, to behave the same on `windows-latest`; flag this if the first real CI run surfaces a Windows-only issue.
- [x] `cargo build` succeeds — verified on the remote Ubuntu server (native Linux target) and via `cargo check --target x86_64-pc-windows-gnu` (type-checks clean cross-compiling for Windows). A full **link** for the Windows GNU target failed with a known mingw-w64 `ld` limitation (`export ordinal too large`, a symbol-table overflow from Rust's v0 mangling colliding with MinGW's PE export limits) unrelated to this project's code — real Windows linking needs either a native Windows toolchain or an MSVC-based cross toolchain (e.g. `cargo-xwin`), neither set up here. **`npm run dev`/`tauri dev` opening an actual window is UNVERIFIED** — this is a headless server with no display; `tauri build --no-bundle` (full release compile against the real frontend bundle) succeeded instead as the closest available substitute.
- [x] Compile, run tests, fix errors, update docs, commit. Full clean-checkout verification on the remote server, all green: `pnpm install` (frozen lockfile), `pnpm run lint` (eslint), `pnpm run check` (svelte-check + tsc, 0 errors), `pnpm run build` (vite), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (0 warnings), `cargo test` (7/7 passing), `cargo tauri build --no-bundle` (release binary produced).

---

## PHASE 3 — Media engine

- [x] Implement `src-tauri/src/media/` — ffprobe wrapper, reimplemented from autocut's `probe.rs` design (direct `ffprobe -show_streams -show_format` JSON parsing, chosen over capcut-mate's `pymediainfo` dependency — audit §4): `ProbedMedia` struct with duration_us/fps (exact `Rational`)/width/height/codec/bitrate/audio channels/sample rate/rotation/creation timestamp per master-prompt §7. Verified against real ffmpeg-generated fixtures on the remote server (13 unit tests) plus JSON-fixture tests for rotation (legacy `tags.rotate` and modern `side_data_list` Display Matrix), fps fallback edge cases, and creation-time fallback.
- [x] Implement FFmpeg command-builder abstraction (`src-tauri/src/ffmpeg/command.rs`) — `FfmpegArgs` process argument arrays, never string concatenation (master-prompt §66/§88); also a `-progress pipe:1` block parser/runner (`run_with_progress`) used by proxy generation. Verified: argument-array tests including Vietnamese/space-containing paths (master-prompt §88's own examples), progress-block parsing tests, and a real subprocess failure-surfacing test.
- [x] Implement FFmpeg/FFprobe sidecar binary resolution (`src-tauri/src/ffmpeg/binaries.rs`, adapted from autocut's `binaries.rs`: layered path search, `#[cfg]` target-triple matrix for Windows x64 MSVC+GNU). **Binary provenance/checksum decision is explicitly NOT made here** — deferred to Phase 12 per master-prompt §59, as instructed. What Phase 3 adds beyond the deferred decision: a dev/test-only system-`PATH` fallback (never consulted in a release build) so this crate's real-ffmpeg-backed tests run against the remote Ubuntu server's apt-installed ffmpeg 4.4.2 without any bundled Windows binary existing yet.
- [x] Implement audio extraction (`src-tauri/src/audio/pcm.rs`) — PCM extraction via ffmpeg pipe, ported from autocut's `audio.rs` design (incremental i16 read to bound memory); no f64→i64 rewrite was needed here specifically (the module only ever handled raw sample counts, not durations — see `docs/upstream.md`). Verified against a real ffmpeg-synthesized 440Hz tone (sample count + peak-amplitude assertions), plus the byte-carry unit tests ported from autocut.
- [x] Implement waveform generation (`src-tauri/src/audio/waveform.rs`) — peak-per-bin downsampler, ported from autocut's `waveform.rs` design, extended with `bin_duration_us` (real-sample-rate-derived) since this project's i64-µs timebase makes that cheap to provide instead of leaving bin→time mapping to the frontend.
- [x] Implement thumbnail generation for video/image import (`src-tauri/src/media/thumbnail.rs`): single-frame ffmpeg extract for video (seek to 10%-into-clip, capped at 5s, so intro title cards don't become every thumbnail), same ffmpeg-based downscale for images (no separate `image`-crate dependency). Verified against a real ffmpeg-synthesized test clip.
- [x] Implement media import: drag & drop (`getCurrentWebview().onDragDropEvent`), multi-file + folder import (recursive, hidden-dir-skipping), file picker (`@tauri-apps/plugin-dialog`), supported formats per master-prompt §7 (MP4/MOV/MKV/AVI/WEBM/M4V, MP3/WAV/AAC/M4A/FLAC, PNG/JPG/JPEG/WEBP) — extension classification in `src-tauri/src/media/import.rs`, Tauri commands in `src-tauri/src/commands/media.rs` (`import_media_paths`/`import_media_folder`, per-file success/error so one bad file doesn't abort a batch), frontend panel `src/components/media/MediaLibrary.svelte` wired into `LeftPanel`'s Media tab (replacing the placeholder).
- [x] Implement proxy media generation (master-prompt §8): 4K→720p (`PROXY_TARGET_HEIGHT`) editing proxy, Off/Auto/Always modes (`src-tauri/src/media/proxy.rs`), real progress reporting via the `media:proxy-progress` Tauri event (`commands::media::generate_media_proxy`/`spawn_proxy_job`) — not mocked: emits live `-progress`-derived fractions during a real ffmpeg encode, verified end-to-end against a real synthetic source in `media::proxy::tests` (progress + a real 720p output file), plus a cancellation test confirming partial output is deleted.
- [x] Implement video preview panel: play/pause/stop/seek/frame-step forward/back/speed control (0.25x–2x)/current-time/duration/volume/mute/fullscreen/canvas ratios 16:9/9:16/1:1/4:5/custom (master-prompt §9) — `src/components/preview/VideoPlayer.svelte`, a real `<video>` element served via Tauri's `asset:` protocol, wired into `CenterPreview.svelte`. **UNVERIFIED visually** — this is a headless Linux build server with no display/GPU; only `svelte-check`/`tsc`/`vite build` passing is confirmed, not actual playback rendering. "Preview follows timeline edits" (this section's closing line) is explicitly deferred to Phase 4, which is when a timeline/playhead concept first exists to follow — Phase 3's preview follows the Media Library selection instead.
- [x] Implement local media library indexing (SQLite, `src-tauri/src/db/mod.rs`, `rusqlite` with the `bundled` feature so Windows packaging never needs a separate SQLite DLL) per master-prompt §35: filename/path/duration/resolution/tags/created/type, kept in its own `media_library.sqlite3` file separate from `project.json` as that section instructs. 8 unit tests (round-trip, re-import-updates-not-duplicates, filename/tag search, kind filter, limit, removal, proxy-path update). AI-generated tags are explicitly NOT implemented (no tagging pipeline exists yet) — `tags` is a real, queryable column with no writer yet.
- [x] Windows path handling tests: spaces, Unicode, Vietnamese filenames (master-prompt §88's own three example paths, including `phỏng vấn 01.mp4`) tested against both the ffmpeg command-builder (`ffmpeg::command::tests`) and media-import extension classification (`media::import::tests`), confirming arguments travel as argument-array elements, never shell-concatenated strings. Real Windows path *semantics* (drive letters, backslashes, UNC) are NOT exercised — this is a Linux build server; that remains a follow-up for a real Windows machine, same caveat as Phase 2.
- [x] Compile, run tests, fix errors, update docs, commit. Verified on the remote Ubuntu server: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (0 warnings), `cargo test --lib` (**63/63 passing**, including real ffmpeg/ffprobe subprocess calls — ffmpeg 4.4.2 apt-installed for this purpose), `cargo check --target x86_64-pc-windows-gnu --lib` (type-checks clean, including the `rusqlite` `bundled` C build cross-compiling with the mingw-w64 toolchain from Phase 2), `cargo tauri build --no-bundle` (release binary produced), `pnpm run lint`/`pnpm run check` (0 errors)/`pnpm run build` (all clean, including the new `bindings.ts` regenerated via `cargo run --bin export_bindings`).

---

## PHASE 4 — Timeline

- [ ] Implement the general multi-clip, multi-track timeline engine (`src-tauri/src/timeline/`) per the Phase 1 data model design — NOT autocut's single-shared-CutList model (audit §4/§5)
- [ ] Implement track types: Video, Audio, Caption, Image, Overlay, Effect (master-prompt §10)
- [ ] Implement clip operations: drag, resize, trim start/end, split, delete, duplicate, move, snap
- [ ] Implement timeline UI features: zoom, horizontal scroll, multi-select, lock track, hide video track, mute/solo audio track, playhead, selection region, ruler, waveform display, thumbnail strip, markers
- [ ] Implement command-based undo/redo (`SplitClipCommand`, `MoveClipCommand`, `TrimClipCommand`, `DeleteClipCommand`, `AddCaptionCommand`, etc. per master-prompt §11) with bounded history — not whole-project-copy undo
- [ ] Implement copy/paste and keyboard shortcuts (master-prompt §49)
- [ ] Implement `SyncGroup`/`ClipGroup` behavior designed in Phase 1 (generalized version of autocut's fixed-rig linked-track offset concept — audit §4)
- [ ] Frontend: `src/components/timeline/` (Timeline, TrackHeader, ClipView, Ruler, Waveform, Markers), `src/timeline/` (non-reactive cut/snap algebra, pattern informed by autocut's `cuts.ts` design — reimplemented, not copied, per audit §7 license gate)
- [ ] Frontend: Svelte 5 runes-based timeline store (pattern informed by autocut's `store.svelte.ts` session-guard design — audit §2)
- [ ] Performance: virtualize large timelines, debounce expensive UI updates (master-prompt §50)
- [ ] Rust unit tests: timeline operations, undo/redo, project serialization round-trip
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 5 — AutoCut integration

- [ ] **Gate check**: confirm licensing status of `vendor/autocut` per Phase 0's flagged human decision — proceed as reimplementation-from-design unless a license was obtained (audit §7)
- [ ] Implement `VadProvider` trait (master-prompt §13) with a Silero-VAD implementation using the same `voice_activity_detector` crate autocut depends on (independently licensed, reusable regardless of autocut's own license status — audit §3)
- [ ] Reimplement two-phase VAD scoring/segmentation design (score once, cache; re-segment cheaply on parameter change) — informed by autocut's `vad.rs` architecture, rewritten to i64-µs timebase (audit §5)
- [ ] Reimplement silence-detection cut-list generation — informed by autocut's `cutlist.rs` "always tiles the timeline" invariant, rewritten as an `EditPlan`/timeline-operation producer feeding the Phase 4 timeline engine (not autocut's standalone single-CutList model — audit §8)
- [ ] Implement Silence Detector UI: threshold, min silence/speech duration, padding before/after, merge nearby speech, channel selection, analysis track selection (master-prompt §12)
- [ ] Implement Analyze / Preview Cuts / Apply Cuts / Reset workflow — non-destructive, generates timeline edits only
- [ ] Implement multi-track sync: linked tracks aligned by embedded timecode or manual offset (concept from autocut's `open_track`/`export_fcpxml.rs` offset-intersection design — audit §2/§4 — rebuilt on top of the general `SyncGroup` model from Phase 4, not autocut's fixed single-cutlist model)
- [ ] Rust unit tests: VAD hysteresis/threshold re-segmentation, cut-list tiling, multi-track offset math (informed by autocut's test suite structure — 15 VAD tests, 7 cutlist tests — but original test code)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 6 — Rendering

- [ ] Implement `RenderGraph` construction from the Project timeline (master-prompt §69)
- [ ] Implement local FFmpeg render engine (`src-tauri/src/render/`): reimplemented from autocut's concat-demuxer cutting technique (audit §2/§3) but extended to a full compositing pipeline (multi-track, effects, captions) that autocut's `export_mp4.rs` does not support (audit §4 — autocut export is single-source-only)
- [ ] Implement export presets: Fast Preview, 1080p, 1440p, 4K, TikTok 1080×1920, YouTube 1080p, YouTube 4K (master-prompt §32)
- [ ] Implement codec/container support: MP4 H.264, MP4 H.265, WebM
- [ ] Implement hardware-acceleration detection (NVENC/Quick Sync/AMD) with libx264/libx265 fallback (master-prompt §33) — a capability autocut's `export_mp4.rs` explicitly lacks (audit §2/§6)
- [ ] Implement render job cancellation with clean child-process termination (master-prompt §44/§45), pattern informed by autocut's `AtomicBool`-polling cancellation design (audit §6 risk #10 notes this pattern is worth keeping for leaf operations)
- [ ] Implement FCPXML export (`src-tauri/src/fcpxml/`): reimplemented from autocut's `export_fcpxml.rs`/`timecode.rs` rational-timecode and lane/connected-clip design (audit §2/§8), generalized to the multi-clip timeline rather than the single-cutlist model
- [ ] Rust unit tests: FFmpeg argument generation, FCPXML rational-timecode math (drop-frame, degenerate-fps guard — informed by autocut's regression-tested edge cases), concat-list path escaping (Windows backslash paths)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 7 — Transcription

- [ ] Implement `TranscriptionProvider` trait (master-prompt §14)
- [ ] Evaluate and integrate Whisper/whisper.cpp/faster-whisper for Windows packaging (GPU/CUDA preferred, CPU fallback)
- [ ] Implement Model Manager: installed/available models, download/delete, size, language support, storage location, resumable download with `.part` + atomic rename (master-prompt §60)
- [ ] Implement word-level timestamp support, schema `{text, start, end, confidence}` (master-prompt §14)
- [ ] Implement transcript-based editing UI: synchronized transcript + timeline, click-word-to-seek, select-sentence-to-select-range, Transcript Text Edit vs. Video Edit Through Transcript modes (master-prompt §15)
- [ ] Implement filler-word detection (English + Vietnamese defaults, custom dictionary support), candidate preview/select/apply workflow with configurable padding (master-prompt §16)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 8 — Captions

- [ ] Implement caption generation from transcript: word-level timing, sentence captions, max words/chars per line, line wrapping (master-prompt §26)
- [ ] Implement caption styling: font, size, bold/italic, alignment, position, background, outline, shadow, opacity, safe margins
- [ ] Implement caption templates: Minimal, TikTok, Podcast, News, Gaming, Karaoke
- [ ] Implement active-word (karaoke-style) caption rendering — efficient rendering model, not one UI object per word (master-prompt §27)
- [ ] Implement caption correction tools: split/merge/retime/drag boundaries/find-replace/bulk style, without forcing retranscription (master-prompt §28)
- [ ] Reference capcut-mate's caption/text-effect resolution logic (`src/service/add_captions.py`, `text_effect_map_generated.py` — audit §1/§3) when mapping caption styles to CapCut-exportable text styles, since this is the CapCut adapter's most complex mapping surface
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 9 — CapCut adapter

- [ ] Port `src/pyJianYingDraft/` class model to Rust (`src-tauri/src/capcut/`): `ScriptFile`, `Track`/`TrackType`, `BaseSegment`/`MediaSegment`/`VisualSegment` hierarchy, `VideoSegment`/`AudioSegment`/`TextSegment`/`StickerSegment`/`EffectSegment`/`FilterSegment`, `ClipSettings`, `VideoMaterial`/`AudioMaterial`, `Animation`/`Keyframe`/`Mask` — preserving the i64-µs timebase and the "materials collected into script bucket only on add_segment" pattern (audit §1/§3/§8)
- [ ] Implement `CapCutAdapter` internal function surface: `create_draft`, `add_video`, `add_audio`, `add_image`, `add_caption`, `add_sticker`, `add_effect`, `add_mask`, `add_animation`, `add_keyframe`, `save_draft`, `export_draft` (master-prompt §29) — the app core calls these functions directly, never HTTP
- [ ] Implement `CapCutExportGraph`: Project → CapCutExportGraph → CapCutAdapter → Draft (master-prompt §70), keeping CapCut-specific IDs/structures out of core timeline code
- [ ] Port draft-format templates/schema reference from `template/default2/`, `assets/draft_content_template.json`
- [ ] **Validate draft compatibility against a real installed CapCut build** (not just Jianying Pro China, which is all capcut-mate's authors evidently tested against — audit §5/§6 risk #5): round-trip a generated draft through actual CapCut and confirm it opens correctly
- [ ] Implement CapCut/Jianying installation detection on Windows: port `draftPathDetect.js`'s heuristic to Rust (folder-name suffixes, `root_meta_info.json`/`.recycle_bin` markers, enumerate `C:\Users\*` profiles), **extended to also probe the international CapCut folder name** (a gap capcut-mate's own Windows path list had — audit §3/§8), plus optional registry lookups
- [ ] Implement CapCut settings UI: detected version/path/draft directory, manual override, never overwrite user drafts without confirmation (master-prompt §30)
- [ ] Implement "Export to CapCut" UI: Create New Draft (default) / Update Existing Draft, with feature-compatibility warnings when an edit can't map to CapCut (master-prompt §31)
- [ ] Create `docs/feature-matrix.md` (master-prompt §71): Feature × Internal/FFmpeg/CapCut/FCPXML support matrix
- [ ] Decide scope of optional RPA-driven automated rendering via installed CapCut (audit §8 integration strategy point 5): if pursued, build fresh against real CapCut using `windows-rs` UI Automation, clearly labeled experimental/optional, never the default render path
- [ ] Rust tests: draft JSON structure validation against known-good fixtures (informed by capcut-mate's caption/keyframe/mask test cases — audit §1), keyframe absolute-µs↔relative-fraction conversion, mask size-ratio computation
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 10 — AI edit-plan architecture

- [ ] Implement `AIProvider` trait with adapters: OpenAI-compatible, Anthropic, Google Gemini, Ollama, custom endpoint (master-prompt §17)
- [ ] Implement secure credential storage via Windows Credential Manager (never plaintext in `project.json`)
- [ ] Implement AI settings UI: provider, base URL, API key, model, temperature, timeout, connection test
- [ ] Implement `EditPlan` JSON schema + strict validation (reject malformed output) — AI never mutates the timeline directly (master-prompt §18/§82)
- [ ] Implement pipeline: AI → JSON Schema validation → Edit Plan Preview → User Approves → Timeline Engine
- [ ] Implement Smart Edit / AI semantic editing: repetition, false starts, off-topic sections, weak sentences, long pauses, filler words, duplicate ideas — each recommendation with time range, transcript, reason, confidence, suggested action (master-prompt §19)
- [ ] Implement natural-language AI command box (master-prompt §20): NL → AI Provider → EditPlan → Schema validation → Preview → Apply; never let LLM output execute shell commands or touch the filesystem directly (master-prompt §82)
- [ ] Implement highlight detection: transcript/speech-density/audio-energy/scene-change signals → start/end/score/title/reason (master-prompt §21)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 11 — Short generator

- [ ] Implement Long Video → Shorts pipeline: transcription → highlight detection → candidate ranking → clip extraction → reframe → captions → optional zoom → export (master-prompt §22)
- [ ] Implement duration/aspect/count settings (15/30/60/90s/custom; 9:16/1:1/4:5; 1/3/5/10 clips)
- [ ] Implement `SubjectTracker` provider abstraction for auto-reframe (face/person detection, motion tracking, active-speaker position), smoothed/interpolated keyframes to prevent camera jumping (master-prompt §23)
- [ ] Implement auto-zoom: Off/Low/Medium/High, keyframe-based, triggered by important sentence/emphasis/long static scene/manual markers (master-prompt §24)
- [ ] Implement scene detection: `Scene {start, end, thumbnail, score}`, timeline markers, split/select/remove/generate-highlights-from-scenes (master-prompt §25)
- [ ] Implement B-roll architecture: local media library + user folders as sources, AI keyword/start/end/duration suggestions, provider interface for future external sources — no automatic downloading from arbitrary websites (master-prompt §34)
- [ ] Implement templates system: Talking Head, Podcast, TikTok, YouTube Shorts, News, Tutorial, Gaming, Football Highlight (generic, no proprietary assets); Save/Import/Export template (master-prompt §36/§37)
- [ ] Implement audio features: volume, mute, fade in/out, normalize, noise reduction architecture, auto-ducking with level/attack/release params (master-prompt §38)
- [ ] Implement batch processing: Jobs UI (Name/Status/Progress/Stage/Elapsed/ETA/Output), states (Queued→Analyzing→Transcribing→Editing→Rendering→Completed/Failed/Cancelled), pause/resume/cancel/retry (master-prompt §42/§43)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 12 — Windows packaging

- [ ] Configure Tauri MSI and/or NSIS installer output (`AI-Video-Editor-Setup-x64.exe`), desktop shortcut, start menu entry, uninstaller, icon, version info, publisher metadata placeholder, upgrade support preserving user data (master-prompt §57)
- [ ] Implement First-Run wizard: Welcome → System Check → FFmpeg → GPU Detection → CapCut Detection → AI Provider (optional) → Transcription Model (optional) → Project Folder → Ready (master-prompt §58)
- [ ] Finalize FFmpeg/FFprobe binary packaging with documented source/version/license (`ffmpeg_version()`/`ffprobe_version()` diagnostics), replacing the placeholder decision deferred in Phase 3 (audit §6 risk #7)
- [ ] Implement Settings → System Information panel + "Copy System Information" (master-prompt §78)
- [ ] Implement auto-update architecture (Tauri updater), with Automatically Check / Notify Only / Disabled options, never updating mid-render (master-prompt §62)
- [ ] Implement crash handling: structured logs to `%LOCALAPPDATA%\AI Video Editor\logs`, recovery project flow, "Open Logs Folder" (master-prompt §54/§86)
- [ ] Finalize `THIRD_PARTY_NOTICES.md`: Apache-2.0 (pyJianYingDraft/capcut-mate), MIT (desktop-client), FFmpeg license terms, autocut status/resolution, VAD model license
- [ ] Rewrite `README.md` per master-prompt §79 (single coherent product, not two repos pasted together)
- [ ] Compile, run tests, fix errors, update docs, commit

---

## PHASE 13 — Testing / performance / security

- [ ] Full Rust test suite: timeline, silence detection, project serialization, migration, FFmpeg argument generation, CapCut mapping (master-prompt §63)
- [ ] Frontend component/store/timeline-operation tests
- [ ] Integration test: import → analyze → cut → save → reload → render → transcribe → caption → export CapCut draft (master-prompt §64), validating the exact Minimum Acceptance Test sequence in master-prompt §89 (23 steps: install → launch → create project → import → preview → waveform → analyze silence → preview cuts → apply cuts → manual split → undo → redo → save → close → reopen → verify identical timeline → render → verify playback → transcript → captions → CapCut export → clean process exit → clean uninstall)
- [ ] Windows path edge-case tests: spaces, Unicode, Vietnamese filenames, long paths, UNC paths (master-prompt §88)
- [ ] Performance validation: 2+ hour video, 4K video, thousands of transcript words/captions, large timelines remain responsive; bounded concurrency (no 20 simultaneous FFmpeg processes) (master-prompt §50/§85)
- [ ] Security review: no arbitrary shell execution from AI output, path traversal prevention, model-hash validation, API key secure storage, sidecar message validation, localhost binding to 127.0.0.1 only if any local service remains (master-prompt §53/§82)
- [ ] Verify no orphan `ffmpeg.exe`/sidecar processes remain after app exit or cancellation (master-prompt §45)
- [ ] CI/CD finalization: PR checks (lint, typecheck, fmt, clippy, tests, build) + tag-triggered Windows build/installer/checksums/release artifacts (master-prompt §65)
- [ ] Final license/attribution audit pass across the whole shipped tree
- [ ] Compile, run tests, fix errors, update docs, commit
