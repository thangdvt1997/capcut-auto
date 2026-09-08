use std::path::Path;

use crate::error::AppErrorPayload;
use crate::project::ProjectV1;

/// Constructs a brand-new, in-memory `ProjectV1` with sensible defaults.
/// Deliberately does not touch the filesystem: wiring this into a real
/// Project Manager UI (recent projects, save/open, `project.json` on disk)
/// is later-phase work. This command exists in Phase 2 to exercise the real
/// `ProjectV1` schema (`docs/project-format.md`) end-to-end over IPC and
/// through specta-generated TypeScript types — it is not a placeholder for
/// "create project" functionality, just proof the schema/IPC layer works.
#[tauri::command]
#[specta::specta]
pub fn new_project(name: String) -> ProjectV1 {
    ProjectV1::new(name)
}

/// Atomically writes `project` to `path` (`STUDIO_PLAN.md` Phase D14: the
/// real "Save Project As…" this app never had a command for — every prior
/// phase's own doc comments named this exact gap, e.g. this module's
/// `new_project` above and `commands::diagnostics::SystemInformation::
/// project_directory`'s "no default project folder ... concept exists
/// anywhere on the backend"). A thin IPC wrapper around the already-tested
/// `ProjectV1::save_atomic` (`project::io`, atomic temp-file-then-rename,
/// covered by that module's own round-trip/Unicode/long-path/UNC-shaped-path
/// tests) — no new file-I/O logic is introduced here, only the missing
/// command surface for it.
#[tauri::command]
#[specta::specta]
pub fn save_project_as(project: ProjectV1, path: String) -> Result<(), AppErrorPayload> {
    project
        .save_atomic(Path::new(&path))
        .map_err(|e| AppErrorPayload::from(&e))
}

/// Loads and migrates a project from an arbitrary caller-chosen `path`
/// (`STUDIO_PLAN.md` Phase D14: the real "Open Project…" counterpart to
/// `save_project_as` above). A thin IPC wrapper around the already-tested
/// `ProjectV1::load` (`project::io`) — same "wire up the existing, tested
/// primitive, add no new logic" scope as `save_project_as`.
#[tauri::command]
#[specta::specta]
pub fn open_project(path: String) -> Result<ProjectV1, AppErrorPayload> {
    ProjectV1::load(Path::new(&path)).map_err(|e| AppErrorPayload::from(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_project_as_then_open_project_round_trips_through_the_real_commands() {
        let dir = std::env::temp_dir().join(format!(
            "ave-commands-project-io-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.json");
        let path_str = path.to_string_lossy().to_string();

        let project = ProjectV1::new("Command-Level Round Trip Test");
        save_project_as(project.clone(), path_str.clone()).expect("save_project_as");

        let loaded = open_project(path_str).expect("open_project");
        assert_eq!(loaded.project.id, project.project.id);
        assert_eq!(loaded.project.name, "Command-Level Round Trip Test");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_project_on_a_missing_file_returns_a_real_error_not_a_panic() {
        let missing = std::env::temp_dir()
            .join(format!(
                "ave-commands-project-missing-{}",
                uuid::Uuid::new_v4()
            ))
            .join("project.json");
        let err = open_project(missing.to_string_lossy().to_string()).unwrap_err();
        assert_eq!(err.code, "PROJECT_CORRUPT_JSON");
    }
}
