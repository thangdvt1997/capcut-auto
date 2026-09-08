// Svelte 5 runes-based store for the Batch Jobs UI (master prompt §42/§43).
// The backend `BatchJobManager`/pipeline/Tauri command surface already
// shipped in an earlier Phase 11 pass (`src-tauri/src/batch/`) — this store
// is the follow-up frontend pass building the Jobs table on top of it.
//
// Structurally mirrors `stores/render.svelte.ts` (the freshest "kick off a
// background job, listen for a named progress event keyed by id, allow
// cancel" store in this codebase): start a batch -> per-job progress arrives
// via the `batch:progress` Tauri event -> pause/resume/cancel/retry per row
// wired to the real commands. Per the task brief, jobs are tracked the same
// way `render.svelte.ts`'s `progressByJob` tracks every job id, not just the
// last one this session started — a batch's Jobs table must reflect every
// job in it, and `list_batch_jobs` can itself return jobs whose individual
// progress events this store already received out of band.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { listen } from "@tauri-apps/api/event";
import { commands } from "../types/bindings";
import type {
  BatchJob,
  BatchJobStatus,
  BatchPipelineConfig,
  Result,
  AppErrorPayload,
  MaxConcurrentJobsSetting,
  WorkerPoolStatus,
} from "../types/bindings";

/** `BatchJobStatus` values a job never leaves once reached — it will never
 * claim or hold a worker slot again. Everything else (`queued`, the four
 * in-progress stages, `paused`) still occupies, or is waiting for, a
 * worker slot, so `get_worker_pool_status`'s numbers are only worth
 * re-fetching while at least one known job isn't yet terminal (Phase D4's
 * own "poll only while there's reason to" requirement). */
const TERMINAL_JOB_STATUSES = new Set<BatchJobStatus>(["completed", "failed", "cancelled"]);

/**
 * Payload of the `batch:progress` Tauri event
 * (`src-tauri/src/batch/manager.rs::BatchProgressEvent`). Hand-written
 * rather than specta-generated — this `tauri-specta` `Builder` only
 * registers *commands*, not typed events (see `stores/render.svelte.ts`'s
 * `RenderProgressEvent` doc comment for the full rationale). Keep in sync
 * with the Rust struct by hand.
 */
export interface BatchProgressEvent {
  batch_id: string;
  job: BatchJob;
}

const BATCH_PROGRESS_EVENT = "batch:progress";

/** One batch started this session — purely for the "which batch am I
 * looking at" picker. There is no backend "list all batches ever started"
 * command (`list_batch_jobs` needs a batch id the caller already has), so
 * this session's own memory of ids it started is the only inventory there
 * is; a batch started in a previous app session is not recoverable here. */
export interface BatchSummary {
  id: string;
  createdAtMs: number;
  fileCount: number;
}

type SimpleResult = Result<null, AppErrorPayload>;

class BatchStore {
  jobsDialogOpen = $state(false);
  startDialogOpen = $state(false);

  batches = $state<BatchSummary[]>([]);
  selectedBatchId = $state<string | null>(null);

  /** Flat, keyed by job id across every batch this session knows about —
   * the same "reflect ALL jobs, not just the last one started" shape
   * `stores/render.svelte.ts`'s `progressByJob` already established. */
  jobsById = $state<Record<string, BatchJob>>({});
  /** batch id -> ordered job ids (stable row order), seeded once from
   * `list_batch_jobs` right after `start_batch` returns and extended by any
   * `batch:progress` event for a job id not already known. */
  batchJobIds = $state<Record<string, string[]>>({});

  starting = $state(false);
  startError = $state<string | null>(null);

  actionErrorByJob = $state<Record<string, string | null>>({});
  actionPendingByJob = $state<Record<string, boolean>>({});
  /** Two-step confirm for Cancel (task brief: lighter than Model Manager's
   * delete confirmation, but stopping a long-running batch job mid-flight
   * is still disruptive enough to want one deliberate extra click) — same
   * "click to arm, click again to confirm" shape as
   * `stores/modelManager.svelte.ts`'s `pendingDeleteId`, scoped per job id
   * rather than one global flag since several rows could be mid-processing
   * at once. */
  pendingCancelId = $state<string | null>(null);

  /** Phase D4 (`STUDIO_PLAN.md`): the real `get_worker_pool_status`
   * (Phase D4a) "Workers: N · Running: R · Queued: Q" snapshot. `null`
   * until the first successful fetch (never fabricated as zeroes). */
  workerPoolStatus = $state<WorkerPoolStatus | null>(null);
  workerPoolStatusError = $state<string | null>(null);

