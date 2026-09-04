//! Clip and track operations: split, trim start/end, move, delete,
//! duplicate, snap-to-nearest, track lock/hide/mute/solo, and the
//! solo-affects-effective-mute computation. Every clip-mutating function
//! here is a pure builder: it reads `&ProjectV1` and returns a
//! `timeline::command::Command` describing the edit — it never mutates the
//! project itself. The caller (`timeline::session::TimelineSession` in
//! practice, or a test directly) applies the returned command through
//! `timeline::command::History` so undo/redo stays centralized.
//!
//! `SyncGroup` propagation (master prompt §39: "when silence cut is applied
//! based on microphone, all linked tracks should cut together") happens
//! inside `split_clip`/`trim_clip_start`/`trim_clip_end`/`move_clip`: each
//! builds one `Command::Batch` containing the primary clip's edit plus the
//! equivalent edit for every other member of its `SyncGroup`, so the whole
//! propagated edit is a single undo step. A group member whose equivalent
//! edit would be invalid (falls outside its own span, or would overlap a
//! neighboring clip) is silently skipped rather than failing the primary
//! edit — sync propagation is a best-effort enhancement of the operation
//! the user actually asked for, not a blocker on it.

use std::collections::HashMap;

use crate::project::{Clip, ProjectV1, SyncGroup, Track, TrackKind};

use super::command::{
    BatchCommand, Command, InsertClipCommand, RemoveClipCommand, SetClipCommand,
    SetSyncGroupCommand, SetTrackCommand,
};
use super::error::TimelineError;

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

pub(crate) fn find_clip<'a>(
    project: &'a ProjectV1,
    clip_id: &str,
) -> Result<&'a Clip, TimelineError> {
    project
        .clips
        .iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| TimelineError::ClipNotFound {
            clip_id: clip_id.to_string(),
        })
}

pub(crate) fn find_track<'a>(
    project: &'a ProjectV1,
    track_id: &str,
) -> Result<&'a Track, TimelineError> {
    project
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .ok_or_else(|| TimelineError::TrackNotFound {
            track_id: track_id.to_string(),
        })
}

pub(crate) fn find_sync_group<'a>(
    project: &'a ProjectV1,
    group_id: &str,
) -> Result<&'a SyncGroup, TimelineError> {
    project
        .sync_groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| TimelineError::SyncGroupNotFound {
            group_id: group_id.to_string(),
        })
}

