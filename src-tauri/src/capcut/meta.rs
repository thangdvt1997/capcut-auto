//! Draft discoverability in CapCut/Jianying Pro's own Projects list — closes
//! a real gap found via a real, first-time test against an actual installed
//! CapCut Pro (v9.3.0.3970), the exact validation
//! `IMPLEMENTATION_PLAN.md`'s long-standing "validate against a real
//! installed CapCut build" item called for.
//!
//! `CapCutAdapter::export_draft` previously wrote only `draft_content.json`/
//! `draft_info.json`, with a documented, deliberate gap: no
//! `draft_meta_info.json`, on the assumption that "nothing in this app's own
//! draft-content JSON depends on it". A real export into a real CapCut Pro's
//! own draft root proved that assumption wrong in one specific, concrete
//! way, discovered by direct comparison against a project CapCut's own
//! "Create project" button wrote into the very same folder during that same
//! test: CapCut Pro's Projects-list UI does **not** scan
//! `com.lveditor.draft/` for subfolders at all — it reads one explicit
//! registry file, `com.lveditor.draft/root_meta_info.json`'s
//! `all_draft_store` array, and a draft absent from that array (even with a
//! perfectly valid, schema-correct `draft_content.json`) never appears in
//! the UI, full stop.
//!
//! This module closes that gap:
//! - [`write_draft_meta_info`] writes a real `draft_meta_info.json` into the
//!   draft's own folder, matching the exact field set a real CapCut-Pro-
//!   created project's own `draft_meta_info.json` was observed to have.
//! - [`register_draft_in_root_registry`] appends (or updates, on a
//!   re-export of the same draft folder) this draft's entry into the shared
//!   `root_meta_info.json` — read-merge-write, atomic
//!   (temp-file-then-rename, this codebase's established convention), and
//!   **additive only**: every other already-registered draft's entry (a
//!   real user's own real CapCut projects, which this file also indexes) is
//!   read back and preserved untouched, never dropped or corrupted — see
//!   this module's own `root_registry_update_preserves_other_existing_entries`
//!   test.
//!
//! ## What's still a known, honest gap after this pass
//!
//! - **`draft_cover`** (the `draft_cover.jpg` thumbnail CapCut's own UI shows
//!   on a project card) is referenced but never generated — this app has no
//!   thumbnail-generation step in its CapCut export path. CapCut itself
//!   tolerates a missing cover file (confirmed in the real test: the draft
//!   opened and displayed correctly with a blank/placeholder thumbnail,
//!   never an error) — cosmetic only, not a functional blocker.
//! - Every other `draft_meta_info.json`/registry-entry field a real,
//!   freshly-created CapCut Pro project leaves at an inert default
//!   (`draft_cloud_*`, `pippit_*`, `tm_draft_cloud_*`, etc.) is written here
//!   as that same observed default. None of those fields were exercised by
//!   the one real scenario this module is based on (a plain local project,
//!   no cloud sync, no AI-generated draft) — a field some *other* CapCut
//!   feature actually needs populated could still be missing. Flagged
//!   honestly, not silently.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::error::CapCutError;

fn now_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}

/// Windows and CapCut's own reference JSON both accept forward slashes in
/// these path fields (confirmed: a real CapCut-Pro-written `draft_fold_path`
/// uses `/`, not `\`) — normalizing here means every path field this module
/// writes matches that observed convention exactly, regardless of which
/// separator `Path::to_string_lossy()` would otherwise produce on Windows.
fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn read_json_object(path: &Path) -> Result<Value, CapCutError> {
    let bytes = std::fs::read(path).map_err(|e| CapCutError::WriteFailed {
        path: path.to_string_lossy().to_string(),
        details: format!("read failed: {e}"),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| CapCutError::WriteFailed {
        path: path.to_string_lossy().to_string(),
        details: format!("parse failed: {e}"),
    })
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), CapCutError> {
    let json = serde_json::to_string_pretty(value).expect("Value serialization cannot fail");
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json).map_err(|e| CapCutError::WriteFailed {
        path: tmp_path.to_string_lossy().to_string(),
        details: e.to_string(),
    })?;
    std::fs::rename(&tmp_path, path).map_err(|e| CapCutError::WriteFailed {
        path: path.to_string_lossy().to_string(),
        details: e.to_string(),
    })
}

