//! `RenderGraph` construction (master prompt §69) and the local FFmpeg
//! render engine: `Project -> RenderGraph -> FFmpeg plan -> FFmpeg`.
//! Reimplemented from `vendor/autocut/src-tauri/src/export_mp4.rs`'s
//! concat-demuxer cutting technique, extended to full multi-track
//! compositing (video/image/overlay overlay stacking, audio mixing) that
//! autocut's version does not support (`docs/architecture-audit.md` §2/§4).
//!
//! Module layout:
//! - `graph` — `ProjectV1 -> RenderGraph` (inputs/cuts/z-order/mute-solo
//!   resolution; `Caption`/`Effect` nodes represented honestly as no-ops).
//! - `presets` — `RenderSettings` + the master-prompt-§32 export presets.
//! - `plan` — `RenderGraph -> FfmpegArgs` (the actual filter-graph builder).
//! - `hwaccel` — NVENC/Quick Sync/AMF capability detection (master prompt
//!   §33), real smoke-tested, not just `-encoders` string matching.
//! - `job` — runs a plan via `ffmpeg::command::run_with_progress`, mirroring
//!   `media::proxy`'s cancellation/progress pattern exactly.
//! - `error` — `RenderError`, this subsystem's `AppErrorPayload` mapping.
//!
//! **Honest scope note**: video/image/overlay-track compositing and
//! audio-track mixing are both real (N tracks, not a single-source fake) —
//! see `plan` module doc comment for the exact filter-graph technique.
//! `Caption`/`Effect` tracks/nodes are represented in the graph schema but
//! deliberately not rendered (no caption burn-in system or effect catalog
//! exists yet) — this is documented in `graph`/`plan`'s doc comments, not
//! silently dropped.

pub mod error;
pub mod graph;
pub mod hwaccel;
pub mod job;
pub mod plan;
pub mod presets;

pub use error::RenderError;
pub use graph::{build_render_graph, RenderGraph};
pub use hwaccel::{detect_encoders, resolve_backend_for_render, DetectedEncoder, EncoderBackend};
pub use job::{run_render_job, RenderJobProgress};
pub use plan::{build_ffmpeg_plan, RenderPlan};
pub use presets::{all_presets, find_preset, RenderPreset, RenderSettings};