fn require_unlocked(project: &ProjectV1, track_id: &str) -> Result<(), TimelineError> {
    let track = find_track(project, track_id)?;
    if track.locked {
        return Err(TimelineError::TrackLocked {
            track_id: track.id.clone(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Span / overlap math
// ---------------------------------------------------------------------------

/// A clip's `[start_us, end_us)` on its track's timeline, derived from its
/// source trim and playback speed. `speed` scales source-duration down to
/// timeline-duration (speed 2.0 plays twice as fast, so the same source span
/// occupies half the timeline time).
pub(crate) fn clip_span(clip: &Clip) -> (i64, i64) {
    let source_span = (clip.source_out_us - clip.source_in_us).max(0);
    let timeline_duration = if clip.speed > 0.0 {
        (source_span as f64 / clip.speed).round() as i64
    } else {
        source_span
    };
    (
        clip.position_us,
        clip.position_us + timeline_duration.max(0),
    )
}

fn spans_overlap(a: (i64, i64), b: (i64, i64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Converts a *timeline* microsecond delta into the equivalent *source*
/// microsecond delta for a clip playing at `speed` (the inverse of the
/// scaling `clip_span` applies).
fn timeline_delta_to_source_delta(delta_us: i64, speed: f64) -> i64 {
    (delta_us as f64 * speed).round() as i64
}

/// The inverse of `timeline_delta_to_source_delta`: converts a *source*
/// microsecond delta into the equivalent *timeline* microsecond delta for a
/// clip playing at `speed`. `pub(crate)` (not `fn`-private) because
/// `timeline::silence` needs it to translate a `Cut`'s source-media-relative
/// interval into a timeline position before it can call `split_clip`/
/// `trim_clip_start`/`trim_clip_end`, which all take timeline positions.
pub(crate) fn source_delta_to_timeline_delta(delta_us: i64, speed: f64) -> i64 {
    if speed > 0.0 {
        (delta_us as f64 / speed).round() as i64
    } else {
        delta_us
    }
}

fn find_overlap<'a>(
    project: &'a ProjectV1,
    track_id: &str,
    span: (i64, i64),
    exclude_clip_id: &str,
) -> Option<&'a Clip> {
    project.clips.iter().find(|c| {
        c.track_id == track_id && c.id != exclude_clip_id && spans_overlap(clip_span(c), span)
    })
}

// ---------------------------------------------------------------------------
// Split
// ---------------------------------------------------------------------------

/// Splits `clip_id` at absolute timeline position `split_at_us`, producing
/// two clips: the original id (shortened) and a freshly-generated id for the
/// tail half. If the clip belongs to a `SyncGroup`, every other member is
/// split at its offset-adjusted equivalent position too, and the new tail
/// clips join the same group (master prompt §39).
pub fn split_clip(
    project: &ProjectV1,
    clip_id: &str,
    split_at_us: i64,
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    require_unlocked(project, &clip.track_id)?;

    let (start, end) = clip_span(clip);
    if split_at_us <= start || split_at_us >= end {
        return Err(TimelineError::InvalidSplitPosition {
            details: format!("split point {split_at_us} must be strictly inside [{start}, {end})"),
        });
    }

    let mut commands = Vec::new();
    let new_clip_id = uuid::Uuid::new_v4().to_string();
    let (first_half, second_half) = split_single_clip(clip, split_at_us, new_clip_id.clone());
    commands.push(Command::SetClip(SetClipCommand {
        old: clip.clone(),
        new: first_half,
    }));
    commands.push(Command::InsertClip(InsertClipCommand { clip: second_half }));

    if let Some(group_id) = &clip.group_id {
        if let Ok(group) = find_sync_group(project, group_id) {
            let primary_offset = group.offsets_us.get(clip_id).copied().unwrap_or(0);
            let t_ref = split_at_us - primary_offset;
            let mut new_offsets: HashMap<String, i64> = HashMap::new();
            new_offsets.insert(new_clip_id.clone(), primary_offset);
            let mut new_member_ids = Vec::new();

            for member_id in group.clip_ids.iter().filter(|id| id.as_str() != clip_id) {
                let Ok(member_clip) = find_clip(project, member_id) else {
                    continue;
                };
                // Locked member tracks are skipped, not fatal: propagation
                // is best-effort (see module doc comment).
                if find_track(project, &member_clip.track_id)
                    .map(|t| t.locked)
                    .unwrap_or(true)
                {
                    continue;
                }
                let member_offset = group.offsets_us.get(member_id).copied().unwrap_or(0);
                let member_split_at = t_ref + member_offset;
                let (m_start, m_end) = clip_span(member_clip);
                if member_split_at <= m_start || member_split_at >= m_end {
                    continue;
                }
                let member_new_id = uuid::Uuid::new_v4().to_string();
                let (m_first, m_second) =
                    split_single_clip(member_clip, member_split_at, member_new_id.clone());
                commands.push(Command::SetClip(SetClipCommand {
                    old: member_clip.clone(),
                    new: m_first,
                }));
                commands.push(Command::InsertClip(InsertClipCommand { clip: m_second }));
                new_offsets.insert(member_new_id.clone(), member_offset);
                new_member_ids.push(member_new_id);
            }

            let mut updated_group = group.clone();
            updated_group.clip_ids.push(new_clip_id.clone());
            updated_group.clip_ids.extend(new_member_ids);
            updated_group.offsets_us.extend(new_offsets);
            commands.push(Command::SetSyncGroup(SetSyncGroupCommand {
                group_id: group.id.clone(),
                old: Some(group.clone()),
                new: Some(updated_group),
            }));
        }
    }

    Ok(Command::Batch(BatchCommand { commands }))
}

/// Pure helper: given a clip and an absolute split point inside its span,
/// returns `(shortened_first_half, new_second_half)`. Does not touch the
/// project or generate the new id (callers supply `new_clip_id` so it can be
/// captured once, deterministically, at command-construction time).
fn split_single_clip(clip: &Clip, split_at_us: i64, new_clip_id: String) -> (Clip, Clip) {
    let (start, _end) = clip_span(clip);
    let delta_from_start = split_at_us - start;
    let source_delta = timeline_delta_to_source_delta(delta_from_start, clip.speed);
    let boundary_source_us = clip.source_in_us + source_delta;

    let mut first_half = clip.clone();
    first_half.source_out_us = boundary_source_us;

    let mut second_half = clip.clone();
    second_half.id = new_clip_id;
    second_half.source_in_us = boundary_source_us;
    second_half.position_us = split_at_us;

    (first_half, second_half)
}

// ---------------------------------------------------------------------------
// Trim start / end
// ---------------------------------------------------------------------------

/// Trims the clip's start edge to `new_start_us` (the end edge stays fixed).
/// Equivalent to a drag-resize of the clip's left handle.
pub fn trim_clip_start(
    project: &ProjectV1,
    clip_id: &str,
    new_start_us: i64,
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    require_unlocked(project, &clip.track_id)?;

    let (start, end) = clip_span(clip);
    if new_start_us >= end {
        return Err(TimelineError::InvalidTrimRange {
            details: format!("new start {new_start_us} must be before clip end {end}"),
        });
    }
    let delta_us = new_start_us - start;
    let new_clip =
        apply_start_delta(clip, delta_us).ok_or_else(|| TimelineError::InvalidTrimRange {
            details: "resulting trim would leave an empty or out-of-source-range clip".to_string(),
        })?;
    if let Some(other) = find_overlap(project, &clip.track_id, clip_span(&new_clip), &clip.id) {
        return Err(TimelineError::ClipOverlap {
            track_id: clip.track_id.clone(),
            other_clip_id: other.id.clone(),
        });
    }

    let mut commands = vec![Command::SetClip(SetClipCommand {
        old: clip.clone(),
        new: new_clip,
    })];
    propagate_delta_to_group(project, clip, delta_us, apply_start_delta, &mut commands);
    Ok(Command::Batch(BatchCommand { commands }))
}

/// Trims the clip's end edge to `new_end_us` (the start edge stays fixed).
pub fn trim_clip_end(
    project: &ProjectV1,
    clip_id: &str,
    new_end_us: i64,
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    require_unlocked(project, &clip.track_id)?;

    let (start, end) = clip_span(clip);
    if new_end_us <= start {
        return Err(TimelineError::InvalidTrimRange {
            details: format!("new end {new_end_us} must be after clip start {start}"),
        });
    }
    let delta_us = new_end_us - end;
    let new_clip =
        apply_end_delta(clip, delta_us).ok_or_else(|| TimelineError::InvalidTrimRange {
            details: "resulting trim would leave an empty or out-of-source-range clip".to_string(),
        })?;
    if let Some(other) = find_overlap(project, &clip.track_id, clip_span(&new_clip), &clip.id) {
        return Err(TimelineError::ClipOverlap {
            track_id: clip.track_id.clone(),
            other_clip_id: other.id.clone(),
        });
    }

    let mut commands = vec![Command::SetClip(SetClipCommand {
        old: clip.clone(),
        new: new_clip,
    })];
    propagate_delta_to_group(project, clip, delta_us, apply_end_delta, &mut commands);
    Ok(Command::Batch(BatchCommand { commands }))
}

fn apply_start_delta(clip: &Clip, delta_us: i64) -> Option<Clip> {
    let source_delta = timeline_delta_to_source_delta(delta_us, clip.speed);
    let new_source_in = clip.source_in_us + source_delta;
    if new_source_in < 0 || new_source_in >= clip.source_out_us {
        return None;
    }
    let mut new_clip = clip.clone();
    new_clip.source_in_us = new_source_in;
    new_clip.position_us += delta_us;
    Some(new_clip)
}

fn apply_end_delta(clip: &Clip, delta_us: i64) -> Option<Clip> {
    let source_delta = timeline_delta_to_source_delta(delta_us, clip.speed);
    let new_source_out = clip.source_out_us + source_delta;
    if new_source_out <= clip.source_in_us {
        return None;
    }
    let mut new_clip = clip.clone();
    new_clip.source_out_us = new_source_out;
    Some(new_clip)
}

/// Applies `delta_us` (same real-world time delta as the primary edit) to
/// every other `SyncGroup` member via `edit`, pushing a `SetClip` for each
/// one whose result is valid and non-overlapping. Members are looked up
/// fresh from `project` (the pre-edit state), consistent with every command
/// here being built from a single read of the project.
fn propagate_delta_to_group(
    project: &ProjectV1,
    clip: &Clip,
    delta_us: i64,
    edit: fn(&Clip, i64) -> Option<Clip>,
    commands: &mut Vec<Command>,
) {
    let Some(group_id) = &clip.group_id else {
        return;
    };
    let Ok(group) = find_sync_group(project, group_id) else {
        return;
    };
    for member_id in group.clip_ids.iter().filter(|id| id.as_str() != clip.id) {
        let Ok(member_clip) = find_clip(project, member_id) else {
            continue;
        };
        if find_track(project, &member_clip.track_id)
            .map(|t| t.locked)
            .unwrap_or(true)
        {
            continue;
        }
        let Some(new_member) = edit(member_clip, delta_us) else {
            continue;
        };
        if find_overlap(
            project,
            &member_clip.track_id,
            clip_span(&new_member),
            &member_clip.id,
        )
        .is_some()
        {
            continue;
        }
        commands.push(Command::SetClip(SetClipCommand {
            old: member_clip.clone(),
            new: new_member,
        }));
    }
}

// ---------------------------------------------------------------------------
// Move
// ---------------------------------------------------------------------------

/// Moves `clip_id` to `new_position_us`, optionally onto a different track.
/// `SyncGroup` members are repositioned by the same delta, on their own
/// tracks (only the directly-moved clip can change track).
pub fn move_clip(
    project: &ProjectV1,
    clip_id: &str,
    target_track_id: &str,
    new_position_us: i64,
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    require_unlocked(project, &clip.track_id)?;
    require_unlocked(project, target_track_id)?;
    find_track(project, target_track_id)?;

    let delta_us = new_position_us - clip.position_us;
    let mut new_clip = clip.clone();
    new_clip.track_id = target_track_id.to_string();
    new_clip.position_us = new_position_us;
    if new_position_us < 0 {
        return Err(TimelineError::InvalidMove {
            details: "position cannot be negative".to_string(),
        });
    }
    if let Some(other) = find_overlap(project, target_track_id, clip_span(&new_clip), &clip.id) {
        return Err(TimelineError::ClipOverlap {
            track_id: target_track_id.to_string(),
            other_clip_id: other.id.clone(),
        });
    }

    let mut commands = vec![Command::SetClip(SetClipCommand {
        old: clip.clone(),
        new: new_clip,
    })];
    propagate_delta_to_group(
        project,
        clip,
        delta_us,
        |c, delta| {
            let np = c.position_us + delta;
            if np < 0 {
                return None;
            }
            let mut nc = c.clone();
            nc.position_us = np;
            Some(nc)
        },
        &mut commands,
    );
    Ok(Command::Batch(BatchCommand { commands }))
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Deletes `clip_id`. If it belongs to a `SyncGroup`, every other member of
/// that group is deleted too (master prompt §39) — deletion is the one
/// propagated edit where "equivalent operation" unambiguously means "also
/// delete", not a offset-adjusted variant.
pub fn delete_clip(project: &ProjectV1, clip_id: &str) -> Result<Command, TimelineError> {
    delete_clips(project, std::slice::from_ref(&clip_id.to_string()))
}

/// Deletes every clip in `clip_ids` (plus their sync-group cascades) as one
/// atomic batch/undo step — the backend-side "multi-select delete" the
/// frontend can drive with a single call instead of N single-clip commands.
pub fn delete_clips(project: &ProjectV1, clip_ids: &[String]) -> Result<Command, TimelineError> {
    use std::collections::HashSet;

    if clip_ids.is_empty() {
        return Err(TimelineError::ClipNotFound {
            clip_id: "<none provided>".to_string(),
        });
    }

    let mut to_remove: HashSet<String> = HashSet::new();
    let mut affected_groups: HashSet<String> = HashSet::new();

    for id in clip_ids {
        let clip = find_clip(project, id)?;
        require_unlocked(project, &clip.track_id)?;
        to_remove.insert(id.clone());
        if let Some(gid) = &clip.group_id {
            affected_groups.insert(gid.clone());
        }
    }

    for gid in &affected_groups {
        let group = find_sync_group(project, gid)?;
        for member_id in &group.clip_ids {
            let member_clip = find_clip(project, member_id)?;
            require_unlocked(project, &member_clip.track_id)?;
            to_remove.insert(member_id.clone());
        }
    }

    let mut commands = Vec::new();
    // Deterministic order (sorted ids) so the resulting Batch's structure
    // doesn't depend on HashSet iteration order — makes tests reproducible.
    let mut ordered: Vec<&String> = to_remove.iter().collect();
    ordered.sort();
    for id in ordered {
        let clip = find_clip(project, id)?;
        commands.push(Command::RemoveClip(RemoveClipCommand {
            clip: clip.clone(),
        }));
    }
    let mut ordered_groups: Vec<&String> = affected_groups.iter().collect();
    ordered_groups.sort();
    for gid in ordered_groups {
        let group = find_sync_group(project, gid)?;
        commands.push(Command::SetSyncGroup(SetSyncGroupCommand {
            group_id: gid.clone(),
            old: Some(group.clone()),
            new: None,
        }));
    }

    Ok(Command::Batch(BatchCommand { commands }))
}

// ---------------------------------------------------------------------------
// Duplicate
// ---------------------------------------------------------------------------

/// Duplicates `clip_id` onto `target_track_id` (defaults to the source
/// clip's own track) at `new_position_us`, with a fresh id. The duplicate
/// does not inherit `SyncGroup` membership — duplication isn't one of the
/// propagated operations (master prompt §39 only lists split/trim/delete/
/// move), and auto-joining a group would create a sync relationship the
/// user never asked for.
pub fn duplicate_clip(
    project: &ProjectV1,
    clip_id: &str,
    new_position_us: i64,
    target_track_id: Option<&str>,
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    let dest_track_id = target_track_id.unwrap_or(&clip.track_id);
    require_unlocked(project, dest_track_id)?;
    find_track(project, dest_track_id)?;

    if new_position_us < 0 {
        return Err(TimelineError::InvalidMove {
            details: "position cannot be negative".to_string(),
        });
    }

    let mut new_clip = clip.clone();
    new_clip.id = uuid::Uuid::new_v4().to_string();
    new_clip.track_id = dest_track_id.to_string();
    new_clip.position_us = new_position_us;
    new_clip.group_id = None;

    if let Some(other) = find_overlap(project, dest_track_id, clip_span(&new_clip), &new_clip.id) {
        return Err(TimelineError::ClipOverlap {
            track_id: dest_track_id.to_string(),
            other_clip_id: other.id.clone(),
        });
    }

    Ok(Command::InsertClip(InsertClipCommand { clip: new_clip }))
}

// ---------------------------------------------------------------------------
// Snap
// ---------------------------------------------------------------------------

/// Returns the candidate in `candidates` nearest `target_us`, provided it is
/// within `threshold_us`; `None` if every candidate exceeds the threshold or
/// `candidates` is empty. Ties (equal distance) break toward the smaller
/// absolute time value — arbitrary but deterministic, documented here so
/// frontend snap behavior is predictable rather than HashMap-iteration-order
/// dependent.
pub fn snap_to_candidates(target_us: i64, candidates: &[i64], threshold_us: i64) -> Option<i64> {
    candidates
        .iter()
        .copied()
        .map(|c| (c, (c - target_us).abs()))
        .filter(|&(_, dist)| dist <= threshold_us)
        .min_by_key(|&(c, dist)| (dist, c))
        .map(|(c, _)| c)
}

// ---------------------------------------------------------------------------
// Track flags
// ---------------------------------------------------------------------------

fn set_track_flag(
    project: &ProjectV1,
    track_id: &str,
    apply: impl FnOnce(&mut Track),
) -> Result<Command, TimelineError> {
    let old = find_track(project, track_id)?.clone();
    let mut new = old.clone();
    apply(&mut new);
    Ok(Command::SetTrack(SetTrackCommand { old, new }))
}

pub fn set_track_locked(
    project: &ProjectV1,
    track_id: &str,
    locked: bool,
) -> Result<Command, TimelineError> {
    set_track_flag(project, track_id, |t| t.locked = locked)
}

pub fn set_track_hidden(
    project: &ProjectV1,
    track_id: &str,
    hidden: bool,
) -> Result<Command, TimelineError> {
    set_track_flag(project, track_id, |t| t.hidden = hidden)
}

pub fn set_track_muted(
    project: &ProjectV1,
    track_id: &str,
    muted: bool,
) -> Result<Command, TimelineError> {
    set_track_flag(project, track_id, |t| t.muted = muted)
}

pub fn set_track_solo(
    project: &ProjectV1,
    track_id: &str,
    solo: bool,
) -> Result<Command, TimelineError> {
    set_track_flag(project, track_id, |t| t.solo = solo)
}

/// Pure function of the track list (master prompt: "if any track has
/// solo = true, all non-solo audio tracks are effectively muted"). Only
/// `Audio` tracks participate in solo/mute semantics; other track kinds
/// simply reflect their own `muted` flag. Useful to the render/preview layer
/// later, computed here since it's timeline-state logic, not a rendering
/// concern.
pub fn effective_track_mute_state(tracks: &[Track]) -> HashMap<String, bool> {
    let any_audio_solo = tracks.iter().any(|t| t.kind == TrackKind::Audio && t.solo);
    tracks
        .iter()
        .map(|t| {
            let effective = if t.kind != TrackKind::Audio {
                t.muted
            } else if t.muted {
                true
            } else {
                any_audio_solo && !t.solo
            };
            (t.id.clone(), effective)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ClipSettings;

    fn track(id: &str, kind: TrackKind) -> Track {
        Track {
            id: id.into(),
            kind,
            name: id.into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: Vec::new(),
        }
    }

    fn clip(id: &str, track_id: &str, position_us: i64, source_in: i64, source_out: i64) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: None,
            source_in_us: source_in,
            source_out_us: source_out,
            position_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    fn project_with_clip(c: Clip) -> ProjectV1 {
        let mut p = ProjectV1::new("ops test");
        let mut t = track(&c.track_id, TrackKind::Video);
        t.clip_ids.push(c.id.clone());
        p.tracks.push(t);
        p.clips.push(c);
        p
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    // -- split -------------------------------------------------------------

    #[test]
    fn split_produces_two_correctly_adjusted_clips() {
        let c = clip("c1", "t1", 1_000_000, 0, 5_000_000);
        let mut project = project_with_clip(c);
        let cmd = split_clip(&project, "c1", 3_000_000).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 2);
        let first = project.clips.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(first.position_us, 1_000_000);
        assert_eq!(first.source_in_us, 0);
        assert_eq!(first.source_out_us, 2_000_000); // 2s of source played before the split
        let second = project.clips.iter().find(|c| c.id != "c1").unwrap();
        assert_eq!(second.position_us, 3_000_000);
        assert_eq!(second.source_in_us, 2_000_000);
        assert_eq!(second.source_out_us, 5_000_000);

        let track = project.tracks.iter().find(|t| t.id == "t1").unwrap();
        assert_eq!(track.clip_ids, vec!["c1".to_string(), second.id.clone()]);
    }

    #[test]
    fn split_at_or_beyond_edges_is_rejected() {
        let c = clip("c1", "t1", 0, 0, 5_000_000);
        let project = project_with_clip(c);
        assert!(matches!(
            split_clip(&project, "c1", 0).unwrap_err(),
            TimelineError::InvalidSplitPosition { .. }
        ));
        assert!(matches!(
            split_clip(&project, "c1", 5_000_000).unwrap_err(),
            TimelineError::InvalidSplitPosition { .. }
        ));
        assert!(matches!(
            split_clip(&project, "c1", 10_000_000).unwrap_err(),
            TimelineError::InvalidSplitPosition { .. }
        ));
    }

    #[test]
    fn split_propagates_across_sync_group_with_offset() {
        let mut a = clip("a", "t1", 0, 0, 5_000_000);
        a.group_id = Some("g1".into());
        let mut b = clip("b", "t2", 200_000, 0, 5_000_000);
        b.group_id = Some("g1".into());

        let mut project = ProjectV1::new("sync split");
        let mut t1 = track("t1", TrackKind::Video);
        t1.clip_ids.push("a".into());
        let mut t2 = track("t2", TrackKind::Audio);
        t2.clip_ids.push("b".into());
        project.tracks.push(t1);
        project.tracks.push(t2);
        project.clips.push(a);
        project.clips.push(b);
        project.sync_groups.push(SyncGroup {
            id: "g1".into(),
            clip_ids: vec!["a".into(), "b".into()],
            offsets_us: HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 200_000i64)]),
        });

        // b starts 200ms later than a (offset 200_000). Splitting a at 2s
        // should split b at 2.2s (same real-world instant).
        let cmd = split_clip(&project, "a", 2_000_000).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 4);
        let b_first = project.clips.iter().find(|c| c.id == "b").unwrap();
        assert_eq!(b_first.source_out_us, 2_000_000);
        let b_second = project
            .clips
            .iter()
            .find(|c| c.track_id == "t2" && c.id != "b")
            .unwrap();
        assert_eq!(b_second.position_us, 2_200_000);
        assert_eq!(b_second.source_in_us, 2_000_000);

        let group = project.sync_groups.iter().find(|g| g.id == "g1").unwrap();
        assert_eq!(group.clip_ids.len(), 4);
        assert_eq!(group.offsets_us.get(&b_second.id).copied(), Some(200_000));
    }

    #[test]
    fn split_rejects_locked_track() {
        let c = clip("c1", "t1", 0, 0, 5_000_000);
        let mut project = project_with_clip(c);
        project.tracks[0].locked = true;
        assert!(matches!(
            split_clip(&project, "c1", 1_000_000).unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
    }

    // -- trim ----------------------------------------------------------

    #[test]
    fn trim_start_adjusts_position_and_source_in() {
        let c = clip("c1", "t1", 1_000_000, 500_000, 5_000_000);
        let mut project = project_with_clip(c);
        let cmd = trim_clip_start(&project, "c1", 1_500_000).unwrap();
        apply(&mut project, cmd);
        let c1 = &project.clips[0];
        assert_eq!(c1.position_us, 1_500_000);
        assert_eq!(c1.source_in_us, 1_000_000);
        assert_eq!(c1.source_out_us, 5_000_000); // end edge untouched
    }

    #[test]
    fn trim_end_adjusts_source_out_only() {
        let c = clip("c1", "t1", 1_000_000, 0, 5_000_000);
        let mut project = project_with_clip(c);
        let cmd = trim_clip_end(&project, "c1", 4_000_000).unwrap();
        apply(&mut project, cmd);
        let c1 = &project.clips[0];
        assert_eq!(c1.position_us, 1_000_000); // start edge untouched
                                               // new end 4_000_000 - start 1_000_000 = 3_000_000 timeline duration,
                                               // so source_out_us = source_in_us(0) + 3_000_000.
        assert_eq!(c1.source_out_us, 3_000_000);
    }

    #[test]
    fn trim_start_past_end_is_rejected() {
        let c = clip("c1", "t1", 0, 0, 5_000_000);
        let project = project_with_clip(c);
        assert!(matches!(
            trim_clip_start(&project, "c1", 5_000_000).unwrap_err(),
            TimelineError::InvalidTrimRange { .. }
        ));
    }

    #[test]
    fn trim_end_before_start_is_rejected() {
        let c = clip("c1", "t1", 1_000_000, 0, 5_000_000);
        let project = project_with_clip(c);
        assert!(matches!(
            trim_clip_end(&project, "c1", 1_000_000).unwrap_err(),
            TimelineError::InvalidTrimRange { .. }
        ));
    }

    #[test]
    fn trim_start_beyond_source_in_zero_is_rejected() {
        // source_in_us is already 0; trimming "outward" (earlier start)
        // would require negative source_in, which must be rejected.
        let c = clip("c1", "t1", 1_000_000, 0, 5_000_000);
        let project = project_with_clip(c);
        assert!(matches!(
            trim_clip_start(&project, "c1", 0).unwrap_err(),
            TimelineError::InvalidTrimRange { .. }
        ));
    }

    #[test]
    fn trim_rejects_locked_track() {
        let c = clip("c1", "t1", 0, 0, 5_000_000);
        let mut project = project_with_clip(c);
        project.tracks[0].locked = true;
        assert!(matches!(
            trim_clip_start(&project, "c1", 500_000).unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
        assert!(matches!(
            trim_clip_end(&project, "c1", 4_000_000).unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
    }

    #[test]
    fn trim_rejects_overlap_with_neighbor() {
        let a = clip("a", "t1", 0, 0, 2_000_000);
        let b = clip("b", "t1", 2_000_000, 0, 2_000_000);
        let mut project = project_with_clip(a);
        project.tracks[0].clip_ids.push("b".into());
        project.clips.push(b);

        // Extending a's end past b's start must be rejected as an overlap.
        assert!(matches!(
            trim_clip_end(&project, "a", 3_000_000).unwrap_err(),
            TimelineError::ClipOverlap { .. }
        ));
    }

    // -- move ------------------------------------------------------------

    #[test]
    fn move_changes_track_and_position() {
        let c = clip("c1", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(c);
        project.tracks.push(track("t2", TrackKind::Video));

        let cmd = move_clip(&project, "c1", "t2", 5_000_000).unwrap();
        apply(&mut project, cmd);

        let c1 = &project.clips[0];
        assert_eq!(c1.track_id, "t2");
        assert_eq!(c1.position_us, 5_000_000);
        assert!(project.tracks[0].clip_ids.is_empty());
        assert_eq!(project.tracks[1].clip_ids, vec!["c1".to_string()]);
    }

    #[test]
    fn move_rejects_overlap_on_destination_track() {
        let a = clip("a", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(a);
        project.tracks.push(track("t2", TrackKind::Video));
        let existing = clip("existing", "t2", 0, 0, 1_000_000);
        project.tracks[1].clip_ids.push("existing".into());
        project.clips.push(existing);

        assert!(matches!(
            move_clip(&project, "a", "t2", 500_000).unwrap_err(),
            TimelineError::ClipOverlap { .. }
        ));
    }

    #[test]
    fn move_propagates_delta_to_sync_group_without_changing_member_tracks() {
        let mut a = clip("a", "t1", 1_000_000, 0, 2_000_000);
        a.group_id = Some("g1".into());
        let mut b = clip("b", "t2", 1_000_000, 0, 2_000_000);
        b.group_id = Some("g1".into());

        let mut project = ProjectV1::new("move sync");
        let mut t1 = track("t1", TrackKind::Video);
        t1.clip_ids.push("a".into());
        let mut t2 = track("t2", TrackKind::Audio);
        t2.clip_ids.push("b".into());
        project.tracks.push(t1);
        project.tracks.push(t2);
        project.clips.push(a);
        project.clips.push(b);
        project.sync_groups.push(SyncGroup {
            id: "g1".into(),
            clip_ids: vec!["a".into(), "b".into()],
            offsets_us: HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 0i64)]),
        });

        let cmd = move_clip(&project, "a", "t1", 3_000_000).unwrap();
        apply(&mut project, cmd);

        let b_after = project.clips.iter().find(|c| c.id == "b").unwrap();
        assert_eq!(b_after.track_id, "t2"); // stays on its own track
        assert_eq!(b_after.position_us, 3_000_000); // same +2s delta as a
    }

    // -- delete ----------------------------------------------------------

    #[test]
    fn delete_removes_clip_from_track_and_clips() {
        let c = clip("c1", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(c);
        let cmd = delete_clip(&project, "c1").unwrap();
        apply(&mut project, cmd);
        assert!(project.clips.is_empty());
        assert!(project.tracks[0].clip_ids.is_empty());
    }

    #[test]
    fn delete_cascades_to_sync_group_members() {
        let mut a = clip("a", "t1", 0, 0, 1_000_000);
        a.group_id = Some("g1".into());
        let mut b = clip("b", "t2", 0, 0, 1_000_000);
        b.group_id = Some("g1".into());

        let mut project = ProjectV1::new("delete sync");
        let mut t1 = track("t1", TrackKind::Video);
        t1.clip_ids.push("a".into());
        let mut t2 = track("t2", TrackKind::Audio);
        t2.clip_ids.push("b".into());
        project.tracks.push(t1);
        project.tracks.push(t2);
        project.clips.push(a);
        project.clips.push(b);
        project.sync_groups.push(SyncGroup {
            id: "g1".into(),
            clip_ids: vec!["a".into(), "b".into()],
            offsets_us: HashMap::new(),
        });

        let cmd = delete_clip(&project, "a").unwrap();
        apply(&mut project, cmd);
        assert!(project.clips.is_empty());
        assert!(project.sync_groups.is_empty());
    }

    #[test]
    fn delete_rejects_locked_track() {
        let c = clip("c1", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(c);
        project.tracks[0].locked = true;
        assert!(matches!(
            delete_clip(&project, "c1").unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
    }

    #[test]
    fn delete_clips_batch_removes_all_as_one_command() {
        let a = clip("a", "t1", 0, 0, 1_000_000);
        let b = clip("b", "t1", 1_000_000, 0, 1_000_000);
        let mut project = project_with_clip(a);
        project.tracks[0].clip_ids.push("b".into());
        project.clips.push(b);

        let cmd = delete_clips(&project, &["a".to_string(), "b".to_string()]).unwrap();
        apply(&mut project, cmd);
        assert!(project.clips.is_empty());
    }

    // -- duplicate ---------------------------------------------------------

    #[test]
    fn duplicate_creates_fresh_id_and_does_not_inherit_group() {
        let mut c = clip("c1", "t1", 0, 0, 1_000_000);
        c.group_id = Some("g1".into());
        let mut project = project_with_clip(c);
        project.sync_groups.push(SyncGroup {
            id: "g1".into(),
            clip_ids: vec!["c1".into()],
            offsets_us: HashMap::new(),
        });

        let cmd = duplicate_clip(&project, "c1", 5_000_000, None).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 2);
        let dup = project.clips.iter().find(|c| c.id != "c1").unwrap();
        assert_ne!(dup.id, "c1");
        assert_eq!(dup.position_us, 5_000_000);
        assert_eq!(dup.group_id, None);
    }

    #[test]
    fn duplicate_onto_locked_target_track_is_rejected() {
        let c = clip("c1", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(c);
        project.tracks.push(track("t2", TrackKind::Video));
        project.tracks[1].locked = true;
        assert!(matches!(
            duplicate_clip(&project, "c1", 0, Some("t2")).unwrap_err(),
            TimelineError::TrackLocked { .. }
        ));
    }

    #[test]
    fn duplicate_rejects_overlap() {
        let a = clip("a", "t1", 0, 0, 1_000_000);
        let project = project_with_clip(a);
        assert!(matches!(
            duplicate_clip(&project, "a", 500_000, None).unwrap_err(),
            TimelineError::ClipOverlap { .. }
        ));
    }

    // -- snap ----------------------------------------------------------

    #[test]
    fn snap_returns_nearest_within_threshold() {
        let candidates = [0i64, 1_000_000, 2_000_000];
        assert_eq!(
            snap_to_candidates(1_100_000, &candidates, 200_000),
            Some(1_000_000)
        );
    }

    #[test]
    fn snap_returns_none_when_all_candidates_exceed_threshold() {
        let candidates = [0i64, 5_000_000];
        assert_eq!(snap_to_candidates(2_000_000, &candidates, 100_000), None);
    }

    #[test]
    fn snap_ties_break_toward_smaller_value() {
        // 1_000_000 is equidistant from 900_000 and 1_100_000.
        let candidates = [1_100_000i64, 900_000];
        assert_eq!(
            snap_to_candidates(1_000_000, &candidates, 500_000),
            Some(900_000)
        );
    }

    #[test]
    fn snap_with_no_candidates_returns_none() {
        assert_eq!(snap_to_candidates(0, &[], 1_000_000), None);
    }

    // -- track flags / effective mute --------------------------------------

    #[test]
    fn set_track_locked_flips_flag_via_command() {
        let c = clip("c1", "t1", 0, 0, 1_000_000);
        let mut project = project_with_clip(c);
        let cmd = set_track_locked(&project, "t1", true).unwrap();
        apply(&mut project, cmd);
        assert!(project.tracks[0].locked);
    }

    #[test]
    fn no_solo_means_no_effective_mute_beyond_own_flag() {
        let tracks = vec![track("a1", TrackKind::Audio), {
            let mut t = track("a2", TrackKind::Audio);
            t.muted = true;
            t
        }];
        let state = effective_track_mute_state(&tracks);
        assert!(!state[&"a1".to_string()]);
        assert!(state[&"a2".to_string()]);
    }

    #[test]
    fn solo_mutes_every_other_non_solo_audio_track() {
        let mut solo_track = track("a1", TrackKind::Audio);
        solo_track.solo = true;
        let other = track("a2", TrackKind::Audio);
        let video = track("v1", TrackKind::Video);
        let state = effective_track_mute_state(&[solo_track, other, video]);
        assert!(!state[&"a1".to_string()]); // the solo'd track itself is audible
        assert!(state[&"a2".to_string()]); // non-solo audio muted
        assert!(!state[&"v1".to_string()]); // video tracks unaffected by audio solo
    }

    #[test]
    fn muted_track_stays_muted_even_if_it_is_also_solo() {
        let mut t = track("a1", TrackKind::Audio);
        t.solo = true;
        t.muted = true;
        let state = effective_track_mute_state(&[t]);
        assert!(state[&"a1".to_string()]);
    }
}
