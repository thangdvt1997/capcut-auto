# Studio Upgrade Plan — Professional Workflow UI (`promt.md`)

Live tracking doc for the spec in `promt.md` (32 sections): a much larger redesign than either prior plan file — a full 3-tab "Video Processing Studio" UI, plus a brand-new dubbing-style pipeline (Extract Subtitle → Speech-to-Text → Translate → Rewrite Script → **Generate Voice (TTS)** → Sync Timeline → Video Processing → Render → Export) that this app has never had any part of before. Tracked separately from `IMPLEMENTATION_PLAN.md` (original master-prompt phases 0–13, 100% complete) and `UPGRADE_PLAN.md` (Auto Video Editor / AI Automation upgrade, phases U1–U4, 100% complete) — do not merge these files.

**Status: audit done for §10/§11 only (this file's first tracked slice, per explicit request); the other ~28 sections of `promt.md` are not yet audited, planned, or started.** See "Full scope, not yet planned" at the bottom for the honest size of what's left.

## Why only §10/§11 so far

`promt.md` proposes replacing this app's entire screen architecture (3 tabs, a Design System component library, a Worker/Slot manager, Job Queue redesign, subtitle-translation + TTS dubbing pipeline that doesn't exist in any form today) — a redesign at least as large as the *entire* original 13-phase master prompt this app was already built from. Committing to all of it in one pass, without the same phase-by-phase confirm-then-build rhythm every prior spec in this project has used, would be exactly the "sửa hàng loạt một cách mù quáng" (blind mass rewrite) the spec's own §29/§30 explicitly warns against. This file exists so that rhythm can start correctly: audit one section, report back, then take direction on what's next — matching this project's own established practice, not a new one invented for this file.

---

## §10/§11 Audit — CapCut Integration + Video Processing Settings

Audited by reading the real code (`src-tauri/src/capcut/`, `src-tauri/src/templates/mod.rs`, `src-tauri/src/render/`, `src-tauri/src/project/types.rs`, `src-tauri/src/zoom/`, `src-tauri/src/reframe/crop.rs`), not assumed from `promt.md`'s own list.

### §10 CapCut Integration

| Item | State |
|---|---|
| CapCut path (install location) | MISSING — only a read-only, best-effort registry hint (`CapCutRegistryHint.install_location`, `capcut/detect.rs`), not an editable/usable config field |
| Project path | MISSING — no concept distinct from Draft path |
| Draft path | **EXISTS** — `capcutStore.manualDraftRoot`/`effectiveDraftRoot` (`stores/capcut.svelte.ts`), backed by `detect_capcut_installations`, persisted to `localStorage` |
| Template project (seed draft to start new ones from) | MISSING — no such concept anywhere |
| Export path | **EXISTS** as a per-export target (`CapCutStore.targetPath`), not a separately persisted setting |
| Auto create/import video/audio/subtitle, auto align timeline, auto apply effects, auto save, auto export | **ALL MISSING** — every export today is one explicit, user-confirmed action; no automation toggle of any kind exists |
| Detect CapCut | **EXISTS** — `detect_capcut_installations`/`detect_capcut_registry_hints` |
| Open CapCut (launch the installed exe) | MISSING |
| Open Current Project (open a draft folder in CapCut) | MISSING |
| Create Draft | **EXISTS** — `export_project_to_capcut_draft` ("Create New Draft" mode) |
| Sync Draft | PARTIAL — "Update Existing Draft" mode is a full re-export/overwrite, not an incremental sync |
| Validate Draft | PARTIAL — `capcut/compat.ts`'s pre-export lint checks only 3 known adapter gaps (unresolved effects/animations/keyframe properties) against the in-app project; nothing checks an existing on-disk draft's own integrity |
| Export Project | **EXISTS** |

### §11 Video Processing Settings

