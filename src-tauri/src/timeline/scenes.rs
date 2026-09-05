//! Scene-based timeline operations (master prompt §25's "split at scenes" /
//! "remove scenes"): thin translation layers over `timeline::ops`/
//! `timeline::silence`'s existing primitives — no new mutation path, per
//! this module's `timeline::silence` sibling's own documented discipline.
//!
//! "Select scenes" (master prompt §25) is a frontend selection-state
//! concern, not a backend operation — nothing here implements it, per this
//! phase's task brief.

use crate::media::scene::Scene;
use crate::project::{Cut, CutKind, CutReason, ProjectV1};

use super::command::Command;
use super::error::TimelineError;
use super::ops::{self, clip_span, find_clip};
use super::silence;

/// Turns detected `Scene`s into `Remove` `Cut`s against `media_id` —
/// "remove scenes" is structurally identical to a silence/filler-word
/// removal (a time range to cut), so this just adapts the shape rather than
/// reimplementing removal logic (this phase's task brief). `Scene::start_us`/
/// `end_us` are already source-media-relative (`media::scene::detect_scenes`
/// doc comment), the exact same timebase convention `Cut::start_us`/`end_us`
/// uses, so no conversion is needed here at all — just a field-for-field
/// reshaping.
pub fn cuts_from_scenes(scenes: &[Scene], media_id: &str) -> Vec<Cut> {
    scenes
        .iter()
        .map(|s| Cut {
            id: uuid::Uuid::new_v4().to_string(),
            kind: CutKind::Remove,
            source_media_id: media_id.to_string(),
            start_us: s.start_us,
            end_us: s.end_us,
            reason: CutReason::AiSuggested,
            applied: false,
        })
        .collect()
}

/// Builds a `Command` (`Batch`, possibly empty) that removes every given
/// scene's span from `clip_id` — a thin wrapper handing off to
/// `timeline::silence::apply_cuts_to_clip` (`cuts_from_scenes` doc comment).
pub fn remove_scenes_from_clip(
    project: &ProjectV1,
    clip_id: &str,
    scenes: &[Scene],
    media_id: &str,
) -> Result<Command, TimelineError> {
    let cuts = cuts_from_scenes(scenes, media_id);
    silence::apply_cuts_to_clip(project, clip_id, &cuts)
}

/// Same as [`remove_scenes_from_clip`], but for every clip on `track_id`
/// (`timeline::silence::apply_cuts_to_track`'s own "target track" shape).
pub fn remove_scenes_from_track(
    project: &ProjectV1,
    track_id: &str,
    scenes: &[Scene],
    media_id: &str,
) -> Result<Command, TimelineError> {
    let cuts = cuts_from_scenes(scenes, media_id);
    silence::apply_cuts_to_track(project, track_id, &cuts)
}

