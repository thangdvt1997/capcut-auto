// Svelte 5 runes-based timeline store (pattern informed by autocut's
// `store.svelte.ts` session-guard design, reimplemented not copied per
// audit §7/§2 — same house style as `stores/media.svelte.ts`: a plain class
// using `$state` fields directly, async methods calling `invoke` via the
// generated `commands` object and folding the returned `ProjectV1` back
// into state so callers never have to separately re-fetch).
//
// Single source of truth for the Phase 4 timeline UI: the current project
// (owned by the Rust-side `TimelineSession`, mirrored here), the clip
// multi-selection, the playhead (i64 microseconds — never float seconds,
// per the project's mandated timebase, master prompt §10/§67), zoom,
// horizontal scroll, and session-local markers.

import { commands } from "../types/bindings";
import type { AppErrorPayload, Clip, MediaItem, ProjectV1, Result, Track, TrackKind, TranscriptEntry } from "../types/bindings";
import {
  clampZoom,
  clipContainsUs,
  clipEndUs,
  collectSnapCandidates,
  DEFAULT_PX_PER_SECOND,
  DEFAULT_SNAP_THRESHOLD_US,
  projectDurationUs,
  ZOOM_STEP_FACTOR,
  type PxPerSecond,
  type Us,
} from "../timeline/algebra";

/**
 * Plain-object copy of a Svelte 5 `$state` reactive value, safe to hand to
 * a non-Svelte-aware API (`structuredClone`, `JSON.stringify`, an IPC call).
 * Wrapped behind an opaque `T => T` generic rather than calling
 * `$state.snapshot` directly at each use site: TS's `Snapshot<T>` mapped
 * type recurses into every nested field, and on `ProjectV1` (whose
 * `serde_json::Value` fields are themselves a recursive `JsonValue` type)
 * that blows up with "Type instantiation is excessively deep and possibly
 * infinite". A generic function body is checked against the abstract `T`
 * without expanding it, so the recursion never happens here.
 */
function snap<T>(value: T): T {
  return $state.snapshot(value) as T;
}

/**
 * Frontend-only, session-local timeline marker. `docs/project-format.md`'s
 * `ProjectV1` schema has no `markers` field — the Phase 4 backend agent's
 * pass worked from that schema and did not add one, and adding a new
 * persisted field now is a schema-migration decision out of this pass's
 * scope (see `IMPLEMENTATION_PLAN.md` Phase 4 notes). Markers therefore
 * live only in this store's memory: they do **not** round-trip through
 * `save`/`load_timeline_project`/`project.json`, and are lost on reload or
 * app restart. A later phase that wants persisted markers needs a real
 * `ProjectV2` migration, not a silent field bolted on here.
 */
export interface TimelineMarker {
  id: string;
  time_us: Us;
  label: string;
}

function randomId(): string {
  // `crypto.randomUUID` is available in the Tauri/WebView2 webview (secure
  // context); markers are session-local only (see doc comment above) so no
  // fallback beyond this is needed.
  return crypto.randomUUID();
}

function makeDefaultTrack(kind: TrackKind, name: string, renderIndex: number): Track {
  return {
    id: randomId(),
    kind,
    name,
    render_index: renderIndex,
    locked: false,
    hidden: false,
    muted: false,
    solo: false,
    clip_ids: [],
  };
}

/**
 * `commands::project::new_project` (Phase 2 scope) intentionally starts a
 * project with zero tracks, and there is no "add track" command yet either
 * (track CRUD beyond the lock/hide/mute/solo flags isn't in this Phase 4
 * pass's checklist). Without *some* starting tracks the real timeline UI
 * this pass builds would have nothing to render. This client-side default
 * — one Video/Audio/Caption track, matching the master-prompt §48 V1/A1/CC
 * shorthand the old static mockup showed — is applied only when a loaded
 * project has no tracks at all; a real saved project (Phase 6+ Project
 * Manager) always keeps whatever tracks it already has.
 */
