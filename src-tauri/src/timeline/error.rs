//! `TimelineError` — this subsystem's slice of the standardized error model
//! (master prompt §56, `docs/project-format.md` "Error model"), following the
//! same `{code, message, details, recoverable, suggested_action}` pattern
//! `project::error::ProjectError` (Phase 2) and `media::error::MediaError`
//! (Phase 3) already established.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum TimelineError {
    #[error("clip not found: {clip_id}")]
    ClipNotFound { clip_id: String },

    #[error("track not found: {track_id}")]
    TrackNotFound { track_id: String },

    #[error("track {track_id} is locked")]
    TrackLocked { track_id: String },

    #[error("sync group not found: {group_id}")]
    SyncGroupNotFound { group_id: String },

    #[error("invalid trim range: {details}")]
    InvalidTrimRange { details: String },

    #[error("invalid split position: {details}")]
    InvalidSplitPosition { details: String },

    #[error("invalid move: {details}")]
    InvalidMove { details: String },

    #[error("clip would overlap clip {other_clip_id} on track {track_id}")]
    ClipOverlap {
        track_id: String,
        other_clip_id: String,
    },

    #[error("nothing to undo")]
    NothingToUndo,

    #[error("nothing to redo")]
    NothingToRedo,

    #[error("no project is loaded in the timeline session")]
    NoActiveProject,

    #[error("clipboard is empty")]
    ClipboardEmpty,

    #[error("clip {clip_id} has no media reference; silence cuts can't be applied to it")]
    ClipHasNoMedia { clip_id: String },

    #[error("media not found: {media_id}")]
    MediaNotFound { media_id: String },

    #[error("invalid sync group request: {details}")]
    InvalidSyncGroup { details: String },

    #[error("clip {clip_id} is already in sync group {group_id}")]
    ClipAlreadyGrouped { clip_id: String, group_id: String },

    #[error("timecode-based sync unavailable: {details}")]
    TimecodeUnavailable { details: String },
}

