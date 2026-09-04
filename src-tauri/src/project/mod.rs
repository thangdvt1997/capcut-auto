//! `ProjectV1` — the application's own project file format
//! (`docs/project-format.md`). This is the one module in the Phase 2
//! scaffold that is fully implemented rather than a placeholder: the schema
//! was already designed in Phase 1, and this phase's task explicitly calls
//! for it to land as real Rust structs.
//!
//! What's here: the full `ProjectV1` struct tree (serde + specta, so it
//! round-trips as JSON and generates correct TypeScript types), a
//! `ProjectV1::new` constructor, atomic save/load, and the
//! version-dispatching `migrate_to_latest` stub the schema doc calls for.
//!
//! What's deliberately NOT here yet: recovery-snapshot pruning (master
//! prompt §86, Phase 12), and any UI/command wiring beyond the Phase 2
//! `new_project` demo command — those belong to the Project Manager
//! (master prompt §6), not the schema module itself.

mod error;
mod io;
mod types;

pub use error::{AppErrorPayload, ProjectError};
pub use types::*;