function withDefaultTracksIfEmpty(project: ProjectV1): ProjectV1 {
  if (project.tracks.length > 0) return project;
  return {
    ...project,
    tracks: [
      makeDefaultTrack("video", "V1", 1),
      makeDefaultTrack("audio", "A1", 0),
      makeDefaultTrack("caption", "CC", 2),
    ],
  };
}

/**
 * Pure counterpart of `TimelineStore.addMediaAsClip` below, factored out
 * (Phase 10 follow-up, `stores/highlightDetection.svelte.ts`) so a caller
 * that is *not* mutating the live session — a synthetic single-clip project
 * built only to hand to `start_render_job` for a highlight's "Export clip"
 * action, or a freshly-created project being seeded for "Create new
 * project" before it's ever loaded into the session — can build the exact
 * same clip-append shape without going through `pushWholeProject`/IPC.
 * `addMediaAsClip` itself now just calls this and pushes the result.
 *
 * `sourceInUs`/`sourceOutUs` default to the whole media (unchanged
 * behavior); a caller passing a highlight's own `start_us`/`end_us` gets a
 * clip trimmed to exactly that span, positioned at the end of whatever's
 * already on the destination track (0 for a fresh project with none).
 */
export function projectWithMediaClip(
  project: ProjectV1,
  media: MediaItem,
  opts: { trackKind?: TrackKind; sourceInUs?: number; sourceOutUs?: number } = {},
): { project: ProjectV1; clip: Clip } {
  // Snapshot both inputs before touching them, regardless of whether the
  // caller already did — `structuredClone`/IPC both choke on a raw Svelte 5
  // `$state` proxy (see `snap()`'s own doc comment), and unlike
  // `addMediaAsClip` (which snaps its own `this.project`/`media` before
  // delegating here), external callers building a synthetic/fresh project
  // (`stores/highlightDetection.svelte.ts`'s "Export clip"/"Create new
  // project", which resolve `media` from `timeline.mediaById` — a live,
  // reactive `MediaItem`) may not. `snap()` is idempotent on an
  // already-plain object, so this is always safe.
  media = snap(media);
  const kind: TrackKind = opts.trackKind ?? (media.kind === "audio" ? "audio" : "video");
  const sourceInUs = opts.sourceInUs ?? 0;
  const sourceOutUs = opts.sourceOutUs ?? media.duration_us;
  const next: ProjectV1 = structuredClone(snap(project));

  let track = next.tracks.find((t) => t.kind === kind && !t.locked);
  if (!track) {
    track = makeDefaultTrack(kind, kind === "audio" ? "A1" : kind === "caption" ? "CC" : "V1", next.tracks.length);
    next.tracks.push(track);
  }

  const trackClips = next.clips.filter((c) => c.track_id === track!.id);
  const position = trackClips.reduce((max, c) => Math.max(max, clipEndUs(c)), 0);
  const clip: Clip = {
    id: randomId(),
    track_id: track.id,
    media_id: media.id,
    source_in_us: sourceInUs,
    source_out_us: sourceOutUs,
    position_us: position,
    speed: 1,
    enabled: true,
    group_id: null,
    clip_settings: {
      opacity: 1,
      flip_h: false,
      flip_v: false,
      rotation_deg: 0,
      scale_x: 1,
      scale_y: 1,
      transform_x: 0,
      transform_y: 0,
    },
  };
  next.clips.push(clip);
  track.clip_ids.push(clip.id);
  if (!next.media.some((m) => m.id === media.id)) next.media.push(media);

  return { project: next, clip };
}

class TimelineStore {
  project = $state<ProjectV1 | null>(null);
  loading = $state(false);
  lastError = $state<string | null>(null);

  selectedClipIds = $state<Set<string>>(new Set());
  playheadUs = $state<Us>(0);
  pxPerSecond = $state<PxPerSecond>(DEFAULT_PX_PER_SECOND);
  scrollLeftPx = $state(0);

