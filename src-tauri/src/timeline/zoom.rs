//! Applies auto-zoom's generated `Keyframe`s (`crate::zoom`, master prompt
//! §24) to a real clip's timeline data — the one piece of "authoring"
//! wiring `project::types::Keyframe`'s own doc comment says doesn't exist
//! yet ("no authoring UI produces them yet until now"). Goes through
//! `timeline::command::Command::SetKeyframes`, the same whole-list-swap
//! primitive `timeline::silence`'s `Cut`s use — no new mutation path.

use crate::project::{Keyframe, ProjectV1};

use super::command::{Command, SetKeyframesCommand};
use super::error::TimelineError;
use super::ops::find_clip;

/// Replaces `clip_id`'s existing `"scale"`-property keyframes (if any) with
/// `new_keyframes` — re-running auto-zoom for a clip overwrites its previous
/// zoom keyframes rather than stacking a second, redundant set on top
/// (`crate::zoom` module doc comment's "avoid excessive zoom"). Keyframes
/// for *other* properties (position/rotation/alpha/volume) or *other* clips
/// are left untouched.
pub fn apply_zoom_keyframes_to_clip(
    project: &ProjectV1,
    clip_id: &str,
    new_keyframes: Vec<Keyframe>,
) -> Result<Command, TimelineError> {
    find_clip(project, clip_id)?; // validates the clip exists before mutating

    let mut keyframes = project.keyframes.clone();
    keyframes.retain(|k| !(k.clip_id == clip_id && k.property == "scale"));
    keyframes.extend(new_keyframes);

    Ok(Command::SetKeyframes(SetKeyframesCommand {
        old: project.keyframes.clone(),
        new: keyframes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Clip, ClipSettings, Track, TrackKind};

    fn project_with_clip() -> ProjectV1 {
        let mut p = ProjectV1::new("zoom apply test");
        p.tracks.push(Track {
            id: "t1".into(),
            kind: TrackKind::Video,
            name: "t1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(Clip {
            id: "c1".into(),
            track_id: "t1".into(),
            media_id: None,
            source_in_us: 0,
            source_out_us: 10_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        p
    }

    fn keyframe(clip_id: &str, property: &str, time_offset_us: i64, value: f64) -> Keyframe {
        Keyframe {
            id: uuid::Uuid::new_v4().to_string(),
            clip_id: clip_id.to_string(),
            property: property.to_string(),
            time_offset_us,
            value,
            curve: "linear".to_string(),
        }
    }

    fn apply(project: &mut ProjectV1, command: Command) {
        command.apply(project).expect("apply should succeed");
    }

    #[test]
    fn inserts_new_scale_keyframes_for_the_clip() {
        let mut project = project_with_clip();
        let new_kfs = vec![
            keyframe("c1", "scale", 0, 1.0),
            keyframe("c1", "scale", 500_000, 1.08),
        ];
        let cmd = apply_zoom_keyframes_to_clip(&project, "c1", new_kfs).unwrap();
        apply(&mut project, cmd);
        assert_eq!(project.keyframes.len(), 2);
    }

    #[test]
    fn replaces_the_clips_own_previous_scale_keyframes_only() {
        let mut project = project_with_clip();
        project.keyframes.push(keyframe("c1", "scale", 0, 1.0));
        project.keyframes.push(keyframe("c1", "position_x", 0, 0.2)); // different property, must survive

        let cmd =
            apply_zoom_keyframes_to_clip(&project, "c1", vec![keyframe("c1", "scale", 0, 1.15)])
                .unwrap();
        apply(&mut project, cmd);

        assert_eq!(project.keyframes.len(), 2);
        let scale_values: Vec<f64> = project
            .keyframes
            .iter()
            .filter(|k| k.property == "scale")
            .map(|k| k.value)
            .collect();
        assert_eq!(scale_values, vec![1.15]);
        assert!(project.keyframes.iter().any(|k| k.property == "position_x"));
    }

    #[test]
    fn errors_on_a_missing_clip() {
        let project = project_with_clip();
        assert!(matches!(
            apply_zoom_keyframes_to_clip(&project, "does-not-exist", vec![]).unwrap_err(),
            TimelineError::ClipNotFound { .. }
        ));
    }

    #[test]
    fn undo_restores_the_previous_keyframe_set() {
        use crate::timeline::command::History;

        let mut project = project_with_clip();
        project.keyframes.push(keyframe("c1", "scale", 0, 1.0));
        let before = serde_json::to_value(&project).unwrap();

        let cmd =
            apply_zoom_keyframes_to_clip(&project, "c1", vec![keyframe("c1", "scale", 0, 1.15)])
                .unwrap();
        let mut history = History::new(crate::timeline::command::MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.keyframes[0].value, 1.15);

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }
}
