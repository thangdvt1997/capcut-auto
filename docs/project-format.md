# Project Format — `project.json`

The unified project file is the application's own format and the single source of truth. CapCut/Jianying drafts, FCPXML, and rendered MP4/WebM are all **export targets** generated from it — never the other way around (master prompt §5, §68, §81).

## Design decisions

- **Timebase: `i64` microseconds everywhere**, matching `pyJianYingDraft`'s `time_util.SEC = 1_000_000` convention (see `docs/architecture-audit.md` §1). This is a deliberate choice, not an accident: capcut-mate's draft engine already speaks this unit natively (zero conversion at the CapCut export boundary), and integer microseconds avoid the float-drift problems documented in autocut's `timecode.rs` (master prompt §67). FFmpeg (seconds) and FCPXML (rational frames) get conversion at their respective adapter boundaries only — never in the core model.
- **Stable IDs, not array positions.** Every timeline entity (`Track`, `Clip`, `Caption`, `Effect`, `Animation`, `Keyframe`, `Marker`) carries a `String` UUID v4 `id`. References between entities (e.g. a `Clip.media_id` pointing at a `MediaItem`) are by ID. Reordering or deleting one array element never invalidates a reference held elsewhere (master prompt §5, §10).
- **Non-destructive.** `project.json` never stores rendered pixels/audio — only edit instructions referencing original source files by path/ID. Source media is never mutated (master prompt §68).
- **Versioned with an explicit migration layer** (`ProjectV1` now; `ProjectV2`+ later) so old projects never break on app upgrade (master prompt §5).

## Schema (`ProjectV1`)

```jsonc
{
  "version": 1,
  "project": {
    "id": "uuid",
    "name": "string",
    "created_at": "RFC3339 timestamp",
    "modified_at": "RFC3339 timestamp",
    "app_version": "semver string that wrote this file"
  },
  "canvas": {
    "width": 1920,
    "height": 1080,
    "fps": { "num": 30000, "den": 1001 },   // rational, avoids NTSC float drift
    "ratio_preset": "16:9 | 9:16 | 1:1 | 4:5 | custom"
  },
  "media": [
    {
      "id": "uuid",
      "kind": "video | audio | image",
      "source_path": "absolute or project-relative path",
      "duration_us": 123456789,             // i64 microseconds
      "width": 1920, "height": 1080,
      "fps": { "num": 30000, "den": 1001 },
      "codec": "string", "bitrate": 0,
      "audio_channels": 2, "sample_rate": 48000,
      "rotation_deg": 0,
      "created_at": "RFC3339 or null",
      "proxy_path": "path or null",
      "thumbnail_path": "path or null"
    }
  ],
  "tracks": [
    {
      "id": "uuid",
      "kind": "video | audio | caption | image | overlay | effect",
      "name": "string",
      "render_index": 0,          // stacking order, higher draws on top (pyJianYingDraft convention)
      "locked": false, "hidden": false, "muted": false, "solo": false,
      "clip_ids": ["uuid", "..."]  // ordered; clips themselves live in `clips` keyed by id
    }
  ],
  "clips": [
    {
      "id": "uuid",
      "track_id": "uuid",
      "media_id": "uuid | null",        // null for e.g. a pure-effect or generated-caption clip
      "source_in_us": 0,                // trim into source
      "source_out_us": 5000000,
      "position_us": 0,                 // placement on the track's timeline
      "speed": 1.0,
      "enabled": true,
      "group_id": "uuid | null",        // SyncGroup membership, see below
      "clip_settings": {
        "opacity": 1.0, "flip_h": false, "flip_v": false,
        "rotation_deg": 0.0,
        "scale_x": 1.0, "scale_y": 1.0,
        "transform_x": 0.0, "transform_y": 0.0
      }
    }
  ],
  "captions": [
    {
      "id": "uuid", "track_id": "uuid",
      "start_us": 0, "end_us": 2000000,
      "text": "string",
      // Sorted, non-overlapping by construction (every producer in
      // `captions::generate`/`timeline::captions` builds this in time
      // order) — the frontend's active-word-at-time-T lookup (master
      // prompt §27) can binary-search this by start_us/end_us in O(log n).
      "words": [ { "text": "string", "start_us": 0, "end_us": 300000, "confidence": 0.94 } ],
      "style_id": "uuid | null"           // references `caption_styles[].id` below
    }
  ],
  "caption_styles": [
    // Phase 8, master prompt §26. Additive field — empty for any project
    // saved before it existed. Built-in templates (Minimal/TikTok/Podcast/
    // News/Gaming/Karaoke, `captions::styles::all_caption_templates`) are a
    // separate catalog, not auto-copied in here (same relationship
    // `RenderPreset` has to a project's own `RenderSettings`).
    {
      "id": "uuid", "name": "string",
      "font_family": "string", "font_size": 32.0,
      "bold": false, "italic": false,
      "alignment": "left | center | right",
      "position": { "anchor": "top | center | bottom", "offset_x": 0.0, "offset_y": 0.0 }, // half-canvas-units, same convention as clip_settings.transform_x/y
      "text_color": { "r": 1.0, "g": 1.0, "b": 1.0 },
      "background": null,                 // or { "color": {...}, "opacity": 0.6 }
      "outline": null,                     // or { "color": {...}, "width": 0.08 } — width is a font-size fraction
      "shadow": null,                      // or { "color": {...}, "opacity": 0.9, "offset_x": 0.0, "offset_y": 0.02, "blur": 15.0 }
      "opacity": 1.0,
      "safe_margins": { "top": 0.05, "bottom": 0.05, "left": 0.05, "right": 0.05 } // fraction of canvas inset from each edge
    }
  ],
  "transcript": [
    // "words" (Phase 7, master prompt §14 "Prefer word-level timestamps") mirrors
    // `captions[].words` above — empty for entries with only segment-level timing.
    { "id": "uuid", "media_id": "uuid", "text": "string", "start_us": 0, "end_us": 300000, "confidence": 0.94,
      "words": [ { "text": "string", "start_us": 0, "end_us": 300000, "confidence": 0.94 } ],
      "is_filler": false }
  ],
  "effects": [ { "id": "uuid", "clip_id": "uuid", "kind": "string", "params": {} } ],
  "animations": [ { "id": "uuid", "clip_id": "uuid", "kind": "in | out | loop | group", "name": "string", "duration_us": 500000 } ],
  "keyframes": [ { "id": "uuid", "clip_id": "uuid", "property": "position_x | position_y | rotation | scale | alpha | volume | ...", "time_offset_us": 0, "value": 0.0, "curve": "linear" } ],
  "cuts": [
    // Edit-plan / silence-removal provenance, NOT a duplicate timeline: records *why* clips
    // were split/removed by an automated pass (VAD, AI EditPlan), for undo/audit/re-analysis.
    { "id": "uuid", "kind": "remove | keep", "source_media_id": "uuid", "start_us": 0, "end_us": 500000, "reason": "silence | filler_word | ai_suggested", "applied": true }
  ],
  "ai": {
    "provider_settings_ref": "opaque id — actual credentials live in Windows Credential Manager, never here",
    // Phase 10: a real, strictly-typed EditPlan (src-tauri/src/ai/edit_plan.rs), not opaque JSON.
    // `operations[].type` is a closed enum — "remove" (real, applies via the existing Cut/timeline
    // machinery) and "zoom" (structural-only in Phase 10; no keyframe-authoring UI exists yet to
    // apply it for real — see that module's doc comment).
    "last_edit_plan": null,
    // {
    //   "version": 1,
    //   "operations": [
    //     { "type": "remove", "start_us": 12300000, "end_us": 15700000, "reason": "long pause", "confidence": 0.95 },
    //     { "type": "zoom", "start_us": 32000000, "end_us": 36000000, "scale": 1.12, "reason": "emphasis" }
    //   ]
    // }
    "highlights": []              // still opaque JSON — real Highlight type is a follow-up pass's job
  },
  "export": {
    "last_render_preset": "string or null",
    "last_capcut_draft_path": "string or null"
  }
}
```

