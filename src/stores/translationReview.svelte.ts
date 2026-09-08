// Svelte 5 runes-based store for the Translation Review dialog
// (`STUDIO_PLAN.md` Phase D12): the real frontend "Accept and Apply" half of
// `ai::translate_captions` (Phase S6, backend-only until this pass — see
// that command's own doc comment: "This is a *proposal* the frontend shows
// the user for review... This command never mutates `project.captions`
// itself"). Same overall shape as `stores/smartEdit.svelte.ts` (Analyze ->
// per-row review/override -> Apply), adapted for translation's own
// "N independent full-coverage proposals, accept/reject each one" contract
// instead of Smart Edit's "per-recommendation suggested action".
//
// ## Accept/reject model
//
// `translate()` populates `proposals` (`caption_id` -> `translated_text`,
// one entry per real caption — `ai::translate::parse_and_validate`'s own
// full-coverage guarantee). Every proposal starts *accepted* by default —
// same "AI's own suggestion is the starting point, the user downgrades/
// rejects the ones they don't want" precedent `smartEdit.actionOverrides`
// already established (there, the default is the AI's `suggested_action`;
// here, the default is "yes, apply this translation" — rejecting one is an
// explicit uncheck, not an explicit opt-in). `rejected` tracks only the
// caption ids a user has explicitly excluded; `apply()` only ever sends the
// accepted subset to the real `apply_caption_translations` command (Phase
// D12's own new backend command — the one and only place a translation
// proposal becomes a real project mutation, matching `ai::translate`'s
// module doc comment precisely).
//
// ## Why proposals for already-applied captions are dropped after `apply()`
//
// Once a caption's translated text is written into `project.captions.text`
// for real, showing it again as a "proposal" would be misleading (the
// Original column now already shows exactly that text — see
// `ScriptEditor.svelte`). `apply()` therefore removes every *accepted* id
// from `proposals` on success, keeping only the still-rejected ones (a user
// can still revisit and accept those later, or re-run translation to get a
// fresh proposal for them).

import { commands } from "../types/bindings";
import type {
  ProfanityHandling,
  TranslatedCaption,
  TranslationGenre,
  TranslationSettings,
} from "../types/bindings";
import { captionsStore } from "./captions.svelte";
import { timeline } from "./timeline.svelte";
import { aiSettingsStore, keyRequirementFor } from "./aiSettings.svelte";

export const TRANSLATION_GENRES: readonly TranslationGenre[] = [
  "drama_romance",
  "fantasy_cultivation",
  "crime_detective",
  "police_bodycam",
  "prison_crime",
  "survival",
  "documentary",
  "custom",
];

export const PROFANITY_HANDLINGS: readonly ProfanityHandling[] = ["preserve", "soften", "remove"];

class TranslationReviewStore {
  open = $state(false);

  // ---- Translation settings (`ai::translate::TranslationSettings`, `promt.md` §8) ----
  sourceLanguage = $state(""); // blank -> `None` (auto-detect), matching `translate_captions`'s own convention
  targetLanguage = $state("vi");
  genre = $state<TranslationGenre | "">("");
  translationStyle = $state("");
  characterContext = $state("");
  preserveNames = $state(false);
  preserveTerminology = $state(false);
  profanityHandling = $state<ProfanityHandling | "">("");
  sentenceLengthOptimization = $state(false);
  voiceFriendlyRewrite = $state(false);

  // ---- Propose (translate) ----
  translating = $state(false);
  translateError = $state<string | null>(null);
  /** `caption_id` -> proposed translated text. One entry per real caption
   * covered by the last successful `translate()` call (full-coverage). */
  proposals = $state<Record<string, string>>({});
  /** Caption ids explicitly excluded from the next `apply()` — absence from
   * this set means "accepted" (see class doc comment). */
  rejected = $state<Set<string>>(new Set());

  // ---- Apply (accept -> real project mutation) ----
  applying = $state(false);
  applyError = $state<string | null>(null);
  appliedThisSession = $state(false);
  lastAppliedCount = $state(0);

  // -------------------------------------------------------------------
  // Derived
  // -------------------------------------------------------------------

  hasProposals = $derived(Object.keys(this.proposals).length > 0);
  proposedIds = $derived(Object.keys(this.proposals));
  acceptedIds = $derived(this.proposedIds.filter((id) => !this.rejected.has(id)));
  acceptedCount = $derived(this.acceptedIds.length);

