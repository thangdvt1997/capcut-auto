//! `CapCutError` — this subsystem's slice of the standard
//! `{code, message, details, recoverable, suggested_action}` envelope
//! (master prompt §56), following `fcpxml::error::FcpxmlError`'s precedent.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum CapCutError {
    /// Mirrors `track.py`'s `SegmentOverlap`.
    #[error("segment [{start_us}, {end_us}) overlaps an existing segment on track '{track_name}'")]
    SegmentOverlap {
        track_name: String,
        start_us: i64,
        end_us: i64,
    },

    /// Mirrors `Track.add_segment`'s `isinstance` check in `track.py`.
    #[error("segment kind does not match the type of track '{track_name}'")]
    SegmentTrackTypeMismatch { track_name: String },

    /// Mirrors `exceptions.TrackNotFound`.
    #[error("no track named '{track_name}' exists in this draft")]
    TrackNotFound { track_name: String },

    /// Mirrors `exceptions.AmbiguousTrack`.
    #[error("multiple tracks of the requested kind exist; a track name is required")]
    AmbiguousTrack,

    /// A `project::Clip`/`Caption`/`Effect`/`Animation`/`Keyframe` refers to
    /// a `clip_id`/`media_id` this graph has no record of.
    #[error("dangling reference: {details}")]
    DanglingReference { details: String },

    /// `add_mask`/`add_animation`/`add_keyframe` given a `segment_id` no
    /// track in this draft currently holds.
    #[error("no segment with id '{segment_id}' exists in this draft")]
    SegmentNotFound { segment_id: String },

    /// `add_mask`/`add_animation`/`add_keyframe` given a `segment_id` that
    /// resolves to a segment kind that operation doesn't support (e.g.
    /// `add_mask` against a `TextSegment` — masks only exist on
    /// `VideoSegment` in CapCut's own model).
    #[error("segment '{segment_id}' is not a '{expected_kind}' segment")]
    SegmentKindMismatch {
        segment_id: String,
        expected_kind: String,
    },

    #[error("failed to write CapCut draft to {path}: {details}")]
    WriteFailed { path: String, details: String },
}

impl From<&CapCutError> for AppErrorPayload {
    fn from(err: &CapCutError) -> Self {
        let message = err.to_string();
        match err {
            CapCutError::SegmentOverlap { .. } => {
                AppErrorPayload::new("CAPCUT_SEGMENT_OVERLAP", message)
                    .recoverable(true)
                    .with_suggestion(
                        "Adjust clip/caption timing so segments on the same track don't overlap.",
                    )
            }
            CapCutError::SegmentTrackTypeMismatch { .. } => {
                AppErrorPayload::new("CAPCUT_SEGMENT_TRACK_TYPE_MISMATCH", message)
                    .recoverable(true)
            }
            CapCutError::TrackNotFound { .. } => {
                AppErrorPayload::new("CAPCUT_TRACK_NOT_FOUND", message).recoverable(true)
            }
            CapCutError::AmbiguousTrack => {
                AppErrorPayload::new("CAPCUT_AMBIGUOUS_TRACK", message).recoverable(true)
            }
            CapCutError::SegmentNotFound { .. } => {
                AppErrorPayload::new("CAPCUT_SEGMENT_NOT_FOUND", message).recoverable(true)
            }
            CapCutError::SegmentKindMismatch { .. } => {
                AppErrorPayload::new("CAPCUT_SEGMENT_KIND_MISMATCH", message).recoverable(true)
            }
            CapCutError::DanglingReference { details } => {
                AppErrorPayload::new("CAPCUT_DANGLING_REFERENCE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Save the project again to repair stale references before exporting.",
                    )
            }
            CapCutError::WriteFailed { path, details } => {
                AppErrorPayload::new("CAPCUT_WRITE_FAILED", message)
                    .with_details(format!("path={path}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check disk space and folder permissions, then retry export.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(CapCutError, &str)> = vec![
            (
                CapCutError::SegmentOverlap {
                    track_name: "t".into(),
                    start_us: 0,
                    end_us: 1,
                },
                "CAPCUT_SEGMENT_OVERLAP",
            ),
            (
                CapCutError::SegmentTrackTypeMismatch {
                    track_name: "t".into(),
                },
                "CAPCUT_SEGMENT_TRACK_TYPE_MISMATCH",
            ),
            (
                CapCutError::TrackNotFound {
                    track_name: "t".into(),
                },
                "CAPCUT_TRACK_NOT_FOUND",
            ),
            (CapCutError::AmbiguousTrack, "CAPCUT_AMBIGUOUS_TRACK"),
            (
                CapCutError::SegmentNotFound {
                    segment_id: "s".into(),
                },
                "CAPCUT_SEGMENT_NOT_FOUND",
            ),
            (
                CapCutError::SegmentKindMismatch {
                    segment_id: "s".into(),
                    expected_kind: "video".into(),
                },
                "CAPCUT_SEGMENT_KIND_MISMATCH",
            ),
            (
                CapCutError::DanglingReference {
                    details: "d".into(),
                },
                "CAPCUT_DANGLING_REFERENCE",
            ),
            (
                CapCutError::WriteFailed {
                    path: "p".into(),
                    details: "d".into(),
                },
                "CAPCUT_WRITE_FAILED",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