  /** Phase D11 (`STUDIO_PLAN.md`): the persisted `max_concurrent_jobs`
   * worker-pool-size setting, plus the currently-*active* pool size the
   * running app was actually started with — `null` until the first
   * successful fetch (never fabricated). See
   * `src-tauri/src/batch/settings.rs` module doc comment for why this one
   * setting is backend-persisted (unlike every other app-level preference
   * in this codebase, which is `localStorage`-only) and why it's a real,
   * deliberately-chosen restart-to-apply setting rather than a live one:
   * `persisted !== active` is exactly the signal a "restart to apply" UI
   * hint should key off. */
  maxConcurrentJobsSetting = $state<MaxConcurrentJobsSetting | null>(null);
  maxConcurrentJobsError = $state<string | null>(null);
  savingMaxConcurrentJobs = $state(false);

  constructor() {
    // Fire-and-forget, matching `stores/render.svelte.ts`'s
    // `RenderProgressEvent` listener pattern exactly — registered once at
    // module load, well before any batch is started, so no progress event
    // for a job this session knows about can ever be missed.
    void listen<BatchProgressEvent>(BATCH_PROGRESS_EVENT, (event) => {
      const { batch_id, job } = event.payload;
      const previous = this.jobsById[job.id];
      this.jobsById[job.id] = job;
      const order = this.batchJobIds[batch_id];
      if (!order) {
        this.batchJobIds[batch_id] = [job.id];
      } else if (!order.includes(job.id)) {
        this.batchJobIds[batch_id] = [...order, job.id];
      }
      // Refresh the worker pool snapshot the instant a job newly reaches a
      // terminal state — that's exactly when a worker frees up and the
      // real `running`/`queued` numbers change server-side, worth showing
      // right away rather than waiting up to the widget's own 2s poll.
      // Deliberately NOT done on every in-flight progress tick (those fire
      // far more often, e.g. per percent) — that would defeat the "don't
      // waste resources" half of the same requirement.
      if (TERMINAL_JOB_STATUSES.has(job.status) && previous?.status !== job.status) {
        void this.refreshWorkerPoolStatus();
      }
    });
  }

  // -------------------------------------------------------------------
  // Derived
  // -------------------------------------------------------------------

  jobsForSelectedBatch = $derived.by((): BatchJob[] => {
    if (!this.selectedBatchId) return [];
    const ids = this.batchJobIds[this.selectedBatchId] ?? [];
    return ids.map((id) => this.jobsById[id]).filter((j): j is BatchJob => j !== undefined);
  });

  /** True while any job this session knows about hasn't reached a terminal
   * state yet — the real gate the Worker/Slot status widget's own `$effect`
   * uses to start/stop its 2s poll (a plain store class has no lifecycle of
   * its own to hook a `setInterval` into — see `autoZoom.svelte.ts`'s own
   * doc comment for this exact precedent — so the widget component owns the
   * interval, this store only exposes the boolean it polls). */
  anyJobActive = $derived.by((): boolean =>
    Object.values(this.jobsById).some((j) => !TERMINAL_JOB_STATUSES.has(j.status)),
  );

  /** The job with the latest real `started_at` across every job this
   * session knows about — a defined, honest "most likely relevant to what's
   * being worked on right now" rule, not a random pick. Originally
   * `PipelineStepper.svelte`'s own local derivation (its Render step); moved
   * here (Phase D13, `STUDIO_PLAN.md`) so the new `StatusBar.svelte`'s own
   * "Current: <job> – <percent>" line reuses the exact same rule instead of
   * re-deriving it a second time — see `PipelineStepper.svelte`'s own doc
   * comment for the full "why this specific rule" reasoning, unchanged. */
  latestJob = $derived.by((): BatchJob | null => {
    const jobs = Object.values(this.jobsById);
    if (jobs.length === 0) return null;
    return jobs.reduce((latest, job) =>
      Date.parse(job.started_at) > Date.parse(latest.started_at) ? job : latest,
    );
  });

  // -------------------------------------------------------------------
  // Worker/Slot pool status (Phase D4, `get_worker_pool_status` from
  // Phase D4a)
  // -------------------------------------------------------------------

  /** Not a `Result` on the Rust side (nothing fallible before reading the
   * atomics/queue length — see `commands::batch::get_worker_pool_status`),
   * so only a real IPC/transport failure throws here. */
  async refreshWorkerPoolStatus(): Promise<void> {
    try {
      this.workerPoolStatus = await commands.getWorkerPoolStatus();
      this.workerPoolStatusError = null;
    } catch (err) {
      this.workerPoolStatusError = String(err);
    }
  }

