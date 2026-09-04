# Upstream provenance

Tracks which code in this repository originates from which upstream project, per master prompt §72. Update this table whenever a module is ported or reimplemented from `vendor/capcut-mate` or `vendor/autocut`.

## Licensing status

| Upstream | License | Status |
|---|---|---|
| `vendor/capcut-mate` (main tree) | Apache-2.0 (© 2024 Gary Guan / pyJianYingDraft, © 2026 Hommy modifications) | Reusable now with attribution + documented modifications (Apache-2.0 §4). |
| `vendor/capcut-mate/desktop-client` | MIT (© 2025 gogoshine) | Reusable now, tracked separately from the Apache-2.0 notice above — different author/work. |
| `vendor/autocut` | No LICENSE file / no grant in the repo itself (`"license": null` on GitHub, confirmed 2026-09-04, see `docs/architecture-audit.md` §7) | **Permission obtained directly from the author, Mert Cobanov, per the project owner (2026-09-04).** Code may be ported/copied under that grant. This project keeps no independent written record of the grant beyond this note — if that becomes a concern, get it in writing (email/issue) and link it here. |

## Module provenance

| New module | Origin | Notes |
|---|---|---|
| `src-tauri/src/capcut/` | Ported from `vendor/capcut-mate/src/pyJianYingDraft/` (`script_file.py`, `track.py`, `segment.py`, `video_segment.py`, `audio_segment.py`, `text_segment.py`, `effect_segment.py`, `animation.py`, `keyframe.py`, `local_materials.py`, `time_util.py`) | Direct structural port to Rust, µs timebase preserved as-is. Apache-2.0 attribution required: keep `NOTICE` text (Gary Guan 2024, Hommy 2026 modifications) in `THIRD_PARTY_NOTICES.md` and document changes made during the port. |
| `src-tauri/src/media/probe.rs` | Reimplemented from `vendor/autocut/src-tauri/src/probe.rs` | Direct ffprobe JSON parsing approach (not capcut-mate's `pymediainfo`), per audit §4. Permission granted for direct reuse — port literally where practical, rewritten to i64-µs where autocut used f64 seconds. |
| `src-tauri/src/audio/` | Reimplemented from `vendor/autocut/src-tauri/src/audio.rs`, `waveform.rs` | Incremental i16 PCM extraction pattern; waveform peak-per-bin. |
| `src-tauri/src/vad/` | Reimplemented from `vendor/autocut/src-tauri/src/vad.rs` | Two-phase score/segment design, `voice_activity_detector` crate (Silero V5) reused as a direct dependency (independently licensed on crates.io regardless of autocut's own status). Rewritten f64s → i64us. |
| `src-tauri/src/render/export_mp4` (Phase 6) | Reimplemented from `vendor/autocut/src-tauri/src/export_mp4.rs` | Concat-demuxer + inpoint/outpoint technique, extended to multi-track compositing (autocut's version is single-source only). |
| `src-tauri/src/fcpxml/` (Phase 6) | Reimplemented from `vendor/autocut/src-tauri/src/export_fcpxml.rs`, `timecode.rs` | Rational-timecode math, lane/connected-clip structure, generalized off the new multi-clip timeline instead of autocut's single-CutList model. |
| `src-tauri/src/timeline/` | New | Neither upstream repo's model generalizes (audit §4/§5) — general clips-on-tracks model, ID-referenced, non-destructive. |
| `src-tauri/src/project/` | New | `ProjectV1` schema per `docs/project-format.md`, informed by capcut-mate's `Timerange`/segment-on-track structure but redesigned around desktop app lifecycle (not FastAPI singleton state, audit §6 risk #4). |
| CapCut/Jianying path detection (Phase 9) | Reimplemented from `vendor/capcut-mate/desktop-client/nodeapi/draftPathDetect.js` (MIT) | Heuristic ported natively to Rust; extended to probe international CapCut's folder name too (a gap in the original — audit §3/§8). |
| `src/components/layout/ResizableSplit.svelte` (Phase 2) | Ported verbatim from `vendor/autocut/src/components/ResizableSplit.svelte` | Logic unchanged (localStorage-persisted two-pane splitter); only the doc comment was added. Direct reuse permitted per the licensing status table above. |
| `src-tauri/src/main.rs`, `src-tauri/src/lib.rs` app-assembly pattern (Phase 2) | Adapted from `vendor/autocut/src-tauri/src/main.rs`/`lib.rs` | `windows_subsystem = "windows"` release console suppression; `#[cfg(not(test))]` on `run()` so `generate_context!()` (which requires `frontendDist` to exist) doesn't block `cargo test`. Extended with a `specta_builder()`/`export_bindings()` split not present in autocut (autocut hand-mirrors `types.ts` instead — audit §6 risk #10). |
| `.gitignore`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json`, `.github/workflows/ci.yml` shape (Phase 2) | Adapted from autocut's equivalents | Config-pattern reuse per the project owner's permission (Cargo dependency choices, config patterns, `.gitignore`, CI workflow structure). CI diverges deliberately: Rust/Tauri jobs run on `windows-latest` on every PR (autocut's ran on `ubuntu-latest` with sidecars stubbed, Windows only via manual `workflow_dispatch` — audit §6 risk #9). |
| `src-tauri/icons/*` (Phase 2) | Copied from `vendor/autocut/src-tauri/icons/` | Stock Tauri-CLI-generated default icon set (not autocut-original artwork) used as a placeholder; real app branding is a follow-up. |

Rows are added as each phase actually ports/reimplements the corresponding code — this table starts empty of "done" claims beyond what's written above and grows alongside `IMPLEMENTATION_PLAN.md`.