  /** Same `model` + (only when the configured provider actually requires a
   * key) `hasKeyConfigured` check `stores/smartEdit.svelte.ts`'s own
   * `aiConfigured` already establishes — not duplicated logic, just the
   * exact same real check against the shared AI Settings store. */
  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );

  canTranslate = $derived(
    captionsStore.captions.length > 0 &&
      this.targetLanguage.trim() !== "" &&
      this.aiConfigured &&
      !this.translating,
  );
  canApply = $derived(this.acceptedCount > 0 && !this.applying);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    this.translateError = null;
    this.applyError = null;
  }

  close(): void {
    this.open = false;
  }

  private buildSettings(): TranslationSettings {
    return {
      genre: this.genre || null,
      translation_style: this.translationStyle.trim() || null,
      character_context: this.characterContext.trim() || null,
      preserve_names: this.preserveNames,
      preserve_terminology: this.preserveTerminology,
      profanity_handling: this.profanityHandling || null,
      sentence_length_optimization: this.sentenceLengthOptimization,
      voice_friendly_rewrite: this.voiceFriendlyRewrite,
      system_prompt_prefix: null,
    };
  }

  // -------------------------------------------------------------------
  // Propose: real `translate_captions` call (Phase S6)
  // -------------------------------------------------------------------

  async translate(): Promise<void> {
    if (!this.canTranslate) return;
    this.translating = true;
    this.translateError = null;
    this.applyError = null;
    try {
      const result = await commands.translateCaptions(
        captionsStore.captions,
        this.sourceLanguage.trim() || null,
        this.targetLanguage.trim(),
        this.buildSettings(),
        aiSettingsStore.settingsSnapshot(),
      );
      if (result.status === "ok") {
        const map: Record<string, string> = {};
        for (const tc of result.data) map[tc.caption_id] = tc.translated_text;
        this.proposals = map;
        this.rejected = new Set();
      } else {
        this.translateError = result.error.message;
      }
    } catch (err) {
      this.translateError = String(err);
    } finally {
      this.translating = false;
    }
  }

  // -------------------------------------------------------------------
  // Per-row accept/reject
  // -------------------------------------------------------------------

  isAccepted(captionId: string): boolean {
    return captionId in this.proposals && !this.rejected.has(captionId);
  }

  setAccepted(captionId: string, accepted: boolean): void {
    const next = new Set(this.rejected);
    if (accepted) next.delete(captionId);
    else next.add(captionId);
    this.rejected = next;
  }

  acceptAll(): void {
    this.rejected = new Set();
  }

  rejectAll(): void {
    this.rejected = new Set(this.proposedIds);
  }

  /** Discards every unapplied proposal (does not touch anything already
   * written to the real project — that's `timeline.undo()`'s job, same as
   * every other AI feature's own "Reset" here). */
  discard(): void {
    this.proposals = {};
    this.rejected = new Set();
    this.translateError = null;
  }

  // -------------------------------------------------------------------
  // Apply: the real "Accept and Apply" step (Phase D12's new backend command)
  // -------------------------------------------------------------------

  async apply(): Promise<void> {
    if (!this.canApply) return;
    this.applying = true;
    this.applyError = null;
    try {
      // `acceptedIds` is always derived from `Object.keys(this.proposals)`
      // (see `acceptedIds`/`proposedIds` above), so every id here is
      // guaranteed to have a proposal — the non-null assertion reflects
      // that real invariant, not an unchecked guess.
      const toApply: TranslatedCaption[] = this.acceptedIds.map((captionId) => ({
        caption_id: captionId,
        translated_text: this.proposals[captionId]!,
      }));
      const outcome = await timeline.applyExternalProjectResult(commands.applyCaptionTranslations(toApply));
      if (outcome.ok) {
        this.lastAppliedCount = toApply.length;
        this.appliedThisSession = true;
        // Applied ids are now real project text — drop them from the
        // proposal list (class doc comment); anything still rejected stays
        // available for a later look.
        const remaining: Record<string, string> = {};
        for (const [captionId, text] of Object.entries(this.proposals)) {
          if (this.rejected.has(captionId)) remaining[captionId] = text;
        }
        this.proposals = remaining;
        this.rejected = new Set();
      } else {
        this.applyError = outcome.error;
      }
    } finally {
      this.applying = false;
    }
  }
}

export const translationReviewStore = new TranslationReviewStore();
