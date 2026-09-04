//! Multi-track sync: creating a `SyncGroup` from a set of selected clips
//! (master prompt §39/§40). Propagating edits *across* an existing group
//! (split/trim/move/delete cascading to every member) already exists in
//! `timeline::ops` and needed no changes — what was missing, and what this
//! module adds, is actually *creating* one in the first place.
//!
//! Two alignment strategies:
//! - [`SyncAlignment::Manual`]: caller supplies the offset for every clip
//!   directly — always available, always correct if the user measured it
//!   right.
//! - [`SyncAlignment::Timecode`]: best-effort, computed from each clip's
//!   `MediaItem::created_at` (an RFC3339 wall-clock timestamp) when every
//!   involved media item has one. This is intentionally coarse:
//!   second-resolution wall-clock time, **not** a frame-accurate embedded
//!   timecode — `project::types::MediaItem` has no such field today.
//!   `TimelineError::TimecodeUnavailable` is returned (never a fabricated
//!   frame-accurate result) whenever any clip's media lacks `created_at`, has
//!   no media at all, or the timestamp doesn't parse. Manual offset
//!   entry/correction is expected to remain the reliable path for real
//!   multi-camera work.

use std::collections::{HashMap, HashSet};

use crate::project::{ProjectV1, SyncGroup};

use super::command::{BatchCommand, Command, SetClipCommand, SetSyncGroupCommand};
use super::error::TimelineError;
use super::ops::find_clip;

/// How to compute each clip's relative offset when forming a new
/// `SyncGroup`.
pub enum SyncAlignment {
    /// Caller-supplied offset (microseconds) for every clip. Must contain
    /// exactly one entry per id passed to `create_sync_group`.
    Manual(HashMap<String, i64>),
    /// Computed from `MediaItem::created_at` — see module doc comment for
    /// the accuracy caveat.
    Timecode,
}

/// Builds the `Command` that creates a new `SyncGroup` from `clip_ids`,
/// joining each clip to it (`Clip::group_id`) and recording `offsets_us` per
/// `alignment`. Rejects fewer than two clips, duplicate ids, and any clip
/// already belonging to a group (ungroup it first).
pub fn create_sync_group(
    project: &ProjectV1,
    clip_ids: &[String],
    alignment: SyncAlignment,
) -> Result<Command, TimelineError> {
    if clip_ids.len() < 2 {
        return Err(TimelineError::InvalidSyncGroup {
            details: "a sync group needs at least two clips".to_string(),
        });
    }
    let mut seen = HashSet::with_capacity(clip_ids.len());
    for id in clip_ids {
        if !seen.insert(id.as_str()) {
            return Err(TimelineError::InvalidSyncGroup {
                details: format!("duplicate clip id {id}"),
            });
        }
    }

    let mut clips = Vec::with_capacity(clip_ids.len());
    for id in clip_ids {
        let clip = find_clip(project, id)?;
        if let Some(group_id) = &clip.group_id {
            return Err(TimelineError::ClipAlreadyGrouped {
                clip_id: id.clone(),
                group_id: group_id.clone(),
            });
        }
        clips.push(clip);
    }

    let offsets_us = match alignment {
        SyncAlignment::Manual(offsets) => {
            for id in clip_ids {
                if !offsets.contains_key(id) {
                    return Err(TimelineError::InvalidSyncGroup {
                        details: format!("missing manual offset for clip {id}"),
                    });
                }
            }
            offsets
        }
        SyncAlignment::Timecode => offsets_from_timecode(project, clip_ids)?,
    };

    let group_id = uuid::Uuid::new_v4().to_string();
    let mut commands = Vec::with_capacity(clip_ids.len() + 1);
    for clip in &clips {
        let mut new_clip = (*clip).clone();
        new_clip.group_id = Some(group_id.clone());
        commands.push(Command::SetClip(SetClipCommand {
            old: (*clip).clone(),
            new: new_clip,
        }));
    }
    commands.push(Command::SetSyncGroup(SetSyncGroupCommand {
        group_id: group_id.clone(),
        old: None,
        new: Some(SyncGroup {
            id: group_id,
            clip_ids: clip_ids.to_vec(),
            offsets_us,
        }),
    }));

    Ok(Command::Batch(BatchCommand { commands }))
}

