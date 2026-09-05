//! Auto-reframe: landscape -> portrait (or any other target aspect ratio)
//! conversion via subject tracking (master prompt §23, `IMPLEMENTATION_PLAN.md`
//! Phase 11's `SubjectTracker` bullet). "Do NOT simply center crop" —
//! instead: track a real subject-position signal over time (`provider`,
//! `motion`), smooth it to prevent camera jumping (`smoothing`), and turn
//! the smoothed track into a real crop-region-over-time result (`crop`).
//!
//! Module layout:
//! - `provider` — the technique-independent `SubjectTracker` trait +
//!   `SubjectPosition`, same design shape as `vad`/`transcription`/`ai`'s own
//!   `*Provider` traits.
//! - `motion` — `MotionTrackingSubjectTracker`, the one real, working
//!   implementation this pass builds (ffmpeg frame sampling + frame-
//!   difference motion detection). See `provider` module doc comment for
//!   the honest scope note on face/person detection and active-speaker
//!   position, both architecturally supported but not implemented here.
//! - `smoothing` — time-based exponential smoothing over raw positions,
//!   plus conversion into real `project::Keyframe` entries.
//! - `crop` — crop-window-over-time computation for a target aspect ratio,
//!   clamped to source bounds — the actual "auto reframe" output.
//! - `error` — `ReframeError`, this subsystem's `AppErrorPayload` mapping.

pub mod crop;
pub mod error;
pub mod motion;
pub mod provider;
pub mod smoothing;

pub use crop::{
    compute_crop_window, crop_window_ffmpeg_filter, crop_windows_over_time, CropWindow,
};
pub use error::ReframeError;
pub use motion::MotionTrackingSubjectTracker;
pub use provider::{SubjectPosition, SubjectTracker};
pub use smoothing::{keyframes_from_smoothed, smooth_positions, DEFAULT_SMOOTHING_TAU_US};
