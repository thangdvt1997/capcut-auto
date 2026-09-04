//! Apply Cuts: translates proposed `Remove` `Cut`s (source-media-relative
//! intervals, produced by `vad::cutlist::build_cuts_from_speech_segments`)
//! into the equivalent split+delete sequence on the *real* timeline clips
//! that reference that media — never a second, parallel mutation path
//! (master prompt §12: "generate timeline edits, never touch source media").
//! Every mutation goes through the existing `timeline::ops` primitives
//! (`split_clip`/`trim_clip_start`/`trim_clip_end`/`delete_clip`) and comes
//! back as ONE `Command::Batch`, meant to be applied through
//! `TimelineSession`'s bounded undo `History` as a single undo step per
//! "Apply" action. "Reset" is just `commands::timeline::undo_timeline` — no
//! second undo mechanism.
//!
//! `Cut::start_us`/`end_us` are relative to the *source media file itself*
//! (the same timebase VAD scored via `audio::pcm::extract_pcm`), not any one
//! clip's position on the timeline — a `Cut` comes from analyzing a
//! `MediaItem`'s audio and may need to be applied wherever a clip trims into
//! that media. This module maps each `Remove` interval into the *current*
//! clip's timeline position via `ops::source_delta_to_timeline_delta`, then
//! picks the cheapest valid edit: `trim_clip_start`/`trim_clip_end` when the
//! interval touches one of the clip's own edges, `delete_clip` when it
//! covers the whole remaining clip, or split-split-delete (removing a middle
//! interval) otherwise — exactly the recipe this phase's brief names.
//!
//! `SyncGroup` propagation for `split`/`trim` is not reimplemented here:
//! `split_clip`/`trim_clip_start`/`trim_clip_end` already cascade to every
//! other member of a clip's `SyncGroup` (`timeline::ops` module doc
//! comment), so a silence cut applied to a clip in a synced multi-camera rig
//! automatically splits/trims every linked track too (master prompt §39).
//!
//! Deletion is the one exception, deliberately *not* delegated to
//! `ops::delete_clip`: that primitive's cascade removes **every** member a
//! `SyncGroup` has ever accumulated (by design — "the user deleted one whole
//! synced clip" per its own module doc comment), but a clip that has already
//! been split by an earlier cut in this same Apply pass has *many* group
//! members, most of which are kept footage that must survive. Instead,
//! `remove_clip_and_synced_counterparts` below removes only the one fragment
//! being cut plus each other member's exactly-corresponding fragment
//! (identified via the group's own recorded `offsets_us`, the same delta
//! math `ops::split_clip`'s cascade already uses internally) — still driven
//! entirely by data `timeline::ops` already maintains, just not by calling
//! its blanket-delete primitive.

use std::collections::HashSet;

use crate::project::{Clip, Cut, CutKind, ProjectV1};

use super::command::{BatchCommand, Command, RemoveClipCommand, SetSyncGroupCommand};
use super::error::TimelineError;
use super::ops::{self, clip_span, find_clip, find_sync_group, find_track};

