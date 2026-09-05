//! `BRollProvider` trait (master prompt §34) — same technique-independent
//! shape as `vad::provider::VadProvider`/`transcription::provider::
//! TranscriptionProvider`/`ai::provider::AIProvider`/`reframe::provider::
//! SubjectTracker`: a `search` call that hands back candidates, with no
//! notion baked into the trait of *where* those candidates came from. Master
//! prompt §34 names three sources — "Local media library", "User-selected
//! folders", "Optional external providers later" — and this trait is
//! deliberately shaped so all three are just different `impl BRollProvider`s
//! behind the same call site, never a hardcoded local-search-only path.
//!
//! ## What's actually implemented this pass
//!
//! [`LocalLibraryBRollProvider`] is the one real, working implementation:
//! it searches the *existing* Phase 3 media library (`crate::db::
//! search_media`/`MediaLibraryEntry`) by keyword against `filename`/`tags` —
//! reusing that exact search function rather than re-implementing keyword
//! matching a second time. "User-selected folders" and "optional external
//! providers" are **not implemented** — same honest-scope treatment
//! `reframe::provider`'s module doc comment used for face/person detection:
//! the trait's signature carries no technique-specific type, so a
//! `FolderBRollProvider`/`StockFootageBRollProvider` could be added later as
//! another `impl BRollProvider` with zero changes to this trait or any
//! caller, but building a real one is out of scope here. Per the master
//! prompt's own explicit instruction ("Do NOT automatically download
//! copyrighted media from arbitrary websites"), this pass does not wire up
//! *any* HTTP client to a stock-footage API, not even a stubbed/best-effort
//! one — the architectural support stops at the trait shape.
//!
//! ## Why `Send + Sync` still holds despite `LocalLibraryBRollProvider`
//! borrowing a `Connection`
//!
//! Unlike `AIProvider`/`VadProvider` (which cross a real `spawn_blocking`
//! thread boundary as owned `Box<dyn Trait>` trait objects),
//! `BRollProvider::search` here is only ever called synchronously from
//! within a Tauri command that already holds a `State<'_, MediaLibrary>` —
//! there is no cross-thread handoff to justify. The trait still carries the
//! same `Send + Sync` bound as this codebase's other `*Provider` traits for
//! consistency and because it costs nothing here:
//! `crate::db::MediaLibrary(Mutex<Connection>)` is itself `Sync` (a
//! `Mutex<T>` is `Sync` whenever `T: Send`, and `rusqlite::Connection` is
//! `Send` — it already crosses real threads elsewhere in this crate, e.g.
//! `commands::media::spawn_proxy_job`'s `spawn_blocking` closures), so `&
//! MediaLibrary` is `Send + Sync` too.

use crate::db::{self, MediaLibrary, MediaLibraryEntry};
use crate::project::MediaKind;

use super::error::BRollError;

/// A keyword search request. Deliberately minimal — master prompt §34's own
/// worked example only ever needs a keyword (`"bitcoin price chart"`) plus
/// how many results to return; a `kind` filter is included since a caller
/// might reasonably want to restrict a search to video-only B-roll, mirroring
/// `db::search_media`'s own optional `kind` parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct BRollQuery {
    pub keyword: String,
    pub kind: Option<MediaKind>,
    pub limit: u32,
}

impl BRollQuery {
    pub fn new(keyword: impl Into<String>, limit: u32) -> Self {
        Self {
            keyword: keyword.into(),
            kind: None,
            limit,
        }
    }
}

/// One candidate piece of local B-roll media a `BRollProvider` found for a
/// query — a reviewable, real result (never a fabricated match): every field
/// here traces back to a real `MediaLibraryEntry` row.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct BRollCandidate {
    pub media_id: String,
    pub filename: String,
    pub path: String,
    pub kind: MediaKind,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub tags: Vec<String>,
    pub thumbnail_path: Option<String>,
}

impl From<MediaLibraryEntry> for BRollCandidate {
    fn from(entry: MediaLibraryEntry) -> Self {
        BRollCandidate {
            media_id: entry.id,
            filename: entry.filename,
            path: entry.path,
            kind: entry.kind,
            duration_us: entry.duration_us,
            width: entry.width,
            height: entry.height,
            tags: entry.tags,
            thumbnail_path: entry.thumbnail_path,
        }
    }
}

/// B-roll source abstraction (module doc comment). `search` is the only
/// required method — a real, working query against whatever this
/// implementation's source actually is.
pub trait BRollProvider: Send + Sync {
    /// Human-readable source name, used only in error/UI context — never
    /// parsed by callers (same convention as `AIProvider::name`).
    fn name(&self) -> &'static str;