  /** Set to `true` after a successful `copySelected()`; used only to
   * enable/disable the Paste affordance in the UI. The real clipboard
   * contents live entirely on the Rust side (`TimelineSession::clipboard`)
   * — this is not a mirror of that data, just a UI hint. */
  hasClipboardContent = $state(false);

  /** See `TimelineMarker` doc comment: session-local only, never persisted. */
  markers = $state<TimelineMarker[]>([]);

  /**
   * Bridge for the Space-key play/pause shortcut (master prompt §49).
   * `Timeline.svelte`'s keyboard handler and `VideoPlayer.svelte` are
   * siblings under different docked panels (`src/App.svelte`), so there's
   * no direct component reference between them; `VideoPlayer` registers its
   * own toggle function here on mount instead of the timeline reaching into
   * the DOM or a global.
   */
  previewApi: { togglePlayPause?: () => void } = $state({});

  tracks = $derived<Track[]>(this.project?.tracks ?? []);
  clips = $derived<Clip[]>(this.project?.clips ?? []);
  media = $derived<MediaItem[]>(this.project?.media ?? []);
  durationUs = $derived(this.project ? projectDurationUs(this.project) : 30_000_000);

  clipsByTrack = $derived.by(() => {
    const map = new Map<string, Clip[]>();
    for (const clip of this.clips) {
      const list = map.get(clip.track_id);
      if (list) list.push(clip);
      else map.set(clip.track_id, [clip]);
    }
    return map;
  });

  mediaById = $derived.by(() => new Map(this.media.map((m) => [m.id, m])));

  selectedClips = $derived(this.clips.filter((c) => this.selectedClipIds.has(c.id)));

  /** Keyed by track id: whether that track's audio is effectively muted
   * right now (direct mute, or another track's solo). Refreshed after every
   * project-mutating call so `TrackHeader.svelte` can show it without its
   * own IPC round trip. */
  effectiveMute = $state<Record<string, boolean>>({});

  /**
   * Preview-follows-timeline (Phase 3's `VideoPlayer.svelte` explicitly
   * deferred this to Phase 4 — see its own doc comment). The single video
   * clip under the playhead on a non-hidden, non-locked video track,
   * topmost by `render_index` when more than one qualifies. This is
   * **single-clip scrubbing only** — true multi-track compositing at the
   * playhead needs the render/compositing engine, which is Phase 6
   * (`RenderGraph`) scope and is deliberately not attempted here.
   */
  activeVideoTarget = $derived.by((): { media: MediaItem; sourceTimeUs: Us } | null => {
    const project = this.project;
    if (!project) return null;
    const playhead = this.playheadUs;
    let best: { clip: Clip; track: Track } | null = null;
    for (const track of project.tracks) {
      if (track.kind !== "video" || track.hidden || track.locked) continue;
      for (const clip of this.clipsByTrack.get(track.id) ?? []) {
        if (!clip.enabled) continue;
        if (playhead < clip.position_us || playhead >= clipEndUs(clip)) continue;
        if (!best || track.render_index > best.track.render_index) {
          best = { clip, track };
        }
      }
    }
    if (!best) return null;
    const media = this.mediaById.get(best.clip.media_id ?? "");
    if (!media || media.kind !== "video") return null;
    // Per the Phase 4 frontend brief's exact formula: source_in_us plus how
    // far the playhead has advanced past the clip's start on the timeline.
    // Converted to float seconds only at the <video> element boundary
    // (VideoPlayer.svelte), never here.
    const sourceTimeUs = best.clip.source_in_us + (playhead - best.clip.position_us);
    return { media, sourceTimeUs };
  });

  // -------------------------------------------------------------------
  // Result plumbing (same envelope convention as stores/media.svelte.ts)
  // -------------------------------------------------------------------

