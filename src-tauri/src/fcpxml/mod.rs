//! FCPXML 1.11 export, reimplemented from
//! `vendor/autocut/src-tauri/src/export_fcpxml.rs` and `timecode.rs`'s
//! rational-timecode and lane/connected-clip design
//! (`docs/architecture-audit.md` §2/§8), generalized to the multi-clip
//! timeline rather than autocut's single-CutList model.
//!
//! - `timecode`: `Rational`-based NTSC-rate detection + the degenerate-fps
//!   guard, and `i64` microseconds → FCPXML rational-string rendering.
//! - `document`: the actual `<fcpxml>` document construction — resources
//!   (formats/assets), spine (primary storyline), and connected clips (lanes)
//!   from a `ProjectV1`. See its module doc comment for the full
//!   track-kind → FCPXML mapping decisions.
//! - `error`: `FcpxmlError`, this subsystem's slice of the standard
//!   `{code, message, details, recoverable, suggested_action}` envelope.
//! - `export`: the public `build_fcpxml`/`export_fcpxml_to_file` functions
//!   and the `export_fcpxml` Tauri command. The command lives here (not
//!   under `commands/`) so this phase's work stays scoped to this module
//!   plus an additive registration line in `lib.rs`, per the phase's task
//!   boundary.

pub mod document;
pub mod error;
pub mod export;
pub mod timecode;

pub use error::FcpxmlError;
pub use export::{build_fcpxml, export_fcpxml, export_fcpxml_to_file};
