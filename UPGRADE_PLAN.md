# Upgrade Plan — Auto Video Editor / AI Video Automation

Live, per-phase checklist for the upgrade spec in `Prompt nâng cấp Auto Video Editor tích hợp CapCut + AI.md` (34 numbered sections). Tracked separately from `IMPLEMENTATION_PLAN.md`, which covers the *original* master-prompt phases (0–13) and is now 100% complete — do not merge these two files or re-litigate anything already decided there.

## Audit summary (done before any code, per the upgrade spec's own §30 instruction)

Full audit given to the user directly in conversation on 2026-09-06 — not duplicated here verbatim, summarized:

- **CapCut integration**: real, already built (Phase 9 of the original plan) — but architecturally **direct draft-file writing** (`Project → CapCutExportGraph → CapCutAdapter → real draft JSON on disk`), **not GUI/RPA automation**. The upgrade spec's own §12/§15/§16 (CapCut Automation Layer, Worker/Resource Management, Failure Recovery from "CapCut crash"/"CapCut không mở") all assume a GUI-automation model. **User's explicit decision (2026-09-06): keep the direct draft-file approach, do NOT build RPA/GUI automation.** This makes §15's CapCut Worker Pool and most of §16's CapCut-process-crash recovery scenarios inapplicable by construction — there is no live CapCut process to crash or lose track of. Any future decision to revisit RPA is a separate, explicit product decision (same open item already tracked in `IMPLEMENTATION_PLAN.md`'s Phase 9 section) — not something this upgrade silently reopens.
- **AI integration**: real, already built (Phase 10) — provider-agnostic (`ai::provider`), `EditPlan` closed-schema pipeline, Smart Edit, NL command box, AI-assisted highlight detection, B-roll suggestion, media tagging. No streaming, no automatic retry, no token/cost tracking yet.
- **Job/queue/worker**: real, already built (Phase 11) — `batch::manager`/`batch::pipeline`, one worker thread per batch, sequential, real pause/resume/cancel/retry.
- **Templates**: real, already built (Phase 11) — `templates::mod::Template`, 8 built-ins + custom, save/import/export. No versioning, no intro/outro/watermark/background-music fields, no asset-by-id references yet.
- **Database**: `db::` (SQLite media index, separate from `project.json`), `templates::io` (JSON-file-per-template). No history/job-log table yet.

**User's confirmed direction (2026-09-06)**: implement phase-by-phase per the upgrade spec's own suggested breakdown, verifying and reporting back after each phase before starting the next. The phases below are *adapted* from the spec's own §31 breakdown to reflect what's already real vs. genuinely new — not a verbatim copy, since large parts of the spec's own "Phase 1/2" already exist.

---

## Phase U1 — Asset Library, Template enhancements, Multi-template batch

- [ ] Asset Library (§17): a real catalog for intro/outro/logo/watermark/music/sound-effect/overlay/font/subtitle-style/transition-preset/background, each referenced by a stable id (not a hardcoded path). CRUD (add/list/remove), local-file-backed (mirrors `templates::io`'s existing storage-location convention).
- [ ] Template schema enhancements (§3/§20): add `intro`/`outro`/`watermark`/`background_music` fields (each an `Option<AssetReference>` — asset id + a few per-use overrides like watermark position/music volume), and template **versioning** (`version: u32`, a version history so an existing job's `template_id`+`template_version` stays pinned even if the template is later edited — §20's explicit requirement: "Không để user sửa template rồi làm thay đổi các job cũ").
- [ ] Multi-template batch (§11): given N videos and M templates, produce N×M outputs (`video01_tiktok.mp4`, `video01_youtube.mp4`, ...) — extends `batch::` to accept a list of template ids per batch rather than one, reusing the existing single-template pipeline per (video, template) pair, not a new render path.
- [ ] Compile, run tests, fix errors, update docs, commit.

## Phase U2 — AI Auto Template + AI Template Generator

- [ ] AI Auto Template (§7): given one video's real signals (duration, aspect, transcript, scene/highlight data — all already real from the original plan's Phase 10/11), ask the configured AI provider to recommend one of the catalog templates (built-in or custom), with a reason — never silently auto-applied, always a proposal the user accepts/changes.
- [ ] AI Template Generator (§8): given a natural-language prompt, ask AI to produce a structured, schema-validated Template definition (closed schema, same "AI proposes → validate → preview → save" discipline as `ai::edit_plan`/`ai::smart_edit`) — never a raw/uncontrolled template.
- [ ] Compile, run tests, fix errors, update docs, commit.

## Phase U3 — History, Preview/Dry Run, Asset Library UI

- [ ] Video Processing History (§21): persist every batch job's real record (input/output paths, template+version, AI prompt/result if any, timings, status, error, retry count) — a new local table, queryable, with a real UI (list/view/re-run/clone-settings).
- [ ] Preview / Dry Run (§18): validate + show the real would-be execution plan (which template/asset/AI decision would apply) for one video without actually rendering, before committing a large batch.
- [ ] Asset Library UI + Template versioning UI (frontend for Phase U1's backend, if not already built alongside it).
- [ ] Compile, run tests, fix errors, update docs, commit.

## Phase U4 — Smart Automation (rules / watch-folder)

- [ ] A minimal rule engine (§27): `WHEN <trigger> IF <condition> THEN <action-sequence>`, starting with a real filesystem watch-folder trigger (a new dependency — a Rust folder-watcher crate — since nothing in this codebase currently watches the filesystem for changes) plus reuse of the existing AI-analyze → template-apply → batch-export pipeline as the action sequence. Explicitly scoped small per the spec's own "không over-engineer nếu codebase hiện tại chưa cần" instruction — one trigger type (new file in folder), not a general-purpose scheduler.
- [ ] Compile, run tests, fix errors, update docs, commit.

---

## Explicitly out of scope / deferred, by the user's own decision

- **CapCut GUI/RPA automation, CapCut Worker Pool, CapCut-process crash recovery** (§12/§15/§16 as literally written) — the user chose to keep direct draft-file export instead (2026-09-06). Revisit only if the user explicitly asks later.
- **Multi-machine worker pool / distributed scheduling** (§15's "Machine"/"Worker 01/02/03" concept) — this is a single-machine desktop app; nothing in this upgrade introduces distributed workers.
- **Visual (drag-and-drop) Template Builder UI** (§4) — the *data model* (a structured step-list JSON) may fall out naturally from Phase U1/U2's work, but the drag-and-drop UI itself is a large, separate frontend undertaking not yet scheduled into a phase above; revisit after U1–U4 land if still wanted.
- **AI cost/token tracking, AI response caching by video-hash** (§22) — not yet scheduled into a phase; flagged as a real gap, not forgotten.
- **Randomization with seed** (§28) — not yet scheduled into a phase.
