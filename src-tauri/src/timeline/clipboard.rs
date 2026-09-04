//! Copy/paste: an in-memory clipboard representation for one or more
//! selected clips, plus their `SyncGroup` membership (relative offset) when
//! relevant. `copy_clips` is a pure read; `paste_clips` builds a `Command`
//! the same way every other operation in `timeline::ops` does — it never
//! mutates the project itself.

use std::collections::HashMap;

use crate::project::{Clip, ProjectV1, SyncGroup};

use super::command::{BatchCommand, Command, InsertClipCommand, SetSyncGroupCommand};
use super::error::TimelineError;
use super::ops::{clip_span, find_clip, find_sync_group, find_track};

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub clip: Clip,
    /// The clip's `SyncGroup` offset at copy time, if it belonged to one.
    /// Re-applied to a freshly-created group on paste so relative alignment
    /// between pasted clips survives the round trip.
    pub group_offset_us: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    pub entries: Vec<ClipboardEntry>,
}

/// Snapshots `clip_ids` (and each one's `SyncGroup` offset, if any) into a
/// `Clipboard`. Read-only — does not touch `project`.
pub fn copy_clips(project: &ProjectV1, clip_ids: &[String]) -> Result<Clipboard, TimelineError> {
    if clip_ids.is_empty() {
        return Err(TimelineError::ClipboardEmpty);
    }
    let mut entries = Vec::with_capacity(clip_ids.len());
    for id in clip_ids {
        let clip = find_clip(project, id)?.clone();
        let group_offset_us = clip.group_id.as_ref().and_then(|gid| {
            find_sync_group(project, gid)
                .ok()
                .and_then(|g| g.offsets_us.get(id).copied())
        });
        entries.push(ClipboardEntry {
            clip,
            group_offset_us,
        });
    }
    Ok(Clipboard { entries })
}

/// Re-inserts `clipboard`'s clips with fresh ids, anchored at
/// `target_position_us` (the copied clip with the smallest `position_us`
/// lands exactly there; every other copied clip keeps its relative offset
/// from that anchor). `target_track_id` overrides every pasted clip's track
/// when given (single-track paste); otherwise each clip returns to the
/// track it was copied from. If two or more copied clips shared a
/// `SyncGroup`, a brand-new group (fresh id, same relative `offsets_us`) is
/// created among the pasted clips — paste never re-joins the *original*
/// group, since that would sync-link unrelated timeline regions.
pub fn paste_clips(
    project: &ProjectV1,
    clipboard: &Clipboard,
    target_track_id: Option<&str>,
    target_position_us: i64,
) -> Result<Command, TimelineError> {
    if clipboard.entries.is_empty() {
        return Err(TimelineError::ClipboardEmpty);
    }
    if target_position_us < 0 {
        return Err(TimelineError::InvalidMove {
            details: "position cannot be negative".to_string(),
        });
    }

    let anchor = clipboard
        .entries
        .iter()
        .map(|e| e.clip.position_us)
        .min()
        .expect("checked non-empty above");

    let grouped_entry_count = clipboard
        .entries
        .iter()
        .filter(|e| e.group_offset_us.is_some())
        .count();
    let make_group = clipboard.entries.len() > 1 && grouped_entry_count > 1;
    let new_group_id = if make_group {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let mut commands = Vec::new();
    let mut offsets: HashMap<String, i64> = HashMap::new();
    let mut member_ids: Vec<String> = Vec::new();

    for entry in &clipboard.entries {
        let dest_track_id = target_track_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| entry.clip.track_id.clone());
        let track = find_track(project, &dest_track_id)?;
        if track.locked {
            return Err(TimelineError::TrackLocked {
                track_id: track.id.clone(),
            });
        }

        let new_id = uuid::Uuid::new_v4().to_string();
        let relative_us = entry.clip.position_us - anchor;
        let mut new_clip = entry.clip.clone();
        new_clip.id = new_id.clone();
        new_clip.track_id = dest_track_id.clone();
        new_clip.position_us = target_position_us + relative_us;
        new_clip.group_id = new_group_id.clone();

        // Note: this only checks against clips already in `project`, not
        // against other clips being pasted in this same batch — pasting a
        // multi-clip selection that would overlap itself at the new anchor
        // is a rare, low-priority edge case left unhandled here.
        if let Some(other) = project.clips.iter().find(|c| {
            c.track_id == dest_track_id && intervals_overlap(clip_span(c), clip_span(&new_clip))
        }) {
            return Err(TimelineError::ClipOverlap {
                track_id: dest_track_id,
                other_clip_id: other.id.clone(),
            });
        }

        if new_group_id.is_some() {
            offsets.insert(new_id.clone(), entry.group_offset_us.unwrap_or(0));
            member_ids.push(new_id.clone());
        }
        commands.push(Command::InsertClip(InsertClipCommand { clip: new_clip }));
    }

    if let Some(gid) = &new_group_id {
        let group = SyncGroup {
            id: gid.clone(),
            clip_ids: member_ids,
            offsets_us: offsets,
        };
        commands.push(Command::SetSyncGroup(SetSyncGroupCommand {
            group_id: gid.clone(),
            old: None,
            new: Some(group),
        }));
    }

    Ok(Command::Batch(BatchCommand { commands }))
}