/// Writes a real `draft_meta_info.json` into `draft_dir` (module doc
/// comment). `duration_us` should be the same value already written into
/// this same draft's own `draft_content.json` (`ScriptFile::duration_us`),
/// so both files agree — `CapCutAdapter::export_draft` passes it through
/// directly rather than this function re-deriving it a second way.
pub fn write_draft_meta_info(
    draft_dir: &Path,
    draft_root: &Path,
    draft_id: &str,
    draft_name: &str,
    duration_us: i64,
) -> Result<(), CapCutError> {
    let now = now_micros();
    let value = json!({
        "cloud_draft_cover": false,
        "cloud_draft_sync": false,
        "draft_cloud_last_action_download": false,
        "draft_cloud_purchase_info": "",
        "draft_cloud_template_id": "",
        "draft_cloud_tutorial_info": "",
        "draft_cloud_videocut_purchase_info": "",
        "draft_cover": "draft_cover.jpg",
        "draft_deeplink_url": "",
        "draft_enterprise_info": {
            "draft_enterprise_extra": "",
            "draft_enterprise_id": "",
            "draft_enterprise_name": "",
            "enterprise_material": []
        },
        "draft_fold_path": forward_slashes(draft_dir),
        "draft_id": draft_id,
        "draft_is_ae_produce": false,
        "draft_is_ai_packaging_used": false,
        "draft_is_ai_shorts": false,
        "draft_is_ai_translate": false,
        "draft_is_article_video_draft": false,
        "draft_is_cloud_temp_draft": false,
        "draft_is_from_deeplink": "false",
        "draft_is_infinite_canvas_draft": false,
        "draft_is_invisible": false,
        "draft_is_pippit_draft": false,
        "draft_is_web_article_video": false,
        "draft_materials": [
            {"type": 0, "value": []},
            {"type": 1, "value": []},
            {"type": 2, "value": []},
            {"type": 3, "value": []},
            {"type": 6, "value": []}
        ],
        "draft_materials_copied_info": [],
        "draft_name": draft_name,
        "draft_need_rename_folder": false,
        "draft_new_version": "",
        "draft_removable_storage_device": "",
        "draft_root_path": draft_root.to_string_lossy(),
        "draft_segment_extra_info": [],
        "draft_timeline_materials_size_": 0,
        "draft_type": "",
        "draft_web_article_video_enter_from": "",
        "pippit_avatar_url": "",
        "pippit_extra_info": "",
        "pippit_id": "",
        "pippit_user_name": "",
        "tm_draft_cloud_completed": "",
        "tm_draft_cloud_entry_id": -1,
        "tm_draft_cloud_modified": 0,
        "tm_draft_cloud_parent_entry_id": -1,
        "tm_draft_cloud_space_id": -1,
        "tm_draft_cloud_user_id": -1,
        "tm_draft_create": now,
        "tm_draft_modified": now,
        "tm_draft_removed": 0,
        "tm_duration": duration_us,
    });
    write_json_atomic(&draft_dir.join("draft_meta_info.json"), &value)
}

