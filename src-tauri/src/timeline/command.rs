//! Command-based undo/redo (master prompt §11) — deliberately NOT
//! whole-project snapshot/copy undo. Every entry in `History` stores only
//! the small per-command delta needed to reverse that one edit (a clip's
//! before/after state, a track's before/after flags, a sync group's
//! before/after membership) — never a clone of the entire `ProjectV1`.
//!
//! Design: five primitive commands (`InsertClip`, `RemoveClip`, `SetClip`,
//! `SetTrack`, `SetSyncGroup`) are the only things that ever mutate a
//! project. Every higher-level operation in `timeline::ops`/`timeline::clipboard`
//! (split/trim/move/delete/duplicate/copy/paste, and sync-group propagation)
//! is expressed as one primitive or a `Batch` of them. This is also what
//! keeps the design open for Phase 8's `AddCaptionCommand` etc. without a
//! redesign: a caption just needs its own `InsertCaption`/`RemoveCaption`/
//! `SetCaption` primitives plumbed into the same `Command` enum and the same
//! `Batch`/`History` machinery.
//!
//! `SetClip`/`SetTrack`/`SetSyncGroup` invert trivially by swapping their
//! `old`/`new` fields; `InsertClip`/`RemoveClip` invert into each other;
//! `Batch` inverts by reversing its list and inverting each member. None of
//! this needs to look at the project to compute an inverse — every command
//! already carries everything required, which is what makes `invert(&self)`
//! a pure, cheap, project-independent function.

use std::collections::{HashMap, VecDeque};

use crate::project::{Caption, Clip, Cut, ProjectV1, SyncGroup, Track};

use super::error::TimelineError;

/// Bounded undo history depth (master prompt §11 "bounded history"). Chosen
/// as a reasonable default for an editing session; oldest entries are
/// dropped once exceeded (`History::apply`/`History::redo`).
pub const MAX_HISTORY: usize = 100;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InsertClipCommand {
    pub clip: Clip,
}

#[derive(Debug, Clone)]
pub struct RemoveClipCommand {
    /// Full pre-image of the clip being removed — required so `invert()`
    /// can reconstruct it via `InsertClip` without touching the project.
    pub clip: Clip,
}

#[derive(Debug, Clone)]
pub struct SetClipCommand {
    pub old: Clip,
    pub new: Clip,
}

#[derive(Debug, Clone)]
pub struct SetTrackCommand {
    pub old: Track,
    pub new: Track,
}

#[derive(Debug, Clone)]
pub struct SetSyncGroupCommand {
    pub group_id: String,
    /// `None` means "the group did not exist before/after this command" —
    /// lets one primitive express create/update/delete of a `SyncGroup`.
    pub old: Option<SyncGroup>,
    pub new: Option<SyncGroup>,
}

/// Whole-list swap of `ProjectV1::cuts`, mirroring `SetTrack`'s
/// whole-object-swap style rather than adding per-`Cut` Insert/Remove
/// primitives — the cuts list is provenance/audit metadata (`project::types`
/// module doc comment), not a set of independently-addressed timeline
/// entities, so one before/after snapshot of the field is the simplest
/// correct primitive. Used by `timeline::silence::apply_cuts_to_clip` to
/// mark the applied cuts `applied: true` as part of the *same* atomic
/// `Batch` as the clip split/delete edits it produces.
#[derive(Debug, Clone)]
pub struct SetCutsCommand {
    pub old: Vec<Cut>,
    pub new: Vec<Cut>,
}

/// `Caption`'s equivalent of `InsertClipCommand`/`RemoveClipCommand`/
/// `SetClipCommand` above — per-entity addressed (not a whole-list swap like
/// `SetCutsCommand`), because individual captions are independently split/
/// merged/retimed much like clips are individually split/trimmed
/// (`timeline::captions` module doc comment).
#[derive(Debug, Clone)]
pub struct InsertCaptionCommand {
    pub caption: Caption,
}

#[derive(Debug, Clone)]
pub struct RemoveCaptionCommand {
    /// Full pre-image, same reason as `RemoveClipCommand::clip`.
    pub caption: Caption,
}

#[derive(Debug, Clone)]
pub struct SetCaptionCommand {
    pub old: Caption,
    pub new: Caption,
}

