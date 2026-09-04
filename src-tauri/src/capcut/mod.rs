//! `CapCutAdapter`: a direct Rust port of
//! `vendor/capcut-mate/src/pyJianYingDraft/` (`ScriptFile`, `Track`,
//! segment/material/animation/keyframe hierarchy), preserving the
//! microsecond timebase and the "materials collected into the script
//! bucket only on `add_segment`" pattern (`docs/architecture-audit.md`
//! §1/§3/§8). `Project -> CapCutExportGraph -> CapCutAdapter -> Draft`
//! (master prompt §70).
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 9 (`IMPLEMENTATION_PLAN.md`).