### `SyncGroup` (linked tracks)

Generalizes autocut's fixed-camera-rig "shared cutlist + per-track offset" concept (audit §4) into a first-class, optional relationship instead of a hard-coded model:

```jsonc
{
  "id": "uuid",
  "clip_ids": ["uuid", "uuid"],   // clips that must move/trim/cut together
  "offsets_us": { "clip-uuid": 0, "clip-uuid-2": -120000 } // relative alignment
}
```

A clip's `group_id` (in the `clips` schema above) points at a `SyncGroup`; the timeline engine propagates split/trim/delete operations across all members of a group unless the user explicitly ungroups. Stored top-level in `project.json` as `"sync_groups": [...]` (omitted above for brevity, added when Phase 4 implements it).

## Migration layer

- `ProjectV1` is the only schema version today. `version: 1` is written by every save.
- The Rust loader dispatches on `version`: `1 => ProjectV1`, with a `migrate_to_latest(json) -> ProjectV1` function that is a no-op today but establishes the pattern — future `ProjectV2` gets a `migrate_v1_to_v2` step chained the same way, so old project files never fail to open after an app upgrade.
- Atomic writes: `project.json.tmp` → `fsync` → rename over `project.json` (master prompt §6). Keep the last N autosave/recovery snapshots alongside (`project.json.bak.<timestamp>`), pruned by count.

## Error model

Standardized Rust error enum, one variant per subsystem (master prompt §56), each carrying `code`, `message`, `details`, `recoverable: bool`, `suggested_action: Option<String>` when serialized to the frontend:

`MediaError`, `FfmpegError`, `TranscriptionError`, `AiProviderError`, `CapCutError`, `ProjectError`, `RenderError`, `ModelError`.

`ProjectError` covers this file's own failure modes specifically: `SchemaVersionTooNew`, `CorruptJson`, `MigrationFailed`, `AtomicWriteFailed`, `RecoverySnapshotFound` (surfaced at startup per master prompt §86).

`TranscriptionError` (Phase 7, `src-tauri/src/transcription/error.rs`) covers the Whisper transcription pipeline itself: model not installed, model load failure, unsupported input sample rate, inference failure, cancellation. `ModelError` (same file) covers the separate Model Manager concern — catalog lookups, storage-directory resolution, download failure/cancellation/verification, delete-when-not-installed.

`AiProviderError` (Phase 10, `src-tauri/src/ai/error.rs`) covers talking to an LLM backend: request/transport failure, non-2xx HTTP responses, unparseable responses, a missing API key, and secure-credential-store failures. `EditPlanError` (same file) is the separate concern of validating an `EditPlan` (Phase 10, `src-tauri/src/ai/edit_plan.rs`) — malformed JSON (including an unknown/closed-enum-rejected operation `type`), an unsupported schema version, or an out-of-range operation field (negative/inverted time range, invalid `scale`, out-of-range `confidence`).