/// Builds the `Command` (always a `Batch`, possibly empty) that applies
/// every `Remove` cut in `cuts` whose `source_media_id` matches `clip_id`'s
/// media to `clip_id`. Non-`Remove` cuts and cuts for a different media id
/// are ignored (defensive — callers may pass a project's whole `cuts` list
/// unfiltered). Returns an empty `Batch` (not an error) if nothing
/// overlapped the clip's current span.
pub fn apply_cuts_to_clip(
    project: &ProjectV1,
    clip_id: &str,
    cuts: &[Cut],
) -> Result<Command, TimelineError> {
    let clip = find_clip(project, clip_id)?;
    let media_id = clip
        .media_id
        .clone()
        .ok_or_else(|| TimelineError::ClipHasNoMedia {
            clip_id: clip_id.to_string(),
        })?;
    let track_id = find_track(project, &clip.track_id)?.id.clone();

    let mut removes: Vec<&Cut> = cuts
        .iter()
        .filter(|c| c.kind == CutKind::Remove && c.source_media_id == media_id)
        .collect();
    removes.sort_by_key(|c| c.start_us);

    let mut scratch = project.clone();
    let mut commands = Vec::new();
    let mut current_clip_id = clip_id.to_string();

    for cut in removes {
        let Ok(current) = find_clip(&scratch, &current_clip_id) else {
            // An earlier (further-left) cut already removed everything that
            // remained of this clip; nothing left for later cuts to apply to.
            break;
        };
        let (start_of_clip, end_of_clip) = clip_span(current);
        let source_start = current.source_in_us;
        let source_end = current.source_out_us;

        let ov_source_start = cut.start_us.max(source_start);
        let ov_source_end = cut.end_us.min(source_end);
        if ov_source_start >= ov_source_end {
            continue; // no overlap with what remains of the clip
        }

        let timeline_start = start_of_clip
            + ops::source_delta_to_timeline_delta(ov_source_start - source_start, current.speed);
        let timeline_end = start_of_clip
            + ops::source_delta_to_timeline_delta(ov_source_end - source_start, current.speed);
        if timeline_end <= timeline_start {
            continue; // degenerate after rounding; nothing to remove
        }

        if timeline_start <= start_of_clip && timeline_end >= end_of_clip {
            // The whole remaining clip is covered by this cut.
            remove_clip_and_synced_counterparts(&mut scratch, &current_clip_id, &mut commands)?;
            break;
        } else if timeline_start <= start_of_clip {
            let cmd = ops::trim_clip_start(&scratch, &current_clip_id, timeline_end)?;
            cmd.apply(&mut scratch)?;
            commands.push(cmd);
            // current_clip_id unchanged: trim keeps the same id.
        } else if timeline_end >= end_of_clip {
            let cmd = ops::trim_clip_end(&scratch, &current_clip_id, timeline_start)?;
            cmd.apply(&mut scratch)?;
            commands.push(cmd);
            break; // clip now ends here; nothing remains to its right
        } else {
            // Interior interval: split at both edges, then delete the
            // middle piece — one atomic multi-step edit.
            let split1 = ops::split_clip(&scratch, &current_clip_id, timeline_start)?;
            split1.apply(&mut scratch)?;
            commands.push(split1);

            let middle_id = clip_starting_at(&scratch, &track_id, timeline_start)
                .ok_or_else(|| TimelineError::ClipNotFound {
                    clip_id: format!("<split tail at {timeline_start}>"),
                })?
                .id
                .clone();

            let split2 = ops::split_clip(&scratch, &middle_id, timeline_end)?;
            split2.apply(&mut scratch)?;
            commands.push(split2);

            let tail_id = clip_starting_at(&scratch, &track_id, timeline_end)
                .ok_or_else(|| TimelineError::ClipNotFound {
                    clip_id: format!("<split tail at {timeline_end}>"),
                })?
                .id
                .clone();

            remove_clip_and_synced_counterparts(&mut scratch, &middle_id, &mut commands)?;

            current_clip_id = tail_id;
        }
    }

    Ok(Command::Batch(BatchCommand { commands }))
}

/// Removes `victim_id` (a specific timeline clip already known to be fully
/// consumed by a `Remove` cut) and, if it belongs to a `SyncGroup`, exactly
/// the corresponding fragment on every other member's track — found via the
/// group's own recorded `offsets_us` delta, NOT via `ops::delete_clip`'s
/// cascade (module doc comment explains why that primitive is unsafe to use
/// here). Also trims the surviving members out of the group's membership
/// (dropping the group entirely if fewer than two members remain), so it
/// never references a clip id that no longer exists.
fn remove_clip_and_synced_counterparts(
    scratch: &mut ProjectV1,
    victim_id: &str,
    commands: &mut Vec<Command>,
) -> Result<(), TimelineError> {
    let victim = find_clip(scratch, victim_id)?.clone();
    let (victim_start, victim_end) = clip_span(&victim);

    let mut to_remove = vec![victim.clone()];
    if let Some(group_id) = &victim.group_id {
        if let Ok(group) = find_sync_group(scratch, group_id) {
            let victim_offset = group.offsets_us.get(victim_id).copied().unwrap_or(0);
            for member_id in group.clip_ids.iter().filter(|id| id.as_str() != victim_id) {
                let Ok(member) = find_clip(scratch, member_id) else {
                    continue;
                };
                let member_offset = group.offsets_us.get(member_id).copied().unwrap_or(0);
                let delta = member_offset - victim_offset;
                let (m_start, m_end) = clip_span(member);
                if m_start == victim_start + delta && m_end == victim_end + delta {
                    to_remove.push(member.clone());
                }
            }
        }
    }

    for clip in &to_remove {
        let cmd = Command::RemoveClip(RemoveClipCommand { clip: clip.clone() });
        cmd.apply(scratch)?;
        commands.push(cmd);
    }

    let removed_ids: HashSet<&str> = to_remove.iter().map(|c| c.id.as_str()).collect();
    let mut touched_groups: HashSet<String> = HashSet::new();
    for clip in &to_remove {
        if let Some(gid) = &clip.group_id {
            touched_groups.insert(gid.clone());
        }
    }
    for gid in touched_groups {
        let Ok(group) = find_sync_group(scratch, &gid) else {
            continue;
        };
        let mut remaining = group.clone();
        remaining
            .clip_ids
            .retain(|id| !removed_ids.contains(id.as_str()));
        remaining
            .offsets_us
            .retain(|id, _| !removed_ids.contains(id.as_str()));
        let cmd = Command::SetSyncGroup(SetSyncGroupCommand {
            group_id: gid,
            old: Some(group.clone()),
            new: if remaining.clip_ids.len() >= 2 {
                Some(remaining)
            } else {
                None // fewer than two members left: nothing left to keep synced
            },
        });
        cmd.apply(scratch)?;
        commands.push(cmd);
    }
    Ok(())
}

