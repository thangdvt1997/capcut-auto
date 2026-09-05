//! `SubjectTracker` trait (master prompt §23) — technique-independent, same
//! design shape as `vad::provider::VadProvider`/
//! `transcription::provider::TranscriptionProvider`/`ai::provider::AIProvider`:
//! no technique-specific type leaks into the trait signature, so a second
//! implementation (a face-detection provider, a person-detection provider,
//! an active-speaker-position provider) can be added later without touching
//! any call site.
//!
//! ## Honest scope note (per this pass's task brief)
//!
//! Master prompt §23 lists four possible tracking techniques: face
//! detection, person detection, motion tracking, active speaker position.
//! This pass implements exactly one of them for real —
//! [`crate::reframe::motion::MotionTrackingSubjectTracker`] — chosen because
//! it is the only one achievable without either bundling a real ML model
//! (this codebase's Phase 7 precedent for that: whisper.cpp, vendored and
//! compiled via `whisper-rs`'s build script) or a heavy new dependency
//! (OpenCV bindings, an ONNX runtime plus a face/person-detection model).
//!
//! Face detection and person detection are **not implemented in this
//! pass**. They are architecturally supported — this trait's signature
//! doesn't care which technique produced the positions, so a
//! `FaceDetectionSubjectTracker`/`PersonDetectionSubjectTracker` could be
//! added later as another `impl SubjectTracker` with zero changes to
//! `smoothing`/`crop`/the Tauri command layer — but no such implementation
//! exists yet. This is a real, stated gap, not a hidden one.
//!
//! Active-speaker-position tracking is also **not implemented**, for a
//! different, structural reason: it requires speaker *diarization*
//! (attributing speech to a specific one of several speakers so their
//! on-screen position can be inferred), and this codebase's transcription
//! subsystem (`crate::transcription`, Phase 7) only ever produces a single
//! undiarized text stream — there is no "which speaker is talking right
//! now" signal anywhere in this codebase to build such a provider on top of.
//! Same honest treatment: architecturally pluggable, not built.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::ReframeError;

/// One normalized subject-position sample over time (master prompt §23:
/// "Return normalized target coordinates over time").
///
/// `target_x`/`target_y` are fractions of the *source* frame, `0.0..=1.0`,
/// origin top-left (`target_x=0.0` is the frame's left edge, `target_y=0.0`
/// is its top edge) — plain image-space normalized coordinates, deliberately
/// not yet converted to `project::ClipSettings`'s half-canvas/y-up
/// convention (that conversion happens once, at the `smoothing` module
/// boundary, when these positions become `project::Keyframe`s — see that
/// module's doc comment for why).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct SubjectPosition {
    /// Microseconds from the start of the *source* media file (not an
    /// absolute project-timeline position — a tracker has no notion of
    /// "project" or "clip placement", only the raw file it was pointed at,
    /// the same "provider sees no project-level concepts" split
    /// `transcription::provider`'s own module doc comment establishes for
    /// `TranscriptSegment`).
    pub time_us: i64,
    pub target_x: f32,
    pub target_y: f32,
}

/// Subject-tracking backend for auto-reframe (master prompt §23). Kept
/// deliberately minimal and free of any technique-specific type (module doc
/// comment) — `track` is the only method every implementation must provide.
pub trait SubjectTracker: Send + Sync {
    /// Analyze `video_path` and return normalized subject-position samples
    /// over time, ascending by `time_us`. `ffmpeg`/`ffprobe` are
    /// already-resolved sidecar binary paths (the same "resolve once at the
    /// command layer, pass plain `&Path`s down" convention
    /// `commands::highlights::run_detection` uses) since every real
    /// implementation of this trait needs to shell out to at least one of
    /// them to read frames.
    fn track(
        &self,
        ffmpeg: &Path,
        ffprobe: &Path,
        video_path: &Path,
    ) -> Result<Vec<SubjectPosition>, ReframeError>;
}
