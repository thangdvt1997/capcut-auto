//! `RenderGraph` construction (master prompt §69) and the local FFmpeg
//! render engine: `Project -> RenderGraph -> FFmpeg plan -> FFmpeg`.
//! Reimplemented from `vendor/autocut/src-tauri/src/export_mp4.rs`'s
//! concat-demuxer cutting technique, extended to full multi-track
//! compositing (video/image/overlay overlay stacking, audio mixing) that
//! autocut's version does not support (`docs/architecture-audit.md` §2/§4).
//!
//! Module layout:
//! - `graph` — `ProjectV1 -> RenderGraph` (inputs/cuts/z-order/mute-solo
//!   resolution; each `Caption` node's `style_id` is resolved to a real,
//!   owned `CaptionStyle`; `Effect` nodes remain represented honestly as a
//!   no-op — no effect catalog exists yet).
//! - `presets` — `RenderSettings` + the master-prompt-§32 export presets.
//! - `plan` — `RenderGraph -> FfmpegArgs` (the actual filter-graph builder).
//! - `captions` — real `drawtext=` filter generation for burned-in captions
//!   (`STUDIO_PLAN.md` Phase S4), called from `plan`.
//! - `hwaccel` — NVENC/Quick Sync/AMF capability detection (master prompt
//!   §33), real smoke-tested, not just `-encoders` string matching.
//! - `job` — runs a plan via `ffmpeg::command::run_with_progress`, mirroring
//!   `media::proxy`'s cancellation/progress pattern exactly.
//! - `error` — `RenderError`, this subsystem's `AppErrorPayload` mapping.
//!
//! **Honest scope note**: video/image/overlay-track compositing and
//! audio-track mixing are both real (N tracks, not a single-source fake) —
//! see `plan` module doc comment for the exact filter-graph technique.
//! `Caption` burn-in is now real too (`STUDIO_PLAN.md` Phase S4) — see
//! `captions`'s own module doc comment for the `drawtext`-vs-`.ass` design
//! call and honest scope notes. `Effect` nodes remain a documented no-op (no
//! effect catalog exists yet) — see `graph`/`plan`'s doc comments.

pub mod audio_filters;
pub mod captions;
pub mod error;
pub mod graph;
pub mod hwaccel;
pub mod job;
pub mod plan;
pub mod presets;

pub use audio_filters::{
    ducking_filter_chain, FfmpegNoiseReductionProvider, NoiseReductionProvider,
};
pub use captions::caption_drawtext_filter;
pub use error::RenderError;
pub use graph::{build_render_graph, RenderGraph};
pub use hwaccel::{detect_encoders, resolve_backend_for_render, DetectedEncoder, EncoderBackend};
pub use job::{run_render_job, RenderJobProgress};
pub use plan::{build_ffmpeg_plan, RenderPlan};
pub use presets::{all_presets, find_preset, RenderPreset, RenderSettings};