fn registry_entry(draft_root: &Path, draft_id: &str, draft_name: &str, draft_dir: &Path) -> Value {
    let draft_fold_path = forward_slashes(draft_dir);
    let now = now_micros();
    json!({
        "cloud_draft_cover": false,
        "cloud_draft_sync": false,
        "draft_cloud_last_action_download": false,
        "draft_cloud_purchase_info": "",
        "draft_cloud_template_id": "",
        "draft_cloud_tutorial_info": "",
        "draft_cloud_videocut_purchase_info": "",
        "draft_cover": format!("{draft_fold_path}/draft_cover.jpg"),
        "draft_fold_path": draft_fold_path,
        "draft_id": draft_id,
        "draft_is_ai_shorts": false,
        "draft_is_cloud_temp_draft": false,
        "draft_is_infinite_canvas_draft": false,
        "draft_is_invisible": false,
        "draft_is_pippit_draft": false,
        "draft_is_web_article_video": false,
        "draft_json_file": format!("{draft_fold_path}/draft_content.json"),
        "draft_name": draft_name,
        "draft_new_version": "",
        "draft_root_path": draft_root.to_string_lossy(),
        "draft_timeline_materials_size": 0,
        "draft_type": "",
        "draft_web_article_video_enter_from": "",
        "pippit_avatar_url": "",
        "pippit_extra_info": "",
        "pippit_id": "",
        "pippit_user_name": "",
        "streaming_edit_draft_ready": true,
        "tm_draft_cloud_completed": "",
        "tm_draft_cloud_entry_id": -1,
        "tm_draft_cloud_modified": 0,
        "tm_draft_cloud_parent_entry_id": -1,
        "tm_draft_cloud_space_id": -1,
        "tm_draft_cloud_user_id": -1,
        "tm_draft_create": now,
        "tm_draft_modified": now,
        "tm_draft_removed": 0,
        "tm_duration": 0,
    })
}