impl From<&TimelineError> for AppErrorPayload {
    fn from(err: &TimelineError) -> Self {
        let message = err.to_string();
        match err {
            TimelineError::ClipNotFound { clip_id } => {
                AppErrorPayload::new("TIMELINE_CLIP_NOT_FOUND", message)
                    .with_details(clip_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The clip may have already been deleted; refresh the timeline.",
                    )
            }
            TimelineError::TrackNotFound { track_id } => {
                AppErrorPayload::new("TIMELINE_TRACK_NOT_FOUND", message)
                    .with_details(track_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The track may have already been deleted; refresh the timeline.",
                    )
            }
            TimelineError::TrackLocked { track_id } => {
                AppErrorPayload::new("TIMELINE_TRACK_LOCKED", message)
                    .with_details(track_id.clone())
                    .recoverable(true)
                    .with_suggestion("Unlock the track before editing its clips.")
            }
            TimelineError::SyncGroupNotFound { group_id } => {
                AppErrorPayload::new("TIMELINE_SYNC_GROUP_NOT_FOUND", message)
                    .with_details(group_id.clone())
                    .recoverable(true)
                    .with_suggestion("The sync group may have already been removed.")
            }
            TimelineError::InvalidTrimRange { details } => AppErrorPayload::new(
                "TIMELINE_INVALID_TRIM_RANGE",
                message,
            )
            .with_details(details.clone())
            .recoverable(true)
            .with_suggestion(
                "Choose a trim point that leaves a positive-length clip within its source media.",
            ),
            TimelineError::InvalidSplitPosition { details } => {
                AppErrorPayload::new("TIMELINE_INVALID_SPLIT_POSITION", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a split point strictly inside the clip's span.")
            }
            TimelineError::InvalidMove { details } => {
                AppErrorPayload::new("TIMELINE_INVALID_MOVE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a valid destination track and position.")
            }
            TimelineError::ClipOverlap {
                track_id,
                other_clip_id,
            } => AppErrorPayload::new("TIMELINE_CLIP_OVERLAP", message)
                .with_details(format!("track={track_id} other_clip={other_clip_id}"))
                .recoverable(true)
                .with_suggestion("Choose a position/track that doesn't overlap an existing clip."),
            TimelineError::NothingToUndo => {
                AppErrorPayload::new("TIMELINE_NOTHING_TO_UNDO", message)
                    .recoverable(true)
                    .with_suggestion("There is no earlier state to restore.")
            }
            TimelineError::NothingToRedo => {
                AppErrorPayload::new("TIMELINE_NOTHING_TO_REDO", message)
                    .recoverable(true)
                    .with_suggestion("There is no later state to restore.")
            }
            TimelineError::NoActiveProject => {
                AppErrorPayload::new("TIMELINE_NO_ACTIVE_PROJECT", message)
                    .recoverable(true)
                    .with_suggestion("Load or create a project before editing the timeline.")
            }
            TimelineError::ClipboardEmpty => {
                AppErrorPayload::new("TIMELINE_CLIPBOARD_EMPTY", message)
                    .recoverable(true)
                    .with_suggestion("Copy one or more clips before pasting.")
            }
            TimelineError::ClipHasNoMedia { clip_id } => {
                AppErrorPayload::new("TIMELINE_CLIP_HAS_NO_MEDIA", message)
                    .with_details(clip_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Silence detection only applies to clips backed by imported media.",
                    )
            }
            TimelineError::MediaNotFound { media_id } => {
                AppErrorPayload::new("TIMELINE_MEDIA_NOT_FOUND", message)
                    .with_details(media_id.clone())
                    .recoverable(true)
                    .with_suggestion("The media item may have been removed from the project.")
            }
            TimelineError::InvalidSyncGroup { details } => {
                AppErrorPayload::new("TIMELINE_INVALID_SYNC_GROUP", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Select at least two clips and provide an offset for each.")
            }
            TimelineError::ClipAlreadyGrouped { clip_id, group_id } => {
                AppErrorPayload::new("TIMELINE_CLIP_ALREADY_GROUPED", message)
                    .with_details(format!("clip={clip_id} group={group_id}"))
                    .recoverable(true)
                    .with_suggestion("Remove the clip from its current sync group first.")
            }
            TimelineError::TimecodeUnavailable { details } => {
                AppErrorPayload::new("TIMELINE_TIMECODE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Not every selected clip's media has an embedded creation timestamp; \
                         enter offsets manually instead.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(TimelineError, &str)> = vec![
            (
                TimelineError::ClipNotFound {
                    clip_id: "c1".into(),
                },
                "TIMELINE_CLIP_NOT_FOUND",
            ),
            (
                TimelineError::TrackNotFound {
                    track_id: "t1".into(),
                },
                "TIMELINE_TRACK_NOT_FOUND",
            ),
            (
                TimelineError::TrackLocked {
                    track_id: "t1".into(),
                },
                "TIMELINE_TRACK_LOCKED",
            ),
            (
                TimelineError::SyncGroupNotFound {
                    group_id: "g1".into(),
                },
                "TIMELINE_SYNC_GROUP_NOT_FOUND",
            ),
            (
                TimelineError::InvalidTrimRange {
                    details: "x".into(),
                },
                "TIMELINE_INVALID_TRIM_RANGE",
            ),
            (
                TimelineError::InvalidSplitPosition {
                    details: "x".into(),
                },
                "TIMELINE_INVALID_SPLIT_POSITION",
            ),
            (
                TimelineError::InvalidMove {
                    details: "x".into(),
                },
                "TIMELINE_INVALID_MOVE",
            ),
            (
                TimelineError::ClipOverlap {
                    track_id: "t1".into(),
                    other_clip_id: "c2".into(),
                },
                "TIMELINE_CLIP_OVERLAP",
            ),
            (TimelineError::NothingToUndo, "TIMELINE_NOTHING_TO_UNDO"),
            (TimelineError::NothingToRedo, "TIMELINE_NOTHING_TO_REDO"),
            (TimelineError::NoActiveProject, "TIMELINE_NO_ACTIVE_PROJECT"),
            (TimelineError::ClipboardEmpty, "TIMELINE_CLIPBOARD_EMPTY"),
            (
                TimelineError::ClipHasNoMedia {
                    clip_id: "c1".into(),
                },
                "TIMELINE_CLIP_HAS_NO_MEDIA",
            ),
            (
                TimelineError::MediaNotFound {
                    media_id: "m1".into(),
                },
                "TIMELINE_MEDIA_NOT_FOUND",
            ),
            (
                TimelineError::InvalidSyncGroup {
                    details: "x".into(),
                },
                "TIMELINE_INVALID_SYNC_GROUP",
            ),
            (
                TimelineError::ClipAlreadyGrouped {
                    clip_id: "c1".into(),
                    group_id: "g1".into(),
                },
                "TIMELINE_CLIP_ALREADY_GROUPED",
            ),
            (
                TimelineError::TimecodeUnavailable {
                    details: "x".into(),
                },
                "TIMELINE_TIMECODE_UNAVAILABLE",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
