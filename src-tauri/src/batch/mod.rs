//! Batch Processing (master prompt §42/§43) — the final Phase 11 backend
//! item: process N media files through a multi-stage pipeline (transcribe ->
//! silence removal -> captions -> template settings -> render), tracked as
//! real `BatchJob`s with real state transitions, progress, and pause/resume/
//! cancel/retry — never blocking Tauri's own command-dispatch thread.
//!
//! ## Why this isn't "spawn a Tauri command per stage and wait for its job"
//!
//! Several individual stages already exist in this codebase as their own
//! async, job-id-returning, event-emitting Tauri commands
//! (`commands::transcription::transcribe_media`, `commands::render::start_render_job`).
//! A batch worker cannot cleanly block-wait on another command's own
//! background job from inside a third one without inventing fragile
//! polling — so this module does **not** call those command wrappers.
//! Instead, `batch::pipeline::run_pipeline` calls each stage's real
//! *synchronous* core directly (the plain function the async command's own
//! spawned thread calls underneath), in sequence, on its own dedicated
//! worker thread per batch (`batch::manager`'s own module doc comment covers
//! the concurrency model in detail). This is the same architecture
//! `docs/architecture.md`'s job-system design already describes, not a
//! workaround.
//!
//! ## Module layout
//!
//! - `types`: `BatchJob`/`BatchJobStatus`/`BatchPipelineConfig` — the schema
//!   exposed to the frontend.
//! - `error`: `BatchError`, this subsystem's slice of the standardized error
//!   model.
//! - `pipeline`: the real, Tauri-free per-file orchestration
//!   (`run_pipeline`) — directly unit-testable, no `AppHandle` anywhere in
//!   its signature.
//! - `manager`: `BatchJobManager` (Tauri-managed state), job registry,
//!   worker-thread spawning, and the real `AppHandle`-dependent glue
//!   (resolving ffmpeg/models/templates directories, emitting
//!   `batch:progress` events).

pub mod error;
pub mod manager;
pub mod pipeline;
pub mod types;

pub use error::BatchError;
pub use manager::{BatchJobManager, BatchProgressEvent};
pub use types::{BatchJob, BatchJobStatus, BatchPipelineConfig};
