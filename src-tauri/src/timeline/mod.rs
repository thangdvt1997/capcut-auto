//! General multi-clip, multi-track timeline engine: clip operations
//! (split/trim/move/delete/duplicate/snap — `ops`), caption correction
//! operations (split/merge/retime/find-replace/bulk-style — `captions`),
//! command-based undo/redo (`command`), copy/paste (`clipboard`), the live
//! in-memory session Tauri commands operate on (`session`), and this
//! subsystem's error type (`error`). Operates on the `ProjectV1` schema
//! (`crate::project`) — NOT autocut's single-shared-CutList model
//! (`docs/architecture-audit.md` §4/§5).
//!
//! Undo/redo is command-based, never whole-project snapshot/copy (master
//! prompt §11): see `command`'s module doc comment for the design. `ops`
//! functions are pure builders — read a `&ProjectV1`, return a `Command` —
//! so every clip/track mutation in the app goes through the same
//! apply/invert/History machinery, including `SyncGroup` propagation.
//!
//! The Tauri command surface fronting this module lives in
//! `crate::commands::timeline`, kept thin per master prompt §66.

pub mod captions;
pub mod clipboard;
pub mod command;
pub mod error;
pub mod ops;
pub mod session;
pub mod silence;
pub mod sync;