/// Best-effort offsets derived from each clip's media `created_at`
/// (RFC3339), relative to the earliest timestamp in the group. Coarse
/// (whole-second wall-clock resolution) by construction — see module doc
/// comment; never fabricates frame accuracy the schema can't back.
fn offsets_from_timecode(
    project: &ProjectV1,
    clip_ids: &[String],
) -> Result<HashMap<String, i64>, TimelineError> {
    let mut timestamps_us: HashMap<String, i64> = HashMap::with_capacity(clip_ids.len());
    for id in clip_ids {
        let clip = find_clip(project, id)?;
        let media_id =
            clip.media_id
                .as_ref()
                .ok_or_else(|| TimelineError::TimecodeUnavailable {
                    details: format!("clip {id} has no media reference"),
                })?;
        let media = project
            .media
            .iter()
            .find(|m| &m.id == media_id)
            .ok_or_else(|| TimelineError::MediaNotFound {
                media_id: media_id.clone(),
            })?;
        let created_at =
            media
                .created_at
                .as_ref()
                .ok_or_else(|| TimelineError::TimecodeUnavailable {
                    details: format!(
                        "media {media_id} (clip {id}) has no embedded creation timestamp"
                    ),
                })?;
        let parsed = chrono::DateTime::parse_from_rfc3339(created_at).map_err(|e| {
            TimelineError::TimecodeUnavailable {
                details: format!(
                    "media {media_id} created_at {created_at:?} is not valid RFC3339: {e}"
                ),
            }
        })?;
        timestamps_us.insert(id.clone(), parsed.timestamp_micros());
    }

    let earliest = timestamps_us.values().copied().min().unwrap_or(0);
    Ok(timestamps_us
        .into_iter()
        .map(|(id, ts)| (id, ts - earliest))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Clip, ClipSettings, MediaItem, MediaKind, Rational, Track, TrackKind};

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            kind: TrackKind::Video,
            name: id.into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: Vec::new(),
        }
    }

    fn clip(id: &str, track_id: &str, media_id: Option<&str>) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: media_id.map(String::from),
            source_in_us: 0,
            source_out_us: 1_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    fn media(id: &str, created_at: Option<&str>) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind: MediaKind::Video,
            source_path: format!("/media/{id}.mp4"),
            duration_us: 10_000_000,
            width: 1920,
            height: 1080,
            fps: Rational::default(),
            codec: "h264".into(),
            bitrate: 1_000_000,
            audio_channels: 2,
            sample_rate: 48_000,
            rotation_deg: 0,
            created_at: created_at.map(String::from),
            proxy_path: None,
            thumbnail_path: None,
        }
    }

    fn two_clip_project() -> ProjectV1 {
        let mut p = ProjectV1::new("sync test");
        let mut t1 = track("t1");
        t1.clip_ids.push("a".into());
        let mut t2 = track("t2");
        t2.clip_ids.push("b".into());
        p.tracks.push(t1);
        p.tracks.push(t2);
        p.clips.push(clip("a", "t1", Some("m1")));
        p.clips.push(clip("b", "t2", Some("m2")));
        p
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    #[test]
    fn manual_alignment_creates_a_group_and_joins_both_clips() {
        let project = two_clip_project();
        let offsets = HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 200_000i64)]);
        let cmd = create_sync_group(
            &project,
            &["a".to_string(), "b".to_string()],
            SyncAlignment::Manual(offsets),
        )
        .unwrap();
        let mut project = project;
        apply(&mut project, cmd);

        assert_eq!(project.sync_groups.len(), 1);
        let group = &project.sync_groups[0];
        assert_eq!(group.clip_ids.len(), 2);
        assert_eq!(group.offsets_us.get("b"), Some(&200_000));
        let a = project.clips.iter().find(|c| c.id == "a").unwrap();
        assert_eq!(a.group_id.as_deref(), Some(group.id.as_str()));
        let b = project.clips.iter().find(|c| c.id == "b").unwrap();
        assert_eq!(b.group_id.as_deref(), Some(group.id.as_str()));
    }

    #[test]
    fn manual_alignment_rejects_a_missing_offset() {
        let project = two_clip_project();
        let offsets = HashMap::from([("a".to_string(), 0i64)]); // "b" missing
        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "b".to_string()],
                SyncAlignment::Manual(offsets),
            )
            .unwrap_err(),
            TimelineError::InvalidSyncGroup { .. }
        ));
    }

    #[test]
    fn fewer_than_two_clips_is_rejected() {
        let project = two_clip_project();
        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string()],
                SyncAlignment::Manual(HashMap::new()),
            )
            .unwrap_err(),
            TimelineError::InvalidSyncGroup { .. }
        ));
    }

    #[test]
    fn duplicate_clip_ids_are_rejected() {
        let project = two_clip_project();
        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "a".to_string()],
                SyncAlignment::Manual(HashMap::new()),
            )
            .unwrap_err(),
            TimelineError::InvalidSyncGroup { .. }
        ));
    }

    #[test]
    fn a_clip_already_in_a_group_is_rejected() {
        let mut project = two_clip_project();
        project.clips[0].group_id = Some("existing-group".into());
        let offsets = HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 0i64)]);
        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "b".to_string()],
                SyncAlignment::Manual(offsets),
            )
            .unwrap_err(),
            TimelineError::ClipAlreadyGrouped { .. }
        ));
    }

    #[test]
    fn timecode_alignment_computes_offsets_from_created_at() {
        let mut project = two_clip_project();
        project
            .media
            .push(media("m1", Some("2026-01-01T10:00:00Z")));
        project
            .media
            .push(media("m2", Some("2026-01-01T10:00:05Z")));

        let cmd = create_sync_group(
            &project,
            &["a".to_string(), "b".to_string()],
            SyncAlignment::Timecode,
        )
        .unwrap();
        apply(&mut project, cmd);

        let group = &project.sync_groups[0];
        assert_eq!(group.offsets_us.get("a"), Some(&0));
        assert_eq!(group.offsets_us.get("b"), Some(&5_000_000));
    }

    #[test]
    fn timecode_alignment_fails_when_any_media_lacks_created_at() {
        let mut project = two_clip_project();
        project
            .media
            .push(media("m1", Some("2026-01-01T10:00:00Z")));
        project.media.push(media("m2", None));

        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "b".to_string()],
                SyncAlignment::Timecode,
            )
            .unwrap_err(),
            TimelineError::TimecodeUnavailable { .. }
        ));
    }

    #[test]
    fn timecode_alignment_fails_on_unparsable_timestamp() {
        let mut project = two_clip_project();
        project.media.push(media("m1", Some("not-a-date")));
        project
            .media
            .push(media("m2", Some("2026-01-01T10:00:05Z")));

        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "b".to_string()],
                SyncAlignment::Timecode,
            )
            .unwrap_err(),
            TimelineError::TimecodeUnavailable { .. }
        ));
    }

    #[test]
    fn timecode_alignment_fails_when_a_clip_has_no_media() {
        let mut project = two_clip_project();
        project.clips[0].media_id = None;
        project
            .media
            .push(media("m2", Some("2026-01-01T10:00:05Z")));

        assert!(matches!(
            create_sync_group(
                &project,
                &["a".to_string(), "b".to_string()],
                SyncAlignment::Timecode,
            )
            .unwrap_err(),
            TimelineError::TimecodeUnavailable { .. }
        ));
    }

    #[test]
    fn created_group_propagates_split_to_both_members() {
        // Confirms this module's new groups plug into the *existing*
        // `timeline::ops` propagation (module doc comment) rather than
        // needing their own cascade logic.
        let mut project = two_clip_project();
        project.clips[0].source_out_us = 5_000_000;
        project.clips[1].source_out_us = 5_000_000;
        let offsets = HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 0i64)]);
        let cmd = create_sync_group(
            &project,
            &["a".to_string(), "b".to_string()],
            SyncAlignment::Manual(offsets),
        )
        .unwrap();
        apply(&mut project, cmd);

        let split_cmd = crate::timeline::ops::split_clip(&project, "a", 2_000_000).unwrap();
        apply(&mut project, split_cmd);

        assert_eq!(project.clips.len(), 4);
        let b_pieces: Vec<_> = project
            .clips
            .iter()
            .filter(|c| c.track_id == "t2")
            .collect();
        assert_eq!(b_pieces.len(), 2, "b should have been split too");
    }
}