/// Splits `clip_id` at every scene boundary in `scene_boundaries_us` that
/// falls strictly inside the clip's current span, reusing
/// `timeline::ops::split_clip` once per boundary (never reimplementing split
/// logic, this phase's task brief). Boundaries are source-media-relative
/// (same convention `Scene::start_us` and `Cut::start_us` share), converted
/// to the clip's current on-timeline position via the same
/// `ops::source_delta_to_timeline_delta` math `timeline::silence` already
/// relies on, and applied left-to-right against the ever-shrinking-from-
/// the-left original clip — mirroring `timeline::silence::apply_cuts_to_clip`'s
/// own walk.
pub fn split_clip_at_scenes(
    project: &ProjectV1,
    clip_id: &str,
    scene_boundaries_us: &[i64],
) -> Result<Command, TimelineError> {
    let mut boundaries = scene_boundaries_us.to_vec();
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut scratch = project.clone();
    let mut commands = Vec::new();
    let mut current_clip_id = clip_id.to_string();

    for boundary in boundaries {
        let Ok(current) = find_clip(&scratch, &current_clip_id) else {
            break; // an earlier split already consumed everything remaining
        };
        let source_start = current.source_in_us;
        let (clip_start, clip_end) = clip_span(current);
        let timeline_boundary = clip_start
            + ops::source_delta_to_timeline_delta(boundary - source_start, current.speed);

        if timeline_boundary <= clip_start || timeline_boundary >= clip_end {
            continue; // boundary doesn't fall strictly inside what remains
        }

        let track_id = current.track_id.clone();
        let cmd = ops::split_clip(&scratch, &current_clip_id, timeline_boundary)?;
        cmd.apply(&mut scratch)?;
        commands.push(cmd);

        // The tail half of this split is what later (larger) boundaries
        // should keep splitting.
        if let Some(tail) = scratch
            .clips
            .iter()
            .find(|c| c.track_id == track_id && c.position_us == timeline_boundary)
        {
            current_clip_id = tail.id.clone();
        }
    }

    Ok(Command::Batch(super::command::BatchCommand { commands }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Clip, ClipSettings, ProjectV1, Track, TrackKind};

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

    fn project_with_one_clip(source_out: i64) -> ProjectV1 {
        let mut p = ProjectV1::new("scene ops test");
        let mut t = track("t1");
        t.clip_ids.push("c1".into());
        p.tracks.push(t);
        p.clips.push(clip_with_media("c1", "t1", 0, source_out));
        p
    }

    fn scene(start_us: i64, end_us: i64, score: f32) -> Scene {
        Scene {
            id: uuid::Uuid::new_v4().to_string(),
            start_us,
            end_us,
            thumbnail_path: None,
            score,
        }
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    #[test]
    fn cuts_from_scenes_maps_scene_spans_directly() {
        let scenes = vec![scene(0, 3_000_000, 0.0), scene(3_000_000, 6_000_000, 0.7)];
        let cuts = cuts_from_scenes(&scenes, "m1");
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].source_media_id, "m1");
        assert_eq!(cuts[0].kind, CutKind::Remove);
        assert_eq!((cuts[1].start_us, cuts[1].end_us), (3_000_000, 6_000_000));
    }

    #[test]
    fn split_clip_at_scenes_splits_at_every_boundary_inside_the_clip() {
        let mut project = project_with_one_clip(10_000_000);
        let cmd = split_clip_at_scenes(&project, "c1", &[3_000_000, 6_000_000]).unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 3);
        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(
            spans,
            vec![
                (0, 3_000_000),
                (3_000_000, 6_000_000),
                (6_000_000, 10_000_000)
            ]
        );
    }

    #[test]
    fn split_clip_at_scenes_ignores_boundaries_outside_the_clip() {
        let mut project = project_with_one_clip(5_000_000);
        let cmd = split_clip_at_scenes(&project, "c1", &[0, 5_000_000, 20_000_000]).unwrap();
        apply(&mut project, cmd);
        // 0 and 5_000_000 are the clip's own edges (not strictly inside);
        // 20_000_000 is past the clip entirely -> no splits happen at all.
        assert_eq!(project.clips.len(), 1);
    }

    #[test]
    fn remove_scenes_from_clip_cuts_the_scenes_span() {
        let mut project = project_with_one_clip(10_000_000);
        let scenes = vec![scene(3_000_000, 5_000_000, 0.5)];
        let cmd = remove_scenes_from_clip(&project, "c1", &scenes, "m1").unwrap();
        apply(&mut project, cmd);

        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(spans, vec![(0, 3_000_000), (5_000_000, 10_000_000)]);
    }

    #[test]
    fn remove_scenes_from_track_covers_every_clip_on_the_track() {
        let mut project = ProjectV1::new("scene track test");
        let mut t = track("t1");
        t.clip_ids.push("a".into());
        t.clip_ids.push("b".into());
        project.tracks.push(t);
        project.clips.push(clip_with_media("a", "t1", 0, 2_000_000));
        project
            .clips
            .push(clip_with_media("b", "t1", 5_000_000, 2_000_000));

        let scenes = vec![scene(0, 1_000_000, 0.5)];
        let cmd = remove_scenes_from_track(&project, "t1", &scenes, "m1").unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.clips.len(), 2);
        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(spans, vec![(1_000_000, 2_000_000), (6_000_000, 7_000_000)]);
    }
}