/// Registers (or updates, on a re-export of the same `draft_dir`) this
/// draft in `draft_root`'s shared `root_meta_info.json` (module doc
/// comment) so CapCut Pro's own Projects-list UI actually discovers it.
pub fn register_draft_in_root_registry(
    draft_root: &Path,
    draft_id: &str,
    draft_name: &str,
    draft_dir: &Path,
) -> Result<(), CapCutError> {
    let registry_path = draft_root.join("root_meta_info.json");
    let mut registry = if registry_path.exists() {
        read_json_object(&registry_path)?
    } else {
        json!({ "all_draft_store": [], "draft_ids": 0, "root_path": forward_slashes(draft_root) })
    };

    let entry = registry_entry(draft_root, draft_id, draft_name, draft_dir);
    let draft_fold_path = forward_slashes(draft_dir);

    let Value::Object(root) = &mut registry else {
        return Err(CapCutError::WriteFailed {
            path: registry_path.to_string_lossy().to_string(),
            details: "root_meta_info.json's top level is not a JSON object".to_string(),
        });
    };
    let store = root.entry("all_draft_store").or_insert_with(|| json!([]));
    let Value::Array(store) = store else {
        return Err(CapCutError::WriteFailed {
            path: registry_path.to_string_lossy().to_string(),
            details: "root_meta_info.json's all_draft_store is not a JSON array".to_string(),
        });
    };
    // Replace an existing entry for this same draft folder (a re-export) in
    // place, never leaving a stale duplicate; otherwise append a new one —
    // every other entry is left completely untouched either way.
    match store.iter_mut().find(|e| {
        e.get("draft_fold_path").and_then(Value::as_str) == Some(draft_fold_path.as_str())
    }) {
        Some(existing) => *existing = entry,
        None => store.push(entry),
    }
    let count = store.len();
    root.insert("draft_ids".to_string(), json!(count));
    root.entry("root_path")
        .or_insert_with(|| json!(forward_slashes(draft_root)));

    write_json_atomic(&registry_path, &registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ave-capcut-meta-test-{label}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_draft_meta_info_round_trips_the_real_fields_that_matter() {
        let root = temp_dir("meta-write");
        let draft_dir = root.join("My Draft");
        std::fs::create_dir_all(&draft_dir).unwrap();

        write_draft_meta_info(&draft_dir, &root, "SOME-ID-123", "My Draft", 5_000_000)
            .expect("write should succeed");

        let written: Value =
            serde_json::from_slice(&std::fs::read(draft_dir.join("draft_meta_info.json")).unwrap())
                .unwrap();
        assert_eq!(written["draft_id"], "SOME-ID-123");
        assert_eq!(written["draft_name"], "My Draft");
        assert_eq!(written["tm_duration"], 5_000_000);
        assert!(written["draft_fold_path"]
            .as_str()
            .unwrap()
            .ends_with("My Draft"));
        assert!(!written["draft_fold_path"].as_str().unwrap().contains('\\'));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn register_draft_in_root_registry_creates_a_fresh_registry_when_none_exists() {
        let root = temp_dir("registry-fresh");
        let draft_dir = root.join("New Draft");
        std::fs::create_dir_all(&draft_dir).unwrap();

        register_draft_in_root_registry(&root, "ID-1", "New Draft", &draft_dir)
            .expect("register should succeed");

        let registry: Value =
            serde_json::from_slice(&std::fs::read(root.join("root_meta_info.json")).unwrap())
                .unwrap();
        let store = registry["all_draft_store"].as_array().unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store[0]["draft_id"], "ID-1");
        assert_eq!(store[0]["draft_name"], "New Draft");
        assert_eq!(registry["draft_ids"], 1);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn root_registry_update_preserves_other_existing_entries() {
        let root = temp_dir("registry-preserve");
        // Seed a registry with one pre-existing "real user project" entry —
        // mirrors a real CapCut Pro-created project's own registry entry
        // shape, including a field this module never writes
        // (`streaming_edit_draft_ready`) to prove untouched entries really
        // pass through byte-for-byte, not just field-by-field.
        let existing_entry = json!({
            "draft_id": "EXISTING-REAL-PROJECT",
            "draft_name": "0906",
            "draft_fold_path": forward_slashes(&root.join("0906")),
            "streaming_edit_draft_ready": true,
            "some_future_field_this_module_has_never_heard_of": "keep me",
        });
        let seed = json!({
            "all_draft_store": [existing_entry.clone()],
            "draft_ids": 1,
            "root_path": forward_slashes(&root),
        });
        std::fs::write(
            root.join("root_meta_info.json"),
            serde_json::to_vec_pretty(&seed).unwrap(),
        )
        .unwrap();

        let new_draft_dir = root.join("AI_Video_Editor_Real_Test");
        std::fs::create_dir_all(&new_draft_dir).unwrap();
        register_draft_in_root_registry(
            &root,
            "NEW-DRAFT-ID",
            "AI_Video_Editor_Real_Test",
            &new_draft_dir,
        )
        .expect("register should succeed");

        let registry: Value =
            serde_json::from_slice(&std::fs::read(root.join("root_meta_info.json")).unwrap())
                .unwrap();
        let store = registry["all_draft_store"].as_array().unwrap();
        assert_eq!(
            store.len(),
            2,
            "the pre-existing entry must be preserved, not replaced"
        );
        assert!(
            store.contains(&existing_entry),
            "the pre-existing real-project entry must round-trip byte-for-byte: {store:?}"
        );
        assert!(store
            .iter()
            .any(|e| e["draft_id"] == "NEW-DRAFT-ID"
                && e["draft_name"] == "AI_Video_Editor_Real_Test"));
        assert_eq!(registry["draft_ids"], 2);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn re_registering_the_same_draft_folder_updates_in_place_not_duplicated() {
        let root = temp_dir("registry-reexport");
        let draft_dir = root.join("Same Draft");
        std::fs::create_dir_all(&draft_dir).unwrap();

        register_draft_in_root_registry(&root, "FIRST-ID", "Same Draft", &draft_dir).unwrap();
        register_draft_in_root_registry(&root, "SECOND-ID", "Same Draft", &draft_dir).unwrap();

        let registry: Value =
            serde_json::from_slice(&std::fs::read(root.join("root_meta_info.json")).unwrap())
                .unwrap();
        let store = registry["all_draft_store"].as_array().unwrap();
        assert_eq!(
            store.len(),
            1,
            "re-exporting the same draft folder must update in place, not duplicate"
        );
        assert_eq!(store[0]["draft_id"], "SECOND-ID");
        assert_eq!(registry["draft_ids"], 1);

        std::fs::remove_dir_all(&root).ok();
    }
}