#[derive(Debug, Clone, Default)]
pub struct BatchCommand {
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone)]
pub enum Command {
    InsertClip(InsertClipCommand),
    RemoveClip(RemoveClipCommand),
    SetClip(SetClipCommand),
    SetTrack(SetTrackCommand),
    SetSyncGroup(SetSyncGroupCommand),
    SetCuts(SetCutsCommand),
    InsertCaption(InsertCaptionCommand),
    RemoveCaption(RemoveCaptionCommand),
    SetCaption(SetCaptionCommand),
    Batch(BatchCommand),
}

/// Keeps `ProjectV1::captions` in a deterministic `(track_id, start_us)`
/// order after every insert/set — captions have no separate ordered-id list
/// the way `Track::clip_ids` does for clips, so the flat `Vec` itself is
/// sorted directly. Purely a determinism/readability convenience for
/// consumers that iterate `project.captions` in display order; nothing in
/// this module depends on the order for correctness.
fn resort_captions(project: &mut ProjectV1) {
    project
        .captions
        .sort_by(|a, b| (&a.track_id, a.start_us).cmp(&(&b.track_id, b.start_us)));
}

fn resort_track_clip_ids(project: &mut ProjectV1, track_id: &str) {
    let positions: HashMap<String, i64> = project
        .clips
        .iter()
        .map(|c| (c.id.clone(), c.position_us))
        .collect();
    if let Some(track) = project.tracks.iter_mut().find(|t| t.id == track_id) {
        track.clip_ids.sort_by(|a, b| {
            let pa = positions.get(a).copied().unwrap_or(i64::MAX);
            let pb = positions.get(b).copied().unwrap_or(i64::MAX);
            pa.cmp(&pb).then_with(|| a.cmp(b))
        });
    }
}

impl Command {
    /// Applies this command to `project`. Deterministic given the project is
    /// in the exact state this command expects (guaranteed by construction:
    /// every `timeline::ops`/`timeline::clipboard` builder reads the project
    /// once to capture pre-images, so first-apply and any later redo-apply
    /// against an undone project produce byte-identical results).
    pub fn apply(&self, project: &mut ProjectV1) -> Result<(), TimelineError> {
        match self {
            Command::InsertClip(c) => {
                project.clips.push(c.clip.clone());
                if let Some(track) = project.tracks.iter_mut().find(|t| t.id == c.clip.track_id) {
                    if !track.clip_ids.contains(&c.clip.id) {
                        track.clip_ids.push(c.clip.id.clone());
                    }
                }
                resort_track_clip_ids(project, &c.clip.track_id);
                Ok(())
            }
            Command::RemoveClip(c) => {
                let clip_id = &c.clip.id;
                project.clips.retain(|existing| &existing.id != clip_id);
                if let Some(track) = project.tracks.iter_mut().find(|t| t.id == c.clip.track_id) {
                    track.clip_ids.retain(|id| id != clip_id);
                }
                Ok(())
            }
            Command::SetClip(c) => {
                let idx = project
                    .clips
                    .iter()
                    .position(|existing| existing.id == c.new.id)
                    .ok_or_else(|| TimelineError::ClipNotFound {
                        clip_id: c.new.id.clone(),
                    })?;
                let old_track_id = project.clips[idx].track_id.clone();
                project.clips[idx] = c.new.clone();
                if old_track_id != c.new.track_id {
                    if let Some(t) = project.tracks.iter_mut().find(|t| t.id == old_track_id) {
                        t.clip_ids.retain(|id| id != &c.new.id);
                    }
                    if let Some(t) = project.tracks.iter_mut().find(|t| t.id == c.new.track_id) {
                        if !t.clip_ids.contains(&c.new.id) {
                            t.clip_ids.push(c.new.id.clone());
                        }
                    }
                }
                resort_track_clip_ids(project, &c.new.track_id);
                Ok(())
            }
            Command::SetTrack(c) => {
                let idx = project
                    .tracks
                    .iter()
                    .position(|existing| existing.id == c.new.id)
                    .ok_or_else(|| TimelineError::TrackNotFound {
                        track_id: c.new.id.clone(),
                    })?;
                project.tracks[idx] = c.new.clone();
                Ok(())
            }
            Command::SetSyncGroup(c) => {
                project.sync_groups.retain(|g| g.id != c.group_id);
                if let Some(new) = &c.new {
                    project.sync_groups.push(new.clone());
                }
                Ok(())
            }
            Command::SetCuts(c) => {
                project.cuts = c.new.clone();
                Ok(())
            }
            Command::InsertCaption(c) => {
                project.captions.push(c.caption.clone());
                resort_captions(project);
                Ok(())
            }
            Command::RemoveCaption(c) => {
                let caption_id = &c.caption.id;
                project
                    .captions
                    .retain(|existing| &existing.id != caption_id);
                Ok(())
            }
            Command::SetCaption(c) => {
                let idx = project
                    .captions
                    .iter()
                    .position(|existing| existing.id == c.new.id)
                    .ok_or_else(|| TimelineError::CaptionNotFound {
                        caption_id: c.new.id.clone(),
                    })?;
                project.captions[idx] = c.new.clone();
                resort_captions(project);
                Ok(())
            }
            Command::Batch(b) => b.apply(project),
        }
    }