/// Convenience wrapper over `apply_cuts_to_clip` for every clip currently on
/// `track_id` (in position order), concatenated into one outer `Batch` — the
/// "target track" half of this phase's "target track/clip" requirement.
/// Clips with no `media_id` (e.g. a pure caption/effect clip) are silently
/// skipped rather than erroring the whole track.
pub fn apply_cuts_to_track(
    project: &ProjectV1,
    track_id: &str,
    cuts: &[Cut],
) -> Result<Command, TimelineError> {
    let track = find_track(project, track_id)?;
    let mut scratch = project.clone();
    let mut commands = Vec::new();
    for clip_id in track.clip_ids.clone() {
        let Ok(clip) = find_clip(&scratch, &clip_id) else {
            continue; // already removed by cascading from an earlier iteration
        };
        if clip.media_id.is_none() {
            continue;
        }
        let cmd = apply_cuts_to_clip(&scratch, &clip_id, cuts)?;
        cmd.apply(&mut scratch)?;
        commands.push(cmd);
    }
    Ok(Command::Batch(BatchCommand { commands }))
}

fn clip_starting_at<'a>(
    project: &'a ProjectV1,
    track_id: &str,
    position_us: i64,
) -> Option<&'a Clip> {
    project
        .clips
        .iter()
        .find(|c| c.track_id == track_id && c.position_us == position_us)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ClipSettings, Track, TrackKind};
    use std::collections::HashMap;

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

    fn clip_with_media(id: &str, track_id: &str, position_us: i64, source_out: i64) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: Some("m1".into()),
            source_in_us: 0,
            source_out_us: source_out,
            position_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    fn remove_cut(start_us: i64, end_us: i64) -> Cut {
        Cut {
            id: uuid::Uuid::new_v4().to_string(),
            kind: CutKind::Remove,
            source_media_id: "m1".into(),
            start_us,
            end_us,
            reason: crate::project::CutReason::Silence,
            applied: false,
        }
    }

    fn project_with_one_clip(source_out: i64) -> ProjectV1 {
        let mut p = ProjectV1::new("silence test");
        let mut t = track("t1");
        t.clip_ids.push("c1".into());
        p.tracks.push(t);
        p.clips.push(clip_with_media("c1", "t1", 0, source_out));
        p
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    #[test]
    fn interior_cut_splits_and_deletes_the_middle_piece_as_one_batch() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [remove_cut(3_000_000, 5_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 2);
        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(spans, vec![(0, 3_000_000), (5_000_000, 10_000_000)]);
    }

    #[test]
    fn cut_touching_start_edge_trims_instead_of_splitting() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [remove_cut(0, 2_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 1);
        assert_eq!(clip_span(&project.clips[0]), (2_000_000, 10_000_000));
        assert_eq!(project.clips[0].id, "c1"); // trim keeps the same id
    }

    #[test]
    fn cut_touching_end_edge_trims_instead_of_splitting() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [remove_cut(8_000_000, 10_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 1);
        assert_eq!(clip_span(&project.clips[0]), (0, 8_000_000));
    }

    #[test]
    fn cut_covering_the_whole_clip_deletes_it() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [remove_cut(0, 10_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);
        assert!(project.clips.is_empty());
    }

    #[test]
    fn cut_covering_more_than_the_clip_still_deletes_it_cleanly() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [remove_cut(-5_000_000, 50_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);
        assert!(project.clips.is_empty());
    }

    #[test]
    fn multiple_interior_cuts_apply_left_to_right_against_the_shrinking_clip() {
        let mut project = project_with_one_clip(10_000_000);
        let cuts = [
            remove_cut(2_000_000, 3_000_000),
            remove_cut(6_000_000, 7_000_000),
        ];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        apply(&mut project, cmd);

        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(
            spans,
            vec![
                (0, 2_000_000),
                (3_000_000, 6_000_000),
                (7_000_000, 10_000_000)
            ]
        );
    }

    #[test]
    fn non_overlapping_cut_produces_an_empty_batch_and_no_change() {
        let project = project_with_one_clip(10_000_000);
        let before = serde_json::to_value(&project).unwrap();
        let cuts = [remove_cut(20_000_000, 30_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();
        let mut after = project.clone();
        apply(&mut after, cmd);
        assert_eq!(serde_json::to_value(&after).unwrap(), before);
    }

    #[test]
    fn cuts_for_a_different_media_id_are_ignored() {
        let project = project_with_one_clip(10_000_000);
        let mut other_media_cut = remove_cut(3_000_000, 5_000_000);
        other_media_cut.source_media_id = "m2".into();
        let cmd = apply_cuts_to_clip(&project, "c1", &[other_media_cut]).unwrap();
        let mut after = project.clone();
        apply(&mut after, cmd);
        assert_eq!(after.clips.len(), 1);
        assert_eq!(clip_span(&after.clips[0]), (0, 10_000_000));
    }

    #[test]
    fn clip_with_no_media_is_rejected() {
        let mut project = project_with_one_clip(10_000_000);
        project.clips[0].media_id = None;
        let cuts = [remove_cut(3_000_000, 5_000_000)];
        assert!(matches!(
            apply_cuts_to_clip(&project, "c1", &cuts).unwrap_err(),
            TimelineError::ClipHasNoMedia { .. }
        ));
    }

    #[test]
    fn undo_after_apply_restores_the_original_single_clip() {
        use crate::timeline::command::History;

        let mut project = project_with_one_clip(10_000_000);
        let before = serde_json::to_value(&project).unwrap();
        let cuts = [remove_cut(3_000_000, 5_000_000)];
        let cmd = apply_cuts_to_clip(&project, "c1", &cuts).unwrap();

        let mut history = History::new(crate::timeline::command::MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.clips.len(), 2);

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }

    #[test]
    fn sync_group_member_is_cut_too_via_existing_propagation() {
        use crate::project::SyncGroup;

        let mut project = ProjectV1::new("silence sync test");
        let mut t1 = track("t1");
        t1.clip_ids.push("a".into());
        let mut t2 = track("t2");
        t2.clip_ids.push("b".into());
        project.tracks.push(t1);
        project.tracks.push(t2);

        let mut a = clip_with_media("a", "t1", 0, 10_000_000);
        a.group_id = Some("g1".into());
        let mut b = clip_with_media("b", "t2", 0, 10_000_000);
        b.group_id = Some("g1".into());
        project.clips.push(a);
        project.clips.push(b);
        project.sync_groups.push(SyncGroup {
            id: "g1".into(),
            clip_ids: vec!["a".into(), "b".into()],
            offsets_us: HashMap::from([("a".to_string(), 0i64), ("b".to_string(), 0i64)]),
        });

        let cuts = [remove_cut(3_000_000, 5_000_000)];
        let cmd = apply_cuts_to_clip(&project, "a", &cuts).unwrap();
        apply(&mut project, cmd);

        // "a" was split+deleted in the middle; its SyncGroup partner "b"
        // must have cascaded the same way via `ops::split_clip`'s own
        // propagation (this module doesn't reimplement it).
        assert_eq!(project.clips.len(), 4);
        let b_spans: Vec<(i64, i64)> = project
            .clips
            .iter()
            .filter(|c| c.track_id == "t2")
            .map(clip_span)
            .collect();
        let mut sorted = b_spans.clone();
        sorted.sort();
        assert_eq!(sorted, vec![(0, 3_000_000), (5_000_000, 10_000_000)]);
    }

    #[test]
    fn apply_cuts_to_track_covers_every_clip_on_the_track() {
        let mut project = ProjectV1::new("track apply test");
        let mut t = track("t1");
        t.clip_ids.push("a".into());
        t.clip_ids.push("b".into());
        project.tracks.push(t);
        project.clips.push(clip_with_media("a", "t1", 0, 2_000_000));
        project
            .clips
            .push(clip_with_media("b", "t1", 5_000_000, 2_000_000));

        // Cut removes [0, 1s) of source, which overlaps the start of both
        // clips (both trim into source 0..2s at different timeline
        // positions), trimming both clips' starts.
        let cuts = [remove_cut(0, 1_000_000)];
        let cmd = apply_cuts_to_track(&project, "t1", &cuts).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 2);
        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(spans, vec![(1_000_000, 2_000_000), (6_000_000, 7_000_000)]);
    }
}
