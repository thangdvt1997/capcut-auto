//! SQLite-backed local media library index (master prompt §35): filename,
//! path, duration, resolution, tags, created/imported timestamps, kind.
//! Deliberately **separate from `project.json`** (master prompt §35's own
//! instruction, echoed in `docs/project-format.md`) — this is a
//! cross-project "everything I've ever imported" index, not part of any one
//! project's save file. Project management / recent-projects / model
//! registry / job history (the other things this module's original Phase 2
//! doc comment named) are later-phase additions to the same database file,
//! not implemented here.
//!
//! Uses `rusqlite`'s bundled SQLite (statically linked, no system libsqlite3
//! dependency) specifically so Windows packaging (Phase 12) never needs to
//! ship or detect a separate SQLite DLL.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::media::error::MediaError;
use crate::project::MediaKind;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MediaLibraryEntry {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub kind: MediaKind,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub tags: Vec<String>,
    /// RFC3339, from the source file's own metadata, if any.
    pub created_at: Option<String>,
    /// RFC3339, when this row was added to the library.
    pub imported_at: String,
    pub thumbnail_path: Option<String>,
    pub proxy_path: Option<String>,
}

/// Guards the single shared `rusqlite::Connection` this app manages as Tauri
/// state. `rusqlite::Connection` isn't `Sync`, so every command touching the
/// database goes through this mutex — acceptable here since the media
/// library is local-disk SQLite, not a network round trip; no command holds
/// the lock across an `.await` point.
pub struct MediaLibrary(pub Mutex<Connection>);

fn to_db_error(context: &str, err: rusqlite::Error) -> MediaError {
    MediaError::DatabaseError {
        details: format!("{context}: {err}"),
    }
}

pub fn open(db_path: &Path) -> Result<Connection, MediaError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MediaError::DatabaseError {
            details: format!("creating db directory {}: {e}", parent.display()),
        })?;
    }
    let conn = Connection::open(db_path).map_err(|e| to_db_error("opening media library db", e))?;
    init_schema(&conn)?;
    Ok(conn)
}

/// In-memory database, for unit tests — avoids touching the filesystem and
/// lets tests run in parallel without colliding on a shared file.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, MediaError> {
    let conn = Connection::open_in_memory().map_err(|e| to_db_error("opening in-memory db", e))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), MediaError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_library (
            id            TEXT PRIMARY KEY,
            filename      TEXT NOT NULL,
            path          TEXT NOT NULL UNIQUE,
            kind          TEXT NOT NULL,
            duration_us   INTEGER NOT NULL,
            width         INTEGER NOT NULL,
            height        INTEGER NOT NULL,
            tags          TEXT NOT NULL DEFAULT '[]',
            created_at    TEXT,
            imported_at   TEXT NOT NULL,
            thumbnail_path TEXT,
            proxy_path    TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_media_library_kind ON media_library(kind);
         CREATE INDEX IF NOT EXISTS idx_media_library_imported_at ON media_library(imported_at);",
    )
    .map_err(|e| to_db_error("creating media_library schema", e))
}

fn kind_to_str(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Image => "image",
    }
}

fn kind_from_str(s: &str) -> MediaKind {
    match s {
        "audio" => MediaKind::Audio,
        "image" => MediaKind::Image,
        _ => MediaKind::Video,
    }
}