    /// Constructs the exact inverse of this command — a pure function of
    /// `self`, no project access needed (see module doc comment).
    pub fn invert(&self) -> Command {
        match self {
            Command::InsertClip(c) => Command::RemoveClip(RemoveClipCommand {
                clip: c.clip.clone(),
            }),
            Command::RemoveClip(c) => Command::InsertClip(InsertClipCommand {
                clip: c.clip.clone(),
            }),
            Command::SetClip(c) => Command::SetClip(SetClipCommand {
                old: c.new.clone(),
                new: c.old.clone(),
            }),
            Command::SetTrack(c) => Command::SetTrack(SetTrackCommand {
                old: c.new.clone(),
                new: c.old.clone(),
            }),
            Command::SetSyncGroup(c) => Command::SetSyncGroup(SetSyncGroupCommand {
                group_id: c.group_id.clone(),
                old: c.new.clone(),
                new: c.old.clone(),
            }),
            Command::SetCuts(c) => Command::SetCuts(SetCutsCommand {
                old: c.new.clone(),
                new: c.old.clone(),
            }),
            Command::InsertCaption(c) => Command::RemoveCaption(RemoveCaptionCommand {
                caption: c.caption.clone(),
            }),
            Command::RemoveCaption(c) => Command::InsertCaption(InsertCaptionCommand {
                caption: c.caption.clone(),
            }),
            Command::SetCaption(c) => Command::SetCaption(SetCaptionCommand {
                old: c.new.clone(),
                new: c.old.clone(),
            }),
            Command::Batch(b) => Command::Batch(BatchCommand {
                commands: b.commands.iter().rev().map(Command::invert).collect(),
            }),
        }
    }
}

impl BatchCommand {
    /// Applies every sub-command atomically: runs them against a scratch
    /// clone of the project and only commits (`*project = scratch`) if every
    /// one of them succeeds, so a mid-batch failure never leaves `project`
    /// partially edited. This clone is a **transient implementation detail**
    /// of applying one already-cheap batch, not the undo storage mechanism —
    /// `History` never stores project clones, only `Command` values (see
    /// module doc comment).
    fn apply(&self, project: &mut ProjectV1) -> Result<(), TimelineError> {
        let mut scratch = project.clone();
        for command in &self.commands {
            command.apply(&mut scratch)?;
        }
        *project = scratch;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HistoryEntry {
    forward: Command,
    inverse: Command,
}

/// Bounded undo/redo stack (master prompt §11). Applying a new command
/// clears the redo stack (standard editor semantics: you can't redo past a
/// fresh edit). Capacity is enforced on both `apply` and `redo` so the undo
/// stack never exceeds `MAX_HISTORY` entries regardless of how it grew.
#[derive(Debug)]
pub struct History {
    cap: usize,
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn apply(
        &mut self,
        project: &mut ProjectV1,
        command: Command,
    ) -> Result<(), TimelineError> {
        command.apply(project)?;
        let inverse = command.invert();
        self.redo_stack.clear();
        self.undo_stack.push_back(HistoryEntry {
            forward: command,
            inverse,
        });
        while self.undo_stack.len() > self.cap {
            self.undo_stack.pop_front();
        }
        Ok(())
    }

    pub fn undo(&mut self, project: &mut ProjectV1) -> Result<(), TimelineError> {
        let entry = self
            .undo_stack
            .pop_back()
            .ok_or(TimelineError::NothingToUndo)?;
        entry.inverse.apply(project)?;
        self.redo_stack.push(entry);
        Ok(())
    }

    pub fn redo(&mut self, project: &mut ProjectV1) -> Result<(), TimelineError> {
        let entry = self.redo_stack.pop().ok_or(TimelineError::NothingToRedo)?;
        entry.forward.apply(project)?;
        self.undo_stack.push_back(entry);
        while self.undo_stack.len() > self.cap {
            self.undo_stack.pop_front();
        }
        Ok(())
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ClipSettings, TrackKind};

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

    fn project_with_one_clip() -> ProjectV1 {
        let mut p = ProjectV1::new("History test");
        p.tracks.push(track("t1"));
        Command::InsertClip(InsertClipCommand {
            clip: clip("c1", "t1", 0),
        })
        .apply(&mut p)
        .unwrap();
        p
    }

    #[test]
    fn insert_and_remove_are_exact_inverses() {
        let mut project = ProjectV1::new("test");
        project.tracks.push(track("t1"));
        let before = serde_json::to_value(&project).unwrap();

        let cmd = Command::InsertClip(InsertClipCommand {
            clip: clip("c1", "t1", 0),
        });
        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.tracks[0].clip_ids, vec!["c1".to_string()]);

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }

    #[test]
    fn set_clip_undo_redo_round_trips() {
        let mut project = project_with_one_clip();
        let before = serde_json::to_value(&project).unwrap();

        let old = project.clips[0].clone();
        let mut new = old.clone();
        new.position_us = 500_000;
        let cmd = Command::SetClip(SetClipCommand { old, new });

        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.clips[0].position_us, 500_000);

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);

