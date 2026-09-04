//! `TimelineSession` — the live, in-memory project a timeline command batch
//! operates on, plus its undo `History` and clipboard, managed as Tauri
//! state (`TimelineState`) the same way `MediaLibrary` is managed in
//! `crate::db` (a `Mutex`-guarded value, since Tauri commands aren't
//! guaranteed to run on the same thread). `commands::timeline` is the only
//! caller; kept separate from that module so the actual session type has
//! nothing Tauri-specific in it and stays easy to unit test on its own.

use std::sync::Mutex;

use crate::project::ProjectV1;

use super::clipboard::Clipboard;
use super::command::{Command, History, MAX_HISTORY};
use super::error::TimelineError;

pub struct TimelineSession {
    pub project: ProjectV1,
    pub history: History,
    pub clipboard: Option<Clipboard>,
}

impl TimelineSession {
    pub fn new(project: ProjectV1) -> Self {
        Self {
            project,
            history: History::new(MAX_HISTORY),
            clipboard: None,
        }
    }

    pub fn apply(&mut self, command: Command) -> Result<(), TimelineError> {
        self.history.apply(&mut self.project, command)
    }

    pub fn undo(&mut self) -> Result<(), TimelineError> {
        self.history.undo(&mut self.project)
    }

    pub fn redo(&mut self) -> Result<(), TimelineError> {
        self.history.redo(&mut self.project)
    }
}

/// Tauri-managed state: `None` until a project is loaded via
/// `commands::timeline::load_timeline_project`.
#[derive(Default)]
pub struct TimelineState(pub Mutex<Option<TimelineSession>>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Clip, ClipSettings, Track, TrackKind};
    use crate::timeline::command::{InsertClipCommand, SetClipCommand};

    fn sample_project() -> ProjectV1 {
        let mut p = ProjectV1::new("session test");
        p.tracks.push(Track {
            id: "t1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec![],
        });
        p
    }

    fn clip(id: &str) -> Clip {
        Clip {
            id: id.into(),
            track_id: "t1".into(),
            media_id: None,
            source_in_us: 0,
            source_out_us: 1_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    #[test]
    fn apply_undo_redo_round_trip_through_session() {
        let mut session = TimelineSession::new(sample_project());
        let before = serde_json::to_value(&session.project).unwrap();

        session
            .apply(Command::InsertClip(InsertClipCommand { clip: clip("c1") }))
            .unwrap();
        assert_eq!(session.project.clips.len(), 1);

        let old = session.project.clips[0].clone();
        let mut new = old.clone();
        new.position_us = 42;
        session
            .apply(Command::SetClip(SetClipCommand { old, new }))
            .unwrap();
        assert_eq!(session.project.clips[0].position_us, 42);

        session.undo().unwrap();
        assert_eq!(session.project.clips[0].position_us, 0);
        session.undo().unwrap();
        assert_eq!(serde_json::to_value(&session.project).unwrap(), before);

        session.redo().unwrap();
        session.redo().unwrap();
        assert_eq!(session.project.clips[0].position_us, 42);
    }

    #[test]
    fn timeline_state_starts_empty() {
        let state = TimelineState::default();
        assert!(state.0.lock().unwrap().is_none());
    }
}