  private async run<T>(promise: Promise<Result<T, AppErrorPayload>>): Promise<T | null> {
    this.loading = true;
    try {
      const result = await promise;
      if (result.status === "ok") {
        this.lastError = null;
        return result.data;
      }
      this.lastError = result.error.message;
      return null;
    } finally {
      this.loading = false;
    }
  }

  private async applyProjectResult(promise: Promise<Result<ProjectV1, AppErrorPayload>>): Promise<void> {
    const project = await this.run(promise);
    if (project) {
      this.project = project;
      this.pruneSelection();
      void this.refreshEffectiveMute();
    }
  }

  /**
   * Public counterpart of `applyProjectResult` for mutations that originate
   * outside this store — Phase 5's `stores/silenceDetector.svelte.ts`
   * (`apply_silence_cuts`/`apply_silence_cuts_to_track`) and its sync-group
   * creation calls (`create_sync_group_manual`/`create_sync_group_by_timecode`).
   * Folds the resulting `ProjectV1` back into this store exactly like every
   * in-house mutation (so the main timeline immediately reflects it), while
   * still handing the caller its own success/error outcome to render in a
   * feature-local error slot rather than only this store's `lastError`.
   */
  async applyExternalProjectResult(
    promise: Promise<Result<ProjectV1, AppErrorPayload>>,
  ): Promise<{ ok: true } | { ok: false; error: string }> {
    const project = await this.run(promise);
    if (project) {
      this.project = project;
      this.pruneSelection();
      void this.refreshEffectiveMute();
      return { ok: true };
    }
    return { ok: false, error: this.lastError ?? "unknown error" };
  }

  private pruneSelection(): void {
    const ids = new Set(this.clips.map((c) => c.id));
    let changed = false;
    const next = new Set<string>();
    for (const id of this.selectedClipIds) {
      if (ids.has(id)) next.add(id);
      else changed = true;
    }
    if (changed) this.selectedClipIds = next;
  }

  async refreshEffectiveMute(): Promise<void> {
    const result = await commands.effectiveTrackMuteState();
    if (result.status === "ok") {
      this.effectiveMute = result.data as Record<string, boolean>;
    }
  }

  // -------------------------------------------------------------------
  // Session lifecycle
  // -------------------------------------------------------------------

  async loadProject(project: ProjectV1): Promise<void> {
    const seeded = withDefaultTracksIfEmpty(project);
    await this.pushWholeProject(seeded);
    this.selectedClipIds = new Set();
    this.playheadUs = 0;
    this.markers = [];
  }

  async refresh(): Promise<void> {
    await this.applyProjectResult(commands.getTimelineProject());
  }

  /** Replaces the entire session project via `load_timeline_project` without
   * resetting selection/playhead/markers — the shared plumbing behind both
   * `loadProject` (a real "open project" — which *does* reset that UI
   * state) and `addMediaAsClip` (an in-place edit that shouldn't). */
  private async pushWholeProject(project: ProjectV1): Promise<void> {
    await commands.loadTimelineProject(project);
    this.project = project;
    this.pruneSelection();
    await this.refreshEffectiveMute();
  }