        history.redo(&mut project).unwrap();
        assert_eq!(project.clips[0].position_us, 500_000);
    }

    #[test]
    fn batch_undo_redo_round_trips_and_is_atomic() {
        let mut project = project_with_one_clip();
        let before = serde_json::to_value(&project).unwrap();

        let batch = Command::Batch(BatchCommand {
            commands: vec![
                Command::InsertClip(InsertClipCommand {
                    clip: clip("c2", "t1", 2_000_000),
                }),
                Command::RemoveClip(RemoveClipCommand {
                    clip: clip("c1", "t1", 0),
                }),
            ],
        });
        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, batch).unwrap();
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.clips[0].id, "c2");

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);

        history.redo(&mut project).unwrap();
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.clips[0].id, "c2");
    }

    #[test]
    fn batch_failure_leaves_project_untouched() {
        let mut project = project_with_one_clip();
        let before = serde_json::to_value(&project).unwrap();

        // Second sub-command targets a clip that doesn't exist -> whole
        // batch must fail without mutating `project` at all.
        let batch = Command::Batch(BatchCommand {
            commands: vec![
                Command::InsertClip(InsertClipCommand {
                    clip: clip("c2", "t1", 2_000_000),
                }),
                Command::SetClip(SetClipCommand {
                    old: clip("does-not-exist", "t1", 0),
                    new: clip("does-not-exist", "t1", 100),
                }),
            ],
        });
        let err = batch.apply(&mut project).unwrap_err();
        assert!(matches!(err, TimelineError::ClipNotFound { .. }));
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }

    #[test]
    fn set_cuts_undo_redo_round_trips() {
        use crate::project::{CutKind, CutReason};

        let mut project = project_with_one_clip();
        let before = serde_json::to_value(&project).unwrap();

        let new_cuts = vec![Cut {
            id: "cut1".into(),
            kind: CutKind::Remove,
            source_media_id: "m1".into(),
            start_us: 0,
            end_us: 1_000_000,
            reason: CutReason::Silence,
            applied: false,
        }];
        let cmd = Command::SetCuts(SetCutsCommand {
            old: project.cuts.clone(),
            new: new_cuts.clone(),
        });

        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.cuts.len(), 1);
        assert_eq!(project.cuts[0].id, "cut1");

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);

        history.redo(&mut project).unwrap();
        assert_eq!(project.cuts, new_cuts);
    }

    fn caption(id: &str, track_id: &str, start_us: i64, end_us: i64) -> Caption {
        Caption {
            id: id.into(),
            track_id: track_id.into(),
            start_us,
            end_us,
            text: "hello".into(),
            words: Vec::new(),
            style_id: None,
        }
    }

    #[test]
    fn insert_and_remove_caption_are_exact_inverses() {
        let mut project = ProjectV1::new("caption test");
        let before = serde_json::to_value(&project).unwrap();

        let cmd = Command::InsertCaption(InsertCaptionCommand {
            caption: caption("cap1", "t1", 0, 1_000_000),
        });
        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.captions.len(), 1);

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
    }

    #[test]
    fn set_caption_undo_redo_round_trips() {
        let mut project = ProjectV1::new("caption set test");
        Command::InsertCaption(InsertCaptionCommand {
            caption: caption("cap1", "t1", 0, 1_000_000),
        })
        .apply(&mut project)
        .unwrap();
        let before = serde_json::to_value(&project).unwrap();

        let old = project.captions[0].clone();
        let mut new = old.clone();
        new.text = "goodbye".into();
        let cmd = Command::SetCaption(SetCaptionCommand { old, new });

        let mut history = History::new(MAX_HISTORY);
        history.apply(&mut project, cmd).unwrap();
        assert_eq!(project.captions[0].text, "goodbye");

        history.undo(&mut project).unwrap();
        assert_eq!(serde_json::to_value(&project).unwrap(), before);

        history.redo(&mut project).unwrap();
        assert_eq!(project.captions[0].text, "goodbye");
    }

    #[test]
    fn set_caption_on_missing_id_errors() {
        let mut project = ProjectV1::new("caption missing test");
        let cmd = Command::SetCaption(SetCaptionCommand {
            old: caption("does-not-exist", "t1", 0, 1_000_000),
            new: caption("does-not-exist", "t1", 0, 2_000_000),
        });
        assert!(matches!(
            cmd.apply(&mut project).unwrap_err(),
            TimelineError::CaptionNotFound { .. }
        ));
    }

    #[test]
    fn captions_stay_sorted_by_track_then_start_after_insert() {
        let mut project = ProjectV1::new("caption sort test");
        let batch = Command::Batch(BatchCommand {
            commands: vec![
                Command::InsertCaption(InsertCaptionCommand {
                    caption: caption("late", "t1", 5_000_000, 6_000_000),
                }),
                Command::InsertCaption(InsertCaptionCommand {
                    caption: caption("early", "t1", 0, 1_000_000),
                }),
            ],
        });
        batch.apply(&mut project).unwrap();
        assert_eq!(
            project
                .captions
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["early", "late"]
        );
    }

    #[test]
    fn undo_with_empty_history_errors() {
        let mut project = project_with_one_clip();
        let mut history = History::new(MAX_HISTORY);
        assert!(matches!(
            history.undo(&mut project),
            Err(TimelineError::NothingToUndo)
        ));
    }

    #[test]
    fn redo_with_empty_stack_errors() {
        let mut project = project_with_one_clip();
        let mut history = History::new(MAX_HISTORY);
        assert!(matches!(
            history.redo(&mut project),
            Err(TimelineError::NothingToRedo)
        ));
    }

    #[test]
    fn applying_a_new_command_clears_redo_stack() {
        let mut project = project_with_one_clip();
        let mut history = History::new(MAX_HISTORY);

        let old = project.clips[0].clone();
        let mut new = old.clone();
        new.position_us = 500_000;
        history
            .apply(&mut project, Command::SetClip(SetClipCommand { old, new }))
            .unwrap();
        history.undo(&mut project).unwrap();
        assert_eq!(history.redo_len(), 1);

        let old2 = project.clips[0].clone();
        let mut new2 = old2.clone();
        new2.position_us = 999;
        history
            .apply(
                &mut project,
                Command::SetClip(SetClipCommand {
                    old: old2,
                    new: new2,
                }),
            )
            .unwrap();
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn history_caps_undo_stack_at_configured_size() {
        let mut project = project_with_one_clip();
        let cap = 3;
        let mut history = History::new(cap);

        for i in 0..10i64 {
            let old = project.clips[0].clone();
            let mut new = old.clone();
            new.position_us = i * 1000;
            history
                .apply(&mut project, Command::SetClip(SetClipCommand { old, new }))
                .unwrap();
        }
        assert_eq!(history.undo_len(), cap);

        // Undo `cap` times must succeed; the (cap+1)th must fail because the
        // older entries were dropped, not because of some other bug.
        for _ in 0..cap {
            history.undo(&mut project).unwrap();
        }
        assert!(matches!(
            history.undo(&mut project),
            Err(TimelineError::NothingToUndo)
        ));
    }
}