| Item | State |
|---|---|
| Remove original voice / Keep background audio | MISSING — no voice/ambience source separation exists at all; only a generic whole-track mute |
| Noise reduction | **EXISTS** — `AudioClipSettings::noise_reduction`, real ffmpeg filter chain |
| Normalize audio | **EXISTS** — `AudioClipSettings::normalize`, real ffmpeg `loudnorm`/EBU R128 |
| Background music + volume | **EXISTS** at the render level (`AudioRole::Music` + `DuckingSettings`, real ducking filter). PARTIAL at the template level: `Template::background_music` validates but nothing consumes it to actually insert a music clip into a built project |
| Zoom | **EXISTS** — `zoom::ZoomIntensity`, real generated keyframes |
| Pan | MISSING — no automated pan/"Ken Burns" logic anywhere |
| Crop (manual, per-clip) | MISSING — `ClipSettings` has no crop-region field (a *different*, automatic reframe-crop exists for Shorts only, not exposed as a manual tool) |
| Aspect Ratio / Resolution / FPS | **ALL EXIST** — `CanvasRatioPreset`, `RenderSettings::width/height/fps` |
| Subtitle burn-in (into rendered/exported video) | MISSING — `render::graph` documents this explicitly as a no-op today; captions only render inside CapCut itself or the app's own live preview overlay |
| Intro/Outro | PARTIAL — `Template::intro`/`outro` fields validate against the real Asset Library, but nothing consumes them to splice a clip onto a built project's timeline |
| Watermark | PARTIAL — `Template::watermark` field validates, but no overlay-rendering engine exists to actually composite it |
| Preset: TikTok 9:16 | **EXISTS** |
| Preset: YouTube Shorts 9:16 | PARTIAL — the template exists but reuses the plain TikTok render preset, no dedicated one |
| Preset: YouTube 16:9 | **EXISTS** (render preset only, no dedicated template) |
| Preset: Facebook Reel | MISSING — only appears as an illustrative string in test/example code, never a real preset |
| Preset: Original (pass-through source res/fps) | MISSING |

