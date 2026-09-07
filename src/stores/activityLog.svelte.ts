// Real, backend-event-backed Activity/Log store (`promt.md` §15 "LOG /
// ACTIVITY PANEL", `STUDIO_PLAN.md` Phase D8b).
//
// §15's own worked example is entirely stage-transition lines:
//   10:21:03 Subtitle extraction completed
//   10:21:05 Translation started
//   10:21:17 Translation completed
//   10:21:18 Voice generation started
// This app has no subtitle-translation/TTS pipeline yet (that's a real,
// separate, unbuilt gap — `STUDIO_PLAN.md`'s own §10/§11 audit), so this
// store does not fabricate those specific lines. What it *does* have, real
// and already flowing today, is the exact same *shape* of signal: batch
// pipeline stage transitions. Every batch job already reports its own
// human-readable current stage (`BatchJob.stage`, e.g. `"Analyzing media"`,
// `"Transcribing"`, `"Removing silence"`, `"Generating captions"`,
// `"Editing complete"`, `"Rendering"`) over the real `batch:progress` Tauri
// event `stores/batch.svelte.ts` already listens to
// (`src-tauri/src/batch/manager.rs::BatchProgressEvent`) — and, for the
// exact same reason, a job's own terminal outcome already arrives as a real
// `stage` value too (`"Completed"`/`"Failed"`/`"Cancelled"`, set by
// `batch::manager::process_job`'s own terminal match arms). This store turns
// that one real, already-flowing stream into log lines — nothing here is
// invented, and no "system log" line is fabricated for an event that doesn't
// actually exist in this codebase yet.
//
// Granularity ("không spam popup cho mọi event" — §15's own explicit
// anti-spam instruction, which this store honors even though a log panel
// isn't a popup): a job's progress ticks *within* the same stage (e.g.
// 10%, 42%, 87% while still "Rendering") fire far more `batch:progress`
// events than there are real, meaningful steps. This store only appends a
// line the moment a job's own `stage` text actually *changes* — which is
// exactly the granularity §15's own example lines are at (one line per real
// step, not one line per percent).
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { listen } from "@tauri-apps/api/event";
import type { BatchProgressEvent } from "./batch.svelte";

export type ActivityLogLevel = "info" | "warning" | "error";

export interface ActivityLogEntry {
  id: string;
  timestampMs: number;
  level: ActivityLogLevel;
  message: string;
}

const BATCH_PROGRESS_EVENT = "batch:progress";

/** Session-local ring buffer, not a persisted audit log (task brief: this is
 * in-memory activity, `STUDIO_PLAN.md` Phase D8a's own `history`/
 * `in_progress_jobs` tables already own real persisted job records — this
 * store is deliberately not a second, redundant persistence layer for the
 * same data). Oldest entries drop once this cap is exceeded. */
const MAX_ENTRIES = 200;

class ActivityLogStore {
  /** Newest-first (a log/activity feed reads top-to-bottom like a chat
   * timeline — "what just happened" should never require scrolling down). */
  entries = $state<ActivityLogEntry[]>([]);
  levelFilter = $state<"all" | ActivityLogLevel>("all");
  /** Collapsed by default (task brief: "collapsible", explicitly not meant
   * to be open/intrusive by default). */
  collapsed = $state(true);

  /** job id -> the last `stage` string logged for it. Plain, non-reactive
   * bookkeeping (never itself rendered) used only to detect a real stage
   * change — deliberately not `$state`, since re-rendering on every write to
   * this map would be pure overhead. */
  private lastStageByJob = new Map<string, string>();
  private nextEntryId = 0;

  constructor() {
    // Fire-and-forget, matching `stores/batch.svelte.ts`'s own
    // `BATCH_PROGRESS_EVENT` listener pattern exactly — registered once at
    // module load, independently of `batchStore`'s own listener (Tauri
    // events support multiple independent listeners on the same event; this
    // store doesn't need any of `batchStore`'s own job-table bookkeeping, so
    // it listens directly rather than routing through that store).
    void listen<BatchProgressEvent>(BATCH_PROGRESS_EVENT, (event) => {
      this.handleBatchProgress(event.payload);
    });
  }

  private handleBatchProgress(payload: BatchProgressEvent): void {
    const { job } = payload;
    const previousStage = this.lastStageByJob.get(job.id);
    if (previousStage === job.stage) return;
    this.lastStageByJob.set(job.id, job.stage);

    // Terminal outcomes get a level that matches severity — everything else
    // (every in-progress stage transition, `Paused` included) is plain
    // informational activity. `Cancelled` is `warning`, not `error`: it's a
    // deliberate interruption, not a pipeline failure.
    if (job.status === "failed") {
      this.append("error", job.error ? `${job.name}: ${job.stage} — ${job.error}` : `${job.name}: ${job.stage}`);
    } else if (job.status === "cancelled") {
      this.append("warning", `${job.name}: ${job.stage}`);
    } else {
      this.append("info", `${job.name}: ${job.stage}`);
    }
  }

  private append(level: ActivityLogLevel, message: string): void {
    this.nextEntryId += 1;
    const entry: ActivityLogEntry = {
      id: `activity-${this.nextEntryId}`,
      timestampMs: Date.now(),
      level,
      message,
    };
    this.entries = [entry, ...this.entries].slice(0, MAX_ENTRIES);
  }

  filteredEntries = $derived.by((): ActivityLogEntry[] => {
    if (this.levelFilter === "all") return this.entries;
    return this.entries.filter((e) => e.level === this.levelFilter);
  });

  /** Real counts over the *entire* retained buffer (not just the current
   * filter) — used for the collapsed header's own at-a-glance badges, so a
   * collapsed panel still honestly shows "there are 2 errors in here"
   * without the user having to expand it first. */
  counts = $derived.by((): Record<ActivityLogLevel, number> => {
    const counts: Record<ActivityLogLevel, number> = { info: 0, warning: 0, error: 0 };
    for (const e of this.entries) counts[e.level] += 1;
    return counts;
  });

  setLevelFilter(level: "all" | ActivityLogLevel): void {
    this.levelFilter = level;
  }

  toggleCollapsed(): void {
    this.collapsed = !this.collapsed;
  }

  clear(): void {
    this.entries = [];
  }
}

export const activityLogStore = new ActivityLogStore();