  /**
   * Bridges a real gap in the Phase 4 backend's exposed command surface:
   * there is no `insert_clip`/`add_clip` command, only `duplicate_clip`
   * (which needs an *existing* clip to copy) and the clip-editing
   * primitives (split/trim/move/delete). Building a timeline's initial
   * content from Media Library items is Project Manager-shaped work that
   * hasn't landed yet (Phase 5/6), so `MediaLibrary.svelte`'s "Add to
   * Timeline" action does it here instead: append a new clip (and, if
   * needed, a destination track) to a cloned `ProjectV1` and push the whole
   * thing back through `load_timeline_project` — the same command a real
   * "open project" flow will eventually use.
   *
   * Documented limitation: unlike every other mutation in this store, this
   * is **not** an undo-able timeline command (there is no backend primitive
   * for it to wrap) — it replaces the session project directly, the same
   * as opening a project would.
   *
   * `sourceInUs`/`sourceOutUs` (Phase 10 follow-up): optionally trim the
   * appended clip to a sub-range of `media` instead of its whole duration —
   * added for `stores/highlightDetection.svelte.ts`'s "Add to timeline"
   * action (a detected highlight's `start_us`/`end_us`), reusing this exact
   * bridge rather than a second one. See `projectWithMediaClip` (the pure
   * half of this method, factored out above) for the actual clip-append
   * logic.
   */
  async addMediaAsClip(
    media: MediaItem,
    opts: { trackKind?: TrackKind; sourceInUs?: number; sourceOutUs?: number } = {},
  ): Promise<void> {
    const current = this.project;
    if (!current) return;
    // `projectWithMediaClip` snaps both its `project`/`media` arguments
    // itself now (its own doc comment: both are Svelte-5-reactive `$state`
    // proxies, which `structuredClone()`/IPC choke on directly) — no need
    // to pre-snapshot here too.
    const { project: next } = projectWithMediaClip(current, media, opts);
    await this.pushWholeProject(next);
  }

  // -------------------------------------------------------------------
  // Transcript (Phase 7, master prompt §15/§16) — same "no dedicated backend
  // primitive, so clone + `pushWholeProject`" bridge `addMediaAsClip` above
  // already uses: there is no `update_transcript_entry`/`add_transcript_entries`
  // command (`src-tauri/src/commands/transcription.rs` only exposes
  // `transcribe_media`/`cancel_transcription`/`detect_filler_words` — the
  // actual `ProjectV1::transcript` array is a plain data field the frontend
  // owns entirely once populated). Both methods below are therefore, like
  // `addMediaAsClip`, **not** undo-able timeline commands.
  // -------------------------------------------------------------------

  /** Merges a freshly-transcribed `Vec<TranscriptEntry>` (from the final
   * `transcription:progress` event, `done: true`) into the project, replacing
   * any previous entries for the same `mediaId` (a re-transcribe of a media
   * item supersedes its old transcript rather than duplicating it). */
  async replaceTranscriptForMedia(mediaId: string, entries: TranscriptEntry[]): Promise<void> {
    if (!this.project) return;
    const project: ProjectV1 = structuredClone(snap(this.project));
    project.transcript = project.transcript.filter((e) => e.media_id !== mediaId).concat(entries);
    await this.pushWholeProject(project);
  }

  /** "Transcript Text Edit" mode's own affordance (`TranscriptEditor.svelte`):
   * corrects one entry's `text` in place. Deliberately does not touch
   * `words`/`start_us`/`end_us` or any clip/track — per master prompt §15
   * this mode is purely cosmetic/corrective and must never itself produce a
   * timeline edit. */
  async updateTranscriptEntryText(entryId: string, text: string): Promise<void> {
    if (!this.project) return;
    const project: ProjectV1 = structuredClone(snap(this.project));
    const entry = project.transcript.find((e) => e.id === entryId);
    if (!entry || entry.text === text) return;
    entry.text = text;
    await this.pushWholeProject(project);
  }

  // -------------------------------------------------------------------
  // Selection
  // -------------------------------------------------------------------

  selectClip(id: string, opts: { additive?: boolean } = {}): void {
    if (opts.additive) {
      const next = new Set(this.selectedClipIds);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      this.selectedClipIds = next;
    } else {
      this.selectedClipIds = new Set([id]);
    }
  }

  setSelection(ids: string[]): void {
    this.selectedClipIds = new Set(ids);
  }

  clearSelection(): void {
    this.selectedClipIds = new Set();
  }

  isSelected(id: string): boolean {
    return this.selectedClipIds.has(id);
  }

  // -------------------------------------------------------------------
  // Playhead / zoom / scroll
  // -------------------------------------------------------------------

  setPlayhead(us: Us): void {
    this.playheadUs = Math.max(0, Math.round(us));
  }