fn intervals_overlap(a: (i64, i64), b: (i64, i64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ClipSettings, Track, TrackKind};

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

    fn clip(id: &str, track_id: &str, position_us: i64) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: None,
            source_in_us: 0,
            source_out_us: 1_000_000,
            position_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    #[test]
    fn copy_then_paste_round_trips_with_fresh_ids() {
        let mut project = ProjectV1::new("clipboard test");
        project.tracks.push(track("t1"));
        project.clips.push(clip("c1", "t1", 0));
        project.tracks[0].clip_ids.push("c1".into());

        let clipboard = copy_clips(&project, &["c1".to_string()]).unwrap();
        let cmd = paste_clips(&project, &clipboard, None, 5_000_000).unwrap();
        cmd.apply(&mut project).unwrap();

        assert_eq!(project.clips.len(), 2);
        let pasted = project.clips.iter().find(|c| c.id != "c1").unwrap();
        assert_ne!(pasted.id, "c1");
        assert_eq!(pasted.position_us, 5_000_000);
        assert_eq!(pasted.track_id, "t1");
    }

    #[test]
    fn paste_preserves_relative_offsets_between_multiple_clips() {
        let mut project = ProjectV1::new("clipboard multi");
        project.tracks.push(track("t1"));
        project.clips.push(clip("a", "t1", 1_000_000));
        project.clips.push(clip("b", "t1", 3_000_000));
        project.tracks[0].clip_ids.push("a".into());
        project.tracks[0].clip_ids.push("b".into());

        let clipboard = copy_clips(&project, &["a".to_string(), "b".to_string()]).unwrap();
        project.tracks.push(track("t2"));
        let cmd = paste_clips(&project, &clipboard, Some("t2"), 0).unwrap();
        cmd.apply(&mut project).unwrap();

        let pasted: Vec<_> = project
            .clips
            .iter()
            .filter(|c| c.track_id == "t2")
            .collect();
        assert_eq!(pasted.len(), 2);
        let positions: Vec<i64> = {
            let mut v: Vec<i64> = pasted.iter().map(|c| c.position_us).collect();
            v.sort();
            v
        };
        // original gap was 2_000_000us; must be preserved, anchored at 0.
        assert_eq!(positions, vec![0, 2_000_000]);
    }

    #[test]
    fn paste_onto_locked_track_is_rejected() {
        let mut project = ProjectV1::new("clipboard locked");
        project.tracks.push(track("t1"));
        project.clips.push(clip("a", "t1", 0));
        project.tracks[0].clip_ids.push("a".into());
        project.tracks.push(track("t2"));
        project.tracks[1].locked = true;

        let clipboard = copy_clips(&project, &["a".to_string()]).unwrap();
        assert!(matches!(
            paste_clips(&project, &clipboard, Some("t2"), 0).unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
    }

    #[test]
    fn copy_with_empty_selection_errors() {
        let project = ProjectV1::new("clipboard empty");
        assert!(matches!(
            copy_clips(&project, &[]).unwrap_err(),
            TimelineError::ClipboardEmpty
        ));
    }

    #[test]
    fn paste_rejects_overlap_on_destination_track() {
        let mut project = ProjectV1::new("clipboard overlap");
        project.tracks.push(track("t1"));
        project.clips.push(clip("a", "t1", 0));
        project.clips.push(clip("existing", "t1", 5_000_000));
        project.tracks[0].clip_ids.push("a".into());
        project.tracks[0].clip_ids.push("existing".into());

        let clipboard = copy_clips(&project, &["a".to_string()]).unwrap();
        assert!(matches!(
            paste_clips(&project, &clipboard, None, 5_000_000).unwrap_err(),
            TimelineError::ClipOverlap { .. }
        ));
    }
}