### Also confirmed missing entirely (relevant to the wider `promt.md` pipeline, not just §10/§11)
- **Text-to-Speech / voice generation**: zero code anywhere. §9's whole "Voice/TTS Configuration" section has no starting point in this codebase at all.
- **Subtitle/caption translation to another language**: zero code. Existing "translate" hits are all unrelated (NL-instruction-to-edit-plan, inert CapCut passthrough metadata, Whisper's translate-to-English flag hardcoded off).

### Summary
Roughly a third of §10/§11's ~30 items are real and working today, mostly because the existing Template/render-preset/CapCut-export systems already had a real mechanism to reuse. A comparable third exist only as unconsumed structural fields (`Template::intro`/`outro`/`watermark`/`background_music`, "Sync Draft", "Validate Draft") — a schema field or loosely-analogous action exists, but nothing wires it into a real pipeline. The last third is genuinely new: launching/controlling CapCut as an external process, a real draft-integrity validator, manual crop, automated pan, subtitle burn-in into rendered video, the missing presets, and voice/background-audio separation.

---

## Phase S1 — CapCut process control + real draft validation

The best-scoped, highest-value slice of §10's gaps — builds directly on this project's own just-completed real-CapCut-Pro validation work (`IMPLEMENTATION_PLAN.md` Phase 9), reusing the exact same real, now-understood `draft_content.json`/`draft_meta_info.json`/`root_meta_info.json` schema rather than guessing at a new one.

- [ ] **Open CapCut**: a new Tauri command that launches the detected CapCut/Jianying executable (real process spawn, not a shell-injection-prone string command — validate the resolved exe path is one of `detect_capcut_installations`'s own results or a user-confirmed override before spawning).
- [ ] **Open Current Project**: launches CapCut the same way, but pointed at a specific draft folder (CapCut's own real deeplink/CLI-argument convention needs to be confirmed — the `draft_deeplink_url` field observed in a real `draft_meta_info.json` during Phase 9's validation is a real, promising lead to check first, rather than guessing a `--project` flag).
- [ ] **Validate Draft**: a real, honest integrity check against an *existing* draft folder — confirms `draft_content.json`/`draft_info.json`/`draft_meta_info.json` all exist and parse as valid JSON, confirms the draft's own entry exists in `root_meta_info.json`'s `all_draft_store` (the exact real gap Phase 9 found and fixed for *new* exports — this check catches the same problem for a draft that predates the fix, or one edited by hand), and confirms every media file the draft's own materials reference still exists on disk (a moved/deleted source file is a real, common failure mode). Reports a structured list of problems, never silently "OK" on a draft it didn't actually check.
- [ ] Compile, run tests, fix errors, update this file, commit.

## Phase S2 — Wire template asset references into the real build pipeline

Closes the "PARTIAL" gaps found above: `Template::intro`/`outro`/`watermark`/`background_music` currently validate but do nothing.

- [ ] A real "apply template to project" step (likely inside `batch::pipeline` and/or a new `commands::templates` command) that, given a template with `intro`/`outro` set, actually splices the referenced asset's clip onto the built project's timeline before/after the main content; given `background_music` set, inserts a real music clip/track using the asset's file and volume; given `watermark` set, honestly either (a) builds a real overlay-compositing step in `render::plan`, or (b) is scoped down to CapCut-export-only (CapCut itself can composite an overlay track natively, unlike this app's own ffmpeg render path) — this decision needs to be made explicitly, not silently assumed, since it changes which output paths (Internal render vs. CapCut export) actually honor a template's watermark.
- [ ] Compile, run tests, fix errors, update this file, commit.

## Phase S3 — Missing presets + manual crop

- [ ] Add the missing render presets/templates: dedicated YouTube Shorts preset (distinct from TikTok's), Facebook Reel, "Original" (pass-through source resolution/fps/aspect — needs a real design decision on how a "no fixed resolution" preset fits `RenderSettings`'s existing shape, since every other preset today is fixed-dimension).
- [ ] Add a manual per-clip crop-region field to `ClipSettings` + real ffmpeg crop-filter support in `render::plan` (distinct from the existing Shorts auto-reframe crop, which stays as-is).
- [ ] Compile, run tests, fix errors, update this file, commit.

## Phase S4 — Subtitle burn-in into rendered video

- [ ] Close the documented `render::graph` no-op: real caption-to-`drawtext`/`subtitles` ffmpeg filter generation so captions actually appear in Internal-rendered/exported output, not just inside CapCut or the app's own live preview. A substantial, self-contained piece of real render-pipeline work — likely its own phase given the size.

## Phase S5 — Automated pan ("Ken Burns")

- [ ] A real automated pan-keyframe generator, parallel to the existing `zoom::` module's own zoom-keyframe generation — likely lives alongside it once designed.

---

## Explicitly flagged, NOT scoped into any phase above (real, honest gaps — not silently dropped)

- **Remove original voice / Keep background audio** (voice/ambience source separation) — this is a genuinely hard, different kind of feature (audio source separation, likely needing an ML model such as Demucs/Spleeter-equivalent, not a simple ffmpeg filter) — flagged as needing its own dedicated research/scoping pass, not bundled into S1–S5's straightforward pipeline-wiring work.
- **Text-to-Speech / voice generation (`promt.md` §9)** — an entirely new subsystem (provider abstraction, voice mapping, a new pipeline stage) with zero existing scaffolding anywhere in this codebase. A large, separate initiative on the scale of adding a whole new Phase-9-sized feature area.
- **Subtitle/caption translation (`promt.md` §8's Translation/Script settings)** — likewise a new subsystem (source/target language config, genre/style prompts, a translation AI call distinct from the existing edit-plan/smart-edit AI features).
- **The full 3-tab redesign, Design System component library, Worker/Slot manager UI, Job Queue table redesign, Project/Preset management tab, dashboard header/status bar, activity log panel, job-state resume** (`promt.md` §2, §4–§9 UI portions, §12–§22) — the bulk of the spec's own size; not audited yet at all.

Do not treat the absence of a phase for any of the above as an oversight — they are real, sized, and simply not yet scheduled. Next step is the user's own direction on ordering, same as every prior spec in this project.
