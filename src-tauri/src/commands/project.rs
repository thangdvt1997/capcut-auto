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
