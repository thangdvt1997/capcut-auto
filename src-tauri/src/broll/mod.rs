//! B-roll architecture (master prompt §34, `IMPLEMENTATION_PLAN.md` Phase
//! 11). Mirrors `highlights`' own three-way split (module doc comment
//! there): `provider` is the real, local, no-AI-needed half (a
//! `BRollProvider` trait plus one real implementation searching the existing
//! Phase 3 media library, `crate::db`); `suggest` is the one piece that
//! needs an `AIProvider` call (proposing keyword/timing/reason B-roll
//! insertion points from a transcript, per master prompt §34's own worked
//! example); `combine` is where the two meet, pairing each AI suggestion
//! with whatever real local media actually matches it.
//!
//! Sources named by master prompt §34 — "Local media library", "User-
//! selected folders", "Optional external providers later" — map onto this
//! module as: local media library is `provider::LocalLibraryBRollProvider`
//! (implemented); user-selected folders and external providers are
//! architecturally supported (any `impl BRollProvider`) but not implemented
//! this pass (`provider` module doc comment's honest-scope note) — and, per
//! the master prompt's own explicit instruction, this crate never
//! automatically downloads media from an external/arbitrary source.

pub mod combine;
pub mod error;
pub mod provider;
pub mod suggest;

pub use combine::{suggest_and_search, BRollSuggestionWithCandidates};
pub use provider::{BRollCandidate, BRollProvider, BRollQuery, LocalLibraryBRollProvider};
pub use suggest::BRollSuggestion;
