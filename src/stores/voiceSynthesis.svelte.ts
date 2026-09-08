// Svelte 5 runes-based store for real voice synthesis (`promt.md` §9,
// STUDIO_PLAN.md Phase D16) — the frontend half of
// `commands::voice::synthesize_speech`. Deliberately its own file, not
// folded into `stores/voiceSettings.svelte.ts`: that store owns
// provider/connection/mapping *configuration*, this one owns the *job
// lifecycle* of an actual synthesis call, the same "settings store vs. job
// store" split `stores/render.svelte.ts` (settings + job state together,
// since that dialog has only ever had one job type) and
// `stores/transcriptEditor.svelte.ts` (transcription job state, separate
// from whatever owns the model catalog) already establish between them.
//
// ## Scope — read before extending this file
//
// This is a standalone "generate one voice line from one mapping" trigger,
// matching Phase D16's own brief: a real end-to-end verification that a
// `VoiceRoleMapping` actually produces playable audio, NOT the full
// per-caption dubbing pipeline `promt.md` §3's 10-step vision describes (no
// per-row script table exists yet to drive that from — a separate, larger,
// not-yet-built gap). One job at a time, keyed by `job_id` the same way
// `stores/render.svelte.ts`'s `progressByJob` is (not assumed singular, even
// though the current UI only ever starts one at a time).
//
// ## Honest progress/cancellation model
//
// `commands::voice::synthesize_speech`'s own module doc comment covers this
// in full: a single HTTP call has no real mid-flight percentage to report,
// so the *only* event this store ever receives for a job is its terminal
// outcome (`done: true` — success, failure, or a pre-start cancellation).
// Between calling `synthesizeSpeech` and that terminal event, this store's
// own `isRunning` derived state is what "running" means — not a fabricated
// backend-reported percentage. Cancellation is real but narrow: it only
// takes effect if the backend hasn't already started the HTTP request (see
// that same doc comment) — `cancel()` here does not claim otherwise.

import { listen } from "@tauri-apps/api/event";
import { commands } from "../types/bindings";
import type { VoiceProviderSettings, VoiceSynthesisSettings } from "../types/bindings";

/**
 * Payload of the `voice:progress` Tauri event
 * (`src-tauri/src/commands/voice.rs::VoiceProgressEvent`). Hand-written
 * rather than specta-generated, matching `stores/render.svelte.ts`'s own
 * `RenderProgressEvent` precedent — this `tauri-specta` `Builder` only
 * registers *commands*, not typed events (see that file's doc comment for
 * the full rationale). Keep in sync with the Rust struct by hand.
 */
export interface VoiceProgressEvent {
  job_id: string;
  done: boolean;
  cancelled: boolean;
  output_path: string | null;
  duration_us: number | null;
  error: string | null;
}

const VOICE_PROGRESS_EVENT = "voice:progress";

class VoiceSynthesisStore {
  /** Keyed by `job_id`, not just "the current job" — see module doc
   * comment's `stores/render.svelte.ts` precedent. */
  progressByJob = $state<Record<string, VoiceProgressEvent>>({});

  jobId = $state<string | null>(null);
  starting = $state(false);
  startError = $state<string | null>(null);
  cancelling = $state(false);

  constructor() {
    // Fire-and-forget, matching `stores/render.svelte.ts`'s
    // `RenderProgressEvent` listener pattern exactly.
    void listen<VoiceProgressEvent>(VOICE_PROGRESS_EVENT, (event) => {
      this.progressByJob[event.payload.job_id] = event.payload;
    });
  }

  progress = $derived(this.jobId ? (this.progressByJob[this.jobId] ?? null) : null);
  /** `job_id` known, no terminal event received yet (module doc comment —
   * this is the only "running" signal this store has, and it is real: the
   * backend genuinely hasn't reported an outcome yet). */
  isRunning = $derived(this.jobId !== null && !(this.progress?.done ?? false));

  async generate(
    text: string,
    voiceId: string,
    settings: VoiceProviderSettings,
    synthesisSettings: VoiceSynthesisSettings,
  ): Promise<void> {
    if (this.starting || this.isRunning) return;
    this.starting = true;
    this.startError = null;
    try {
      const result = await commands.synthesizeSpeech(text, voiceId, settings, synthesisSettings);
      if (result.status === "ok") {
        this.jobId = result.data;
      } else {
        this.startError = result.error.message;
      }
    } catch (err) {
      this.startError = String(err);
    } finally {
      this.starting = false;
    }
  }

  async cancel(): Promise<void> {
    if (!this.jobId || this.cancelling) return;
    this.cancelling = true;
    try {
      const result = await commands.cancelVoiceJob(this.jobId);
      if (result.status === "error") {
        this.startError = result.error.message;
      }
    } finally {
      this.cancelling = false;
    }
  }

  /** Clears the finished job's result so a new generation can be started
   * without it looking like a stale result from a previous attempt. */
  reset(): void {
    this.jobId = null;
    this.startError = null;
  }
}

export const voiceSynthesisStore = new VoiceSynthesisStore();
