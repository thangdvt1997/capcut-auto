// Svelte 5 runes-based store for the Phase 12 First-Run Wizard (master
// prompt §58): Welcome -> System Check -> FFmpeg -> GPU Detection -> CapCut
// Detection -> AI Provider (optional) -> Transcription Model (optional) ->
// Project Folder -> Ready.
//
// This store owns only step navigation + the "has this ever been completed"
// flag + the one genuinely new data fetch (`get_system_information`, for the
// System Check/FFmpeg steps). Every other step's real data/actions are
// deliberately NOT duplicated here — the component reuses the already-real
// stores directly (`stores/capcut.svelte.ts` for CapCut Detection,
// `stores/render.svelte.ts`'s `hardware`/`ensureHardwareDetected` for GPU
// Detection, `stores/aiSettings.svelte.ts` for AI Provider,
// `stores/modelManager.svelte.ts` for Transcription Model,
// `stores/projectFolder.svelte.ts` for Project Folder), per this task's own
// "reuse the real store, don't duplicate its logic in the wizard" brief.
//
// ## Gating: shown automatically on first launch, reachable manually later
//
// `ave:wizard:completed` (`localStorage`, matching every other app-level
// persisted setting in this codebase) gates whether the wizard auto-opens.
// `openManually()` (wired to a "Setup Wizard…" button in `TopBar.svelte`) can
// always reopen it regardless of that flag, resetting to step 0.
//
// Closing via the header's "×" (`close()`) does NOT set the completed flag —
// the wizard simply wasn't finished, so it auto-opens again next launch,
// same as never having dismissed it. Only reaching the last ("Ready") step's
// "Finish" button, or the Welcome step's explicit "Skip Setup" link, marks it
// completed (`finish()`) — the latter exists for a user who genuinely never
// wants to see this wizard again, without forcing them through all 9 steps
// first.

import { commands } from "../types/bindings";
import type { SystemInformation } from "../types/bindings";

const COMPLETED_STORAGE_KEY = "ave:wizard:completed";

function loadCompleted(): boolean {
  try {
    return localStorage.getItem(COMPLETED_STORAGE_KEY) === "1";
  } catch {
    // localStorage may be unavailable (private browsing, disabled storage) —
    // treat as "not completed yet" so the wizard still gets a chance to show
    // rather than silently never appearing.
    return false;
  }
}

function saveCompleted(): void {
  try {
    localStorage.setItem(COMPLETED_STORAGE_KEY, "1");
  } catch {
    /* storage may be disabled — the flag simply won't survive a restart,
       so the wizard would show again next launch; harmless. */
  }
}

export const WIZARD_STEPS = [
  "welcome",
  "systemCheck",
  "ffmpeg",
  "gpu",
  "capcut",
  "aiProvider",
  "transcriptionModel",
  "projectFolder",
  "ready",
] as const;

export type WizardStep = (typeof WIZARD_STEPS)[number];

class FirstRunWizardStore {
  /** Whether the wizard has ever been completed/skipped before — read once
   * at module load; only ever flips `false -> true` via `finish()`. */
  private completedBefore = loadCompleted();

  open = $state(this.completedBefore ? false : true);
  stepIndex = $state(0);

  // System Check / FFmpeg steps' shared data source — the one genuinely new
  // fetch this store owns (every other step reuses an existing store).
  systemInfo = $state<SystemInformation | null>(null);
  systemInfoLoading = $state(false);
  systemInfoError = $state<string | null>(null);

  currentStep = $derived<WizardStep>(WIZARD_STEPS[this.stepIndex] ?? "welcome");
  isFirstStep = $derived(this.stepIndex === 0);
  isLastStep = $derived(this.stepIndex === WIZARD_STEPS.length - 1);
  stepNumber = $derived(this.stepIndex + 1);
  totalSteps = WIZARD_STEPS.length;

  constructor() {
    if (this.open) {
      void this.loadSystemInfo();
    }
  }

  /** Reachable from `TopBar.svelte`'s "Setup Wizard…" button — reopens
   * regardless of `completedBefore`, always starting fresh at step 0. */
  openManually(): void {
    this.stepIndex = 0;
    this.open = true;
    void this.loadSystemInfo();
  }

  /** Closes without marking completed — see class doc comment. */
  close(): void {
    this.open = false;
  }

  next(): void {
    if (this.stepIndex < WIZARD_STEPS.length - 1) {
      this.stepIndex += 1;
      if (this.currentStep === "systemCheck" || this.currentStep === "ffmpeg") {
        void this.loadSystemInfo();
      }
    }
  }

  back(): void {
    if (this.stepIndex > 0) {
      this.stepIndex -= 1;
    }
  }

  /** Marks the wizard completed and closes it — called by the final "Ready"
   * step's Finish button, and by the Welcome step's "Skip Setup" link. */
  finish(): void {
    this.completedBefore = true;
    saveCompleted();
    this.open = false;
  }

  async loadSystemInfo(): Promise<void> {
    if (this.systemInfoLoading) return;
    this.systemInfoLoading = true;
    this.systemInfoError = null;
    try {
      const result = await commands.getSystemInformation();
      if (result.status === "ok") {
        this.systemInfo = result.data;
      } else {
        this.systemInfoError = result.error.message;
      }
    } catch (err) {
      this.systemInfoError = String(err);
    } finally {
      this.systemInfoLoading = false;
    }
  }
}

export const firstRunWizardStore = new FirstRunWizardStore();