  seekBy(deltaUs: Us): void {
    this.setPlayhead(this.playheadUs + deltaUs);
  }

  setZoom(pxPerSecond: PxPerSecond): void {
    this.pxPerSecond = clampZoom(pxPerSecond);
  }

  zoomIn(): void {
    this.setZoom(this.pxPerSecond * ZOOM_STEP_FACTOR);
  }

  zoomOut(): void {
    this.setZoom(this.pxPerSecond / ZOOM_STEP_FACTOR);
  }

  setScrollLeft(px: number): void {
    this.scrollLeftPx = Math.max(0, px);
  }

  // -------------------------------------------------------------------
  // Snap
  // -------------------------------------------------------------------

  /** Candidate list for a drag/trim of `excludeClipId` — the frontend's
   * side of `commands::timeline::snap_to_candidates`'s contract (it's a
   * pure nearest-within-threshold query; the caller supplies candidates). */
  snapCandidatesFor(excludeClipId?: string): Us[] {
    return collectSnapCandidates(this.clips, this.playheadUs, excludeClipId);
  }

  async snap(targetUs: Us, excludeClipId?: string, thresholdUs: Us = DEFAULT_SNAP_THRESHOLD_US): Promise<Us> {
    const candidates = this.snapCandidatesFor(excludeClipId);
    const snapped = await commands.snapToCandidates(targetUs, candidates, thresholdUs);
    return snapped ?? targetUs;
  }

  // -------------------------------------------------------------------
  // Clip operations
  // -------------------------------------------------------------------

  async splitClip(clipId: string, atUs: Us): Promise<void> {
    await this.applyProjectResult(commands.splitClip(clipId, Math.round(atUs)));
  }

  /** `S` key / double-click (master prompt §49): splits every selected
   * clip at the playhead, or — when nothing is selected — every clip on an
   * unlocked track whose span contains the playhead. Each split is its own
   * backend command/undo step (there is no batch-split primitive on the
   * backend, unlike `delete_clips`), so splitting several clips at once
   * takes several undo presses to fully reverse; documented here rather
   * than silently pretended to be atomic. */
  async splitAtPlayhead(): Promise<void> {
    const at = this.playheadUs;
    const targets =
      this.selectedClipIds.size > 0
        ? this.selectedClips.filter((c) => clipContainsUs(c, at))
        : this.clips.filter((c) => {
            const track = this.tracks.find((t) => t.id === c.track_id);
            return !!track && !track.locked && clipContainsUs(c, at);
          });
    for (const clip of targets) {
      await this.splitClip(clip.id, at);
    }
  }

  async trimClipStart(clipId: string, newStartUs: Us): Promise<void> {
    await this.applyProjectResult(commands.trimClipStart(clipId, Math.round(newStartUs)));
  }

  async trimClipEnd(clipId: string, newEndUs: Us): Promise<void> {
    await this.applyProjectResult(commands.trimClipEnd(clipId, Math.round(newEndUs)));
  }

  async moveClip(clipId: string, targetTrackId: string, newPositionUs: Us): Promise<void> {
    await this.applyProjectResult(commands.moveClip(clipId, targetTrackId, Math.round(newPositionUs)));
  }

  async deleteClip(clipId: string): Promise<void> {
    await this.applyProjectResult(commands.deleteClip(clipId));
  }

  /** Delete key (master prompt §49): the whole multi-selection as one
   * atomic undo step, via the backend's batch `delete_clips` primitive. */
  async deleteSelected(): Promise<void> {
    if (this.selectedClipIds.size === 0) return;
    await this.applyProjectResult(commands.deleteClips(Array.from(this.selectedClipIds)));
    this.selectedClipIds = new Set();
  }

  async duplicateClip(clipId: string, newPositionUs: Us, targetTrackId?: string | null): Promise<void> {
    await this.applyProjectResult(commands.duplicateClip(clipId, Math.round(newPositionUs), targetTrackId ?? null));
  }