/// Insert a new row, or replace the existing one for the same `path` (a
/// re-import of an already-known file refreshes its metadata rather than
/// creating a duplicate row).
pub fn upsert_media(conn: &Connection, entry: &MediaLibraryEntry) -> Result<(), MediaError> {
    let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO media_library (id, filename, path, kind, duration_us, width, height, tags, created_at, imported_at, thumbnail_path, proxy_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(path) DO UPDATE SET
            filename = excluded.filename,
            kind = excluded.kind,
            duration_us = excluded.duration_us,
            width = excluded.width,
            height = excluded.height,
            tags = excluded.tags,
            created_at = excluded.created_at,
            imported_at = excluded.imported_at,
            thumbnail_path = excluded.thumbnail_path,
            proxy_path = excluded.proxy_path",
        params![
            entry.id,
            entry.filename,
            entry.path,
            kind_to_str(entry.kind),
            entry.duration_us,
            entry.width,
            entry.height,
            tags_json,
            entry.created_at,
            entry.imported_at,
            entry.thumbnail_path,
            entry.proxy_path,
        ],
    )
    .map_err(|e| to_db_error("upserting media_library row", e))?;
    Ok(())
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<MediaLibraryEntry> {
    let tags_json: String = row.get("tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let kind_str: String = row.get("kind")?;
    Ok(MediaLibraryEntry {
        id: row.get("id")?,
        filename: row.get("filename")?,
        path: row.get("path")?,
        kind: kind_from_str(&kind_str),
        duration_us: row.get("duration_us")?,
        width: row.get("width")?,
        height: row.get("height")?,
        tags,
        created_at: row.get("created_at")?,
        imported_at: row.get("imported_at")?,
        thumbnail_path: row.get("thumbnail_path")?,
        proxy_path: row.get("proxy_path")?,
    })
}

/// Search by filename substring (case-insensitive) and/or kind, newest
/// import first. `query: None` with `kind: None` is "list everything",
/// bounded by `limit` (master prompt §35's `football`/`bitcoin`/`city`-style
/// filename search — no AI tagging pipeline exists yet, so search is
/// filename/tag substring matching only, honestly not semantic search).
pub fn search_media(
    conn: &Connection,
    query: Option<&str>,
    kind: Option<MediaKind>,
    limit: u32,
) -> Result<Vec<MediaLibraryEntry>, MediaError> {
    let like = query.map(|q| format!("%{}%", q.to_lowercase()));
    let kind_str = kind.map(kind_to_str);

    let mut stmt = conn
        .prepare(
            "SELECT * FROM media_library
             WHERE (?1 IS NULL OR lower(filename) LIKE ?1 OR lower(tags) LIKE ?1)
               AND (?2 IS NULL OR kind = ?2)
             ORDER BY imported_at DESC
             LIMIT ?3",
        )
        .map_err(|e| to_db_error("preparing search_media", e))?;

    let rows = stmt
        .query_map(params![like, kind_str, limit], row_to_entry)
        .map_err(|e| to_db_error("running search_media", e))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_db_error("reading search_media rows", e))
}

pub fn get_media_by_path(
    conn: &Connection,
    path: &str,
) -> Result<Option<MediaLibraryEntry>, MediaError> {
    conn.query_row(
        "SELECT * FROM media_library WHERE path = ?1",
        params![path],
        row_to_entry,
    )
    .optional()
    .map_err(|e| to_db_error("get_media_by_path", e))
}

/// Update just the proxy path for an existing row, called once background
/// proxy generation (`commands::media::generate_media_proxy`) finishes —
/// separate from `upsert_media` because proxy generation happens after the
/// initial import row is already written and doesn't have (or need) the
/// full `MediaLibraryEntry` in hand at that point.
pub fn set_proxy_path(
    conn: &Connection,
    id: &str,
    proxy_path: Option<&str>,
) -> Result<(), MediaError> {
    conn.execute(
        "UPDATE media_library SET proxy_path = ?1 WHERE id = ?2",
        params![proxy_path, id],
    )
    .map_err(|e| to_db_error("set_proxy_path", e))?;
    Ok(())
}

pub fn remove_media(conn: &Connection, id: &str) -> Result<(), MediaError> {
    conn.execute("DELETE FROM media_library WHERE id = ?1", params![id])
        .map_err(|e| to_db_error("remove_media", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, filename: &str, kind: MediaKind, tags: Vec<&str>) -> MediaLibraryEntry {
        MediaLibraryEntry {
            id: id.to_string(),
            filename: filename.to_string(),
            path: format!("/media/{filename}"),
            kind,
            duration_us: 5_000_000,
            width: 1920,
            height: 1080,
            tags: tags.into_iter().map(String::from).collect(),
            created_at: None,
            imported_at: "2026-09-04T00:00:00Z".to_string(),
            thumbnail_path: None,
            proxy_path: None,
        }
    }

    #[test]
    fn set_proxy_path_updates_an_existing_row() {
        let conn = open_in_memory().unwrap();
        upsert_media(&conn, &sample("m1", "a.mp4", MediaKind::Video, vec![])).unwrap();
        set_proxy_path(&conn, "m1", Some("/cache/m1/proxy.mp4")).unwrap();
        let found = get_media_by_path(&conn, "/media/a.mp4").unwrap().unwrap();
        assert_eq!(found.proxy_path.as_deref(), Some("/cache/m1/proxy.mp4"));
    }

    #[test]
    fn round_trips_a_media_entry() {
        let conn = open_in_memory().unwrap();
        let entry = sample(
            "m1",
            "football_match.mp4",
            MediaKind::Video,
            vec!["football", "sports"],
        );
        upsert_media(&conn, &entry).unwrap();

        let found = get_media_by_path(&conn, "/media/football_match.mp4")
            .unwrap()
            .unwrap();
        assert_eq!(found.id, "m1");
        assert_eq!(found.tags, vec!["football", "sports"]);
        assert_eq!(found.kind, MediaKind::Video);
    }

    #[test]
    fn reimporting_the_same_path_updates_rather_than_duplicates() {
        let conn = open_in_memory().unwrap();
        let mut entry = sample("m1", "a.mp4", MediaKind::Video, vec![]);
        upsert_media(&conn, &entry).unwrap();

        entry.duration_us = 9_999_999;
        entry.id = "m1-reimported".to_string(); // even a different id, same path
        upsert_media(&conn, &entry).unwrap();

        let all = search_media(&conn, None, None, 100).unwrap();
        assert_eq!(all.len(), 1, "expected one row, got {all:?}");
        assert_eq!(all[0].duration_us, 9_999_999);
    }

    #[test]
    fn search_matches_filename_case_insensitively() {
        let conn = open_in_memory().unwrap();
        upsert_media(
            &conn,
            &sample("m1", "Football_Match.mp4", MediaKind::Video, vec![]),
        )
        .unwrap();
        upsert_media(
            &conn,
            &sample("m2", "cooking.mp4", MediaKind::Video, vec![]),
        )
        .unwrap();

        let results = search_media(&conn, Some("football"), None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn search_matches_tags_too() {
        let conn = open_in_memory().unwrap();
        upsert_media(
            &conn,
            &sample(
                "m1",
                "clip.mp4",
                MediaKind::Video,
                vec!["bitcoin", "finance"],
            ),
        )
        .unwrap();

        let results = search_media(&conn, Some("bitcoin"), None, 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_filters_by_kind() {
        let conn = open_in_memory().unwrap();
        upsert_media(&conn, &sample("m1", "a.mp4", MediaKind::Video, vec![])).unwrap();
        upsert_media(&conn, &sample("m2", "b.mp3", MediaKind::Audio, vec![])).unwrap();

        let videos = search_media(&conn, None, Some(MediaKind::Video), 10).unwrap();
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].id, "m1");
    }

    #[test]
    fn search_respects_the_limit() {
        let conn = open_in_memory().unwrap();
        for i in 0..5 {
            upsert_media(
                &conn,
                &sample(
                    &format!("m{i}"),
                    &format!("clip{i}.mp4"),
                    MediaKind::Video,
                    vec![],
                ),
            )
            .unwrap();
        }
        let results = search_media(&conn, None, None, 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn remove_deletes_the_row() {
        let conn = open_in_memory().unwrap();
        upsert_media(&conn, &sample("m1", "a.mp4", MediaKind::Video, vec![])).unwrap();
        remove_media(&conn, "m1").unwrap();
        assert!(search_media(&conn, None, None, 10).unwrap().is_empty());
    }
}