  // -------------------------------------------------------------------
  // Max concurrent jobs setting (Phase D11, `STUDIO_PLAN.md`) — persisted,
  // restart-to-apply (see `maxConcurrentJobsSetting`'s own doc comment above
  // for why).
  // -------------------------------------------------------------------

  async refreshMaxConcurrentJobsSetting(): Promise<void> {
    try {
      const result = await commands.getMaxConcurrentJobs();
      if (result.status === "ok") {
        this.maxConcurrentJobsSetting = result.data;
        this.maxConcurrentJobsError = null;
      } else {
        this.maxConcurrentJobsError = result.error.message;
      }
    } catch (err) {
      this.maxConcurrentJobsError = String(err);
    }
  }

  /** Validates + persists `value` (real bounds enforced on the Rust side
   * too, never trusted from the frontend alone). Returns `true` on success.
   * Deliberately does **not** touch `workerPoolStatus`/the running pool —
   * only the next app restart's `spawn_worker_pool` call ever reads this
   * (`batch::settings` module doc comment) — so `maxConcurrentJobsSetting`'s
   * own `active` field is expected to keep showing the old size until then. */
  async setMaxConcurrentJobs(value: number): Promise<boolean> {
    this.savingMaxConcurrentJobs = true;
    this.maxConcurrentJobsError = null;
    try {
      const result = await commands.setMaxConcurrentJobs(value);
      if (result.status === "ok") {
        this.maxConcurrentJobsSetting = result.data;
        return true;
      }
      this.maxConcurrentJobsError = result.error.message;
      return false;
    } catch (err) {
      this.maxConcurrentJobsError = String(err);
      return false;
    } finally {
      this.savingMaxConcurrentJobs = false;
    }
  }

  // -------------------------------------------------------------------
  // Dialog lifecycle
  // -------------------------------------------------------------------

  openJobsDialog(): void {
    this.jobsDialogOpen = true;
  }

  closeJobsDialog(): void {
    this.jobsDialogOpen = false;
  }

  openStartDialog(): void {
    this.startError = null;
    this.startDialogOpen = true;
  }

  closeStartDialog(): void {
    this.startDialogOpen = false;
  }

  // -------------------------------------------------------------------
  // Start a batch (master prompt §42's "100 videos -> pipeline -> Export")
  // -------------------------------------------------------------------

  async startBatch(mediaPaths: string[], config: BatchPipelineConfig): Promise<boolean> {
    if (this.starting || mediaPaths.length === 0) return false;
    this.starting = true;
    this.startError = null;
    try {
      // `start_batch` returns the batch id directly (not a `Result` — Rust
      // side has no fallible step before spawning, see
      // `commands::batch::start_batch`), so only a real IPC failure throws.
      const batchId = await commands.startBatch(mediaPaths, config);
      const listResult = await commands.listBatchJobs(batchId);
      if (listResult.status === "ok") {
        // Merge rather than blindly overwrite: the worker thread starts
        // concurrently with this seed call, so a `batch:progress` event for
        // one of these jobs may already have arrived and must never be
        // clobbered back to a stale `Queued` snapshot.
        for (const job of listResult.data) {
          if (!this.jobsById[job.id]) this.jobsById[job.id] = job;
        }
        this.batchJobIds[batchId] = listResult.data.map((j) => j.id);
      } else {
        this.batchJobIds[batchId] = this.batchJobIds[batchId] ?? [];
      }
      this.batches = [
        { id: batchId, createdAtMs: Date.now(), fileCount: mediaPaths.length },
        ...this.batches,
      ];
      this.selectedBatchId = batchId;
      this.startDialogOpen = false;
      this.jobsDialogOpen = true;
      // Eager refresh (Phase D4): don't make the Worker/Slot widget wait up
      // to its own 2s poll interval to reflect a batch that was JUST
      // started — its jobs already occupy/queue for real worker slots the
      // instant `start_batch` returns.
      void this.refreshWorkerPoolStatus();
      return true;
    } catch (err) {
      this.startError = String(err);
      return false;
    } finally {
      this.starting = false;
    }
  }