    fn search(&self, query: &BRollQuery) -> Result<Vec<BRollCandidate>, BRollError>;
}

/// The one real, working `BRollProvider`: searches the existing Phase 3
/// local media library by keyword against `filename`/`tags`
/// (`db::search_media`, unchanged — this type adds no new matching logic of
/// its own). Holds a borrowed reference to the already-managed
/// `MediaLibrary` Tauri state rather than owning/cloning a `Connection`
/// (`rusqlite::Connection` has no `Clone`), locking its inner `Mutex` only
/// for the duration of one `search` call.
pub struct LocalLibraryBRollProvider<'a> {
    library: &'a MediaLibrary,
}

impl<'a> LocalLibraryBRollProvider<'a> {
    pub fn new(library: &'a MediaLibrary) -> Self {
        Self { library }
    }
}

impl BRollProvider for LocalLibraryBRollProvider<'_> {
    fn name(&self) -> &'static str {
        "local_media_library"
    }

    fn search(&self, query: &BRollQuery) -> Result<Vec<BRollCandidate>, BRollError> {
        let conn = self.library.0.lock().expect("media library mutex poisoned");
        let entries = db::search_media(&conn, Some(&query.keyword), query.kind, query.limit)?;
        Ok(entries.into_iter().map(BRollCandidate::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn sample_entry(id: &str, filename: &str, tags: Vec<&str>) -> MediaLibraryEntry {
        MediaLibraryEntry {
            id: id.to_string(),
            filename: filename.to_string(),
            path: format!("/media/{filename}"),
            kind: MediaKind::Video,
            duration_us: 5_000_000,
            width: 1920,
            height: 1080,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: None,
            imported_at: "2026-09-04T00:00:00Z".to_string(),
            thumbnail_path: Some(format!("/thumbs/{id}.jpg")),
            proxy_path: None,
        }
    }

    fn library_with(entries: &[MediaLibraryEntry]) -> MediaLibrary {
        let conn = db::open_in_memory().unwrap();
        for entry in entries {
            db::upsert_media(&conn, entry).unwrap();
        }
        MediaLibrary(Mutex::new(conn))
    }

    #[test]
    fn searches_the_real_local_library_by_keyword_against_tags() {
        let library = library_with(&[
            sample_entry("m1", "bitcoin_chart.mp4", vec!["bitcoin", "finance"]),
            sample_entry("m2", "cooking.mp4", vec!["food"]),
        ]);
        let provider = LocalLibraryBRollProvider::new(&library);
        let results = provider
            .search(&BRollQuery::new("bitcoin", 10))
            .expect("search succeeds");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].media_id, "m1");
        assert_eq!(results[0].filename, "bitcoin_chart.mp4");
        assert_eq!(results[0].thumbnail_path.as_deref(), Some("/thumbs/m1.jpg"));
    }

    #[test]
    fn searches_by_filename_too() {
        let library = library_with(&[sample_entry("m1", "football_match.mp4", vec![])]);
        let provider = LocalLibraryBRollProvider::new(&library);
        let results = provider
            .search(&BRollQuery::new("football", 10))
            .expect("search succeeds");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].media_id, "m1");
    }

    #[test]
    fn an_unmatched_keyword_returns_an_honest_empty_result() {
        let library = library_with(&[sample_entry("m1", "cooking.mp4", vec!["food"])]);
        let provider = LocalLibraryBRollProvider::new(&library);
        let results = provider
            .search(&BRollQuery::new("spaceship", 10))
            .expect("search succeeds");
        assert!(results.is_empty());
    }

    #[test]
    fn respects_the_kind_filter() {
        let mut video = sample_entry("m1", "clip.mp4", vec!["shared"]);
        video.kind = MediaKind::Video;
        let mut audio = sample_entry("m2", "clip.mp3", vec!["shared"]);
        audio.kind = MediaKind::Audio;
        let library = library_with(&[video, audio]);
        let provider = LocalLibraryBRollProvider::new(&library);

        let mut query = BRollQuery::new("shared", 10);
        query.kind = Some(MediaKind::Video);
        let results = provider.search(&query).expect("search succeeds");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].media_id, "m1");
    }

    #[test]
    fn respects_the_limit() {
        let entries: Vec<MediaLibraryEntry> = (0..5)
            .map(|i| sample_entry(&format!("m{i}"), &format!("clip{i}.mp4"), vec!["shared"]))
            .collect();
        let library = library_with(&entries);
        let provider = LocalLibraryBRollProvider::new(&library);
        let results = provider
            .search(&BRollQuery::new("shared", 2))
            .expect("search succeeds");
        assert_eq!(results.len(), 2);
    }
}