  // -------------------------------------------------------------------
  // Track flags
  // -------------------------------------------------------------------

  async setTrackLocked(trackId: string, locked: boolean): Promise<void> {
    await this.applyProjectResult(commands.setTrackLocked(trackId, locked));
  }

  async setTrackHidden(trackId: string, hidden: boolean): Promise<void> {
    await this.applyProjectResult(commands.setTrackHidden(trackId, hidden));
  }

  async setTrackMuted(trackId: string, muted: boolean): Promise<void> {
    await this.applyProjectResult(commands.setTrackMuted(trackId, muted));
  }

  async setTrackSolo(trackId: string, solo: boolean): Promise<void> {
    await this.applyProjectResult(commands.setTrackSolo(trackId, solo));
  }

  // -------------------------------------------------------------------
  // Undo / redo
  // -------------------------------------------------------------------

  async undo(): Promise<void> {
    await this.applyProjectResult(commands.undoTimeline());
  }

  async redo(): Promise<void> {
    await this.applyProjectResult(commands.redoTimeline());
  }

  // -------------------------------------------------------------------
  // Copy / paste
  // -------------------------------------------------------------------

  async copySelected(): Promise<void> {
    if (this.selectedClipIds.size === 0) return;
    // `copy_clips` returns `()` on success, so `run()`'s T is `null` either
    // way — `lastError` (set by `run()` itself) is what actually
    // distinguishes success from failure here.
    await this.run(commands.copyClips(Array.from(this.selectedClipIds)));
    if (this.lastError === null) this.hasClipboardContent = true;
  }

  /** Pastes at the playhead, keeping each clip on the track it was copied
   * from (backend default when `target_track_id` is `null`) — see
   * `timeline::clipboard::paste_clips`'s doc comment. */
  async paste(): Promise<void> {
    await this.applyProjectResult(commands.pasteClips(null, this.playheadUs));
  }

  // -------------------------------------------------------------------
  // Multi-track sync (master prompt §39/§40) — grouping UI lives in
  // `components/timeline/SyncGroupDialog.svelte`; this store only wraps the
  // two backend commands so that dialog (and any future caller) folds the
  // result back into the shared project the same way every other mutation
  // here does.
  // -------------------------------------------------------------------

  /** Best-effort alignment from each clip's embedded `MediaItem::created_at`
   * timestamp. Fails (returned, not thrown) with `TIMELINE_TIMECODE_UNAVAILABLE`
   * when any involved clip's media lacks one — the caller's fallback is
   * `createSyncGroupManual`. */
  async createSyncGroupByTimecode(clipIds: string[]): Promise<{ ok: true } | { ok: false; error: string }> {
    return this.applyExternalProjectResult(commands.createSyncGroupByTimecode(clipIds));
  }

  /** Manual fallback: caller supplies one offset (microseconds) per clip id. */
  async createSyncGroupManual(
    clipIds: string[],
    offsetsUs: Record<string, number>,
  ): Promise<{ ok: true } | { ok: false; error: string }> {
    return this.applyExternalProjectResult(commands.createSyncGroupManual(clipIds, offsetsUs));
  }

  // -------------------------------------------------------------------
  // Markers (session-local — see `TimelineMarker` doc comment)
  // -------------------------------------------------------------------

  addMarker(timeUs: Us, label = ""): TimelineMarker {
    const marker: TimelineMarker = { id: randomId(), time_us: Math.round(timeUs), label };
    this.markers = [...this.markers, marker].sort((a, b) => a.time_us - b.time_us);
    return marker;
  }

  removeMarker(id: string): void {
    this.markers = this.markers.filter((m) => m.id !== id);
  }

  renameMarker(id: string, label: string): void {
    this.markers = this.markers.map((m) => (m.id === id ? { ...m, label } : m));
  }
}

export const timeline = new TimelineStore();