  /** Starts a **multi-template** batch (upgrade-plan §11): N `mediaPaths` x
   * M `templateIds` -> N x M jobs, one call. Mirrors `startBatch`'s own
   * seed-then-select-then-open sequence exactly; the one real difference is
   * `commands.startMultiTemplateBatch` returning a `Result` (an unknown
   * `templateIds` entry fails the whole call up front — `commands::batch::
   * start_multi_template_batch`'s own doc comment) rather than a bare batch
   * id, so that error path is surfaced into `startError` exactly like every
   * other failure this store already reports. */
  async startMultiTemplateBatch(
    mediaPaths: string[],
    templateIds: string[],
    config: BatchPipelineConfig,
  ): Promise<boolean> {
    if (this.starting || mediaPaths.length === 0 || templateIds.length === 0) return false;
    this.starting = true;
    this.startError = null;
    try {
      const result = await commands.startMultiTemplateBatch(mediaPaths, templateIds, config);
      if (result.status === "error") {
        this.startError = result.error.message;
        return false;
      }
      const batchId = result.data;
      const listResult = await commands.listBatchJobs(batchId);
      if (listResult.status === "ok") {
        for (const job of listResult.data) {
          if (!this.jobsById[job.id]) this.jobsById[job.id] = job;
        }
        this.batchJobIds[batchId] = listResult.data.map((j) => j.id);
      } else {
        this.batchJobIds[batchId] = this.batchJobIds[batchId] ?? [];
      }
      this.batches = [
        { id: batchId, createdAtMs: Date.now(), fileCount: mediaPaths.length * templateIds.length },
        ...this.batches,
      ];
      this.selectedBatchId = batchId;
      this.startDialogOpen = false;
      this.jobsDialogOpen = true;
      void this.refreshWorkerPoolStatus();
      return true;
    } catch (err) {
      this.startError = String(err);
      return false;
    } finally {
      this.starting = false;
    }
  }

  /** Adopts a batch this store did not itself start (Phase U3 History's
   * "Re-run"/"Re-run with another template" — `stores/history.svelte.ts`
   * calls this after `rerun_from_history`/`rerun_from_history_with_template`
   * return a fresh `RerunResult`) into the same Jobs table/dialog every
   * batch started from `StartBatchDialog` already lands in, rather than
   * building a second "where did my re-run go" UI. Mirrors `startBatch`'s
   * own seed-then-select-then-open sequence exactly, just without also
   * calling `start_batch` itself (the caller already has a real batch id). */
  async adoptExternalBatch(batchId: string, jobIds: string[]): Promise<void> {
    const listResult = await commands.listBatchJobs(batchId);
    if (listResult.status === "ok") {
      for (const job of listResult.data) {
        if (!this.jobsById[job.id]) this.jobsById[job.id] = job;
      }
      this.batchJobIds[batchId] = listResult.data.map((j) => j.id);
    } else {
      this.batchJobIds[batchId] = this.batchJobIds[batchId] ?? jobIds;
    }
    this.batches = [
      { id: batchId, createdAtMs: Date.now(), fileCount: jobIds.length },
      ...this.batches,
    ];
    this.selectedBatchId = batchId;
    this.jobsDialogOpen = true;
    void this.refreshWorkerPoolStatus();
  }

  /** Manual refresh fallback — live updates via `batch:progress` are the
   * primary mechanism (task brief: not polling), but this gives the user a
   * deliberate way to re-sync if the dialog was closed for a long time and
   * they simply want to confirm current state. Overwrites freely since this
   * is an explicit user action, not a background merge. */
  async refreshSelectedBatch(): Promise<void> {
    const batchId = this.selectedBatchId;
    if (!batchId) return;
    const result = await commands.listBatchJobs(batchId);
    if (result.status === "ok") {
      for (const job of result.data) {
        this.jobsById[job.id] = job;
      }
      this.batchJobIds[batchId] = result.data.map((j) => j.id);
    }
  }

  selectBatch(id: string): void {
    this.selectedBatchId = id;
  }

  // -------------------------------------------------------------------
  // Per-row actions (pause/resume/cancel/retry — master prompt §42's
  // "Allow" list)
  // -------------------------------------------------------------------

  private async runAction(jobId: string, action: () => Promise<SimpleResult>): Promise<void> {
    this.actionPendingByJob[jobId] = true;
    this.actionErrorByJob[jobId] = null;
    try {
      const result = await action();
      if (result.status === "error") {
        this.actionErrorByJob[jobId] = result.error.message;
      }
    } finally {
      this.actionPendingByJob[jobId] = false;
    }
  }

  async pause(jobId: string): Promise<void> {
    await this.runAction(jobId, () => commands.pauseBatchJob(jobId));
  }

  async resume(jobId: string): Promise<void> {
    await this.runAction(jobId, () => commands.resumeBatchJob(jobId));
  }

  requestCancel(jobId: string): void {
    this.pendingCancelId = jobId;
  }

  cancelCancelRequest(): void {
    this.pendingCancelId = null;
  }

  async confirmCancel(jobId: string): Promise<void> {
    this.pendingCancelId = null;
    await this.runAction(jobId, () => commands.cancelBatchJob(jobId));
  }

  async retry(jobId: string): Promise<void> {
    await this.runAction(jobId, () => commands.retryBatchJob(jobId));
  }
}

export const batchStore = new BatchStore();
