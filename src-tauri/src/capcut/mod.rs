//! `CapCutAdapter`: a direct Rust port of
//! `vendor/capcut-mate/src/pyJianYingDraft/` (`ScriptFile`, `Track`,
//! segment/material/animation/keyframe hierarchy), preserving the
//! microsecond timebase and the "materials collected into the script
//! bucket only on `add_segment`" pattern (`docs/architecture-audit.md`
//! §1/§3/§8). `Project -> CapCutExportGraph -> CapCutAdapter -> Draft`
//! (master prompt §70).
//!
//! ## Module map
//!
//! - `detect`: CapCut/Jianying installation detection on Windows (a
//!   separate sub-area of this same module — see that module's own doc
//!   comment).
//! - `timerange`: `Timerange` (i64-µs `{start, duration}`), the exact
//!   `time_util.py` port.
//! - `material`: `VideoMaterial`/`AudioMaterial` (`local_materials.py`).
//! - `clip_settings`: `CapCutClipSettings` (`segment.py`'s `ClipSettings`) —
//!   named to never collide with `crate::project::ClipSettings`.
//! - `keyframe`: `Keyframe`/`KeyframeList`/`KeyframeProperty` (`keyframe.py`),
//!   plus the absolute-project-µs -> segment-relative-µs conversion.
//! - `animation`: `SegmentAnimation`/`SegmentAnimations` (`animation.py`),
//!   documented passthrough (no animation-resource catalog ported).
//! - `mask`: `Mask`/`MaskType` (`video_segment.py`'s `Mask`,
//!   `metadata/mask_meta.py`) — the one small metadata enum this phase does
//!   port in full, since it backs a required size-ratio test.
//! - `caption_style`: `TextStyle`/`TextBorder`/`TextBackground`/`TextShadow`
//!   (`text_segment.py`) plus the `project::CaptionStyle -> these` mapping.
//! - `segment`: the `BaseSegment`/`MediaSegment`/`VisualSegment` hierarchy
//!   and the six concrete segment kinds.
//! - `track`: `Track`/`TrackType` (`track.py`).
//! - `script`: `ScriptFile`/`ScriptMaterial` (`script_file.py`) — the
//!   materials-bucket invariant lives here.
//! - `error`: `CapCutError`, this subsystem's `AppErrorPayload` slice.
//! - `graph`: `CapCutExportGraph` — `ProjectV1` resolved into everything the
//!   adapter needs, mirroring `render::graph::RenderGraph`.
//! - `adapter`: `CapCutAdapter`, the exact function surface master prompt
//!   §29 lists (`create_draft`, `add_video`, ..., `export_draft`).
//! - `export`: the public pipeline functions and the
//!   `export_project_to_capcut_draft` Tauri command.
//!
//! ## Deliberate scope reductions (see each module's own doc comment for
//! the full reasoning)
//!
//! - **Not ported**: `jianying_controller.py` (RPA automation — a separate,
//!   explicitly-deferred future decision, not core adapter work) and the
//!   multi-thousand-entry static effect/filter/font/transition/scene-effect
//!   catalogs (`metadata/video_scene_effect.py`, `filter_meta.py`,
//!   `video_character_effect.py`, `font_meta.py`, `transition_meta.py`,
//!   `audio_scene_effect.py`) — this app has no effect/filter/transition/
//!   font catalog or authoring UI of its own yet, so porting thousands of
//!   name -> CapCut-resource-id mappings for features nothing here can
//!   produce would be unverifiable busywork.
//! - **Ported as honest structural passthrough** (real, working, callable
//!   Rust; just no resource-catalog resolution behind it): `add_sticker`,
//!   `add_effect` (see `segment`/`adapter` module doc comments) — these
//!   carry `project::Effect::kind`/`params` or a bare `resource_id` straight
//!   through as an unresolved reference, exactly as this phase's task brief
//!   asks for.
//! - **Ported in full** (small, real, needed by a required test):
//!   `mask::MaskType`'s 6-entry enum.
//! - **Installation detection, settings UI, export UI, the feature-matrix
//!   doc, and the RPA-scope-decision item are explicitly out of scope for
//!   this module** — owned by other phases/passes (see
//!   `IMPLEMENTATION_PLAN.md` Phase 9). `detect` is the one exception,
//!   implemented separately (see that module's own doc comment).

pub mod adapter;
pub mod animation;
pub mod caption_style;
pub mod clip_settings;
pub mod detect;
pub mod error;
pub mod export;
pub mod graph;
pub mod keyframe;
pub mod mask;
pub mod material;
pub mod meta;
pub mod script;
pub mod segment;
pub mod timerange;
pub mod track;

pub use adapter::CapCutAdapter;
pub use error::CapCutError;
pub use export::{
    build_capcut_draft, export_project_to_capcut_draft, export_project_to_capcut_draft_at,
};
pub use graph::{build_capcut_export_graph, CapCutExportGraph};
