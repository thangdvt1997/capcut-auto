// Svelte 5 runes-based store for the Phase 8 frontend pass: caption
// generation, the styling panel, and the correction tools (master prompt
// §26/§27/§28). Same "own transient workflow state, composes with
// `stores/timeline.svelte.ts` rather than living inside it" pattern
// `stores/silenceDetector.svelte.ts`/`stores/transcriptEditor.svelte.ts`
// already established — see either file's own doc comment for the
// rationale. `.svelte.ts` (not `.ts`) is required for `$state` outside a
// `.svelte` file, same reasoning as every other store in this directory.
//
// Placement decision (documented per the task brief's "your call, document
// it"): `generate_captions` consumes the *whole project's* `transcript`
// (every media's entries at once, not just one clip's — see that command's
// own doc comment), so it doesn't fit `stores/transcriptEditor.svelte.ts`'s
// "anchored to the selected clip's media" scope without being misleading.
// Generation, styling, and correction all live together in one new
// `CaptionsPanel.svelte` (mounted as `RightPanel.svelte`'s new "Captions"
// tab) instead, since none of the three has a natural single-clip anchor.
//
// Style workflow (mirrors `stores/render.svelte.ts`'s preset-then-override
// pattern): `editStyle(id)` seeds `draft` from any catalog entry (a
// built-in template or an existing project style) as a fully independent
// working copy; every `CaptionStyle` field is then freely overridable.
// `draftDirty` is true whenever `draft` no longer matches the catalog entry
// it was seeded from — `applyStyleToSelected` refuses to run while dirty,
// so a caption can never be silently stamped with a style id whose catalog
// entry doesn't actually match what the panel is showing. `Save` persists
// `draft` into the project's own `caption_styles` catalog (updating in
// place if it started from an existing *custom* style, otherwise forking a
// new one — built-in templates are never mutated in place), after which the
// draft is clean again and applying becomes possible.

import { commands } from "../types/bindings";
import type {
  Caption,
  CaptionGenerationSettings,
  CaptionGroupingMode,
  CaptionSplitPoint,
  CaptionStyle,
  FindReplaceOptions,
} from "../types/bindings";
import { findActiveCaption, findActiveWordIndex, hiddenTrackIdSet } from "../captions/karaoke";
import { buildStyleCatalog, cloneStyle, FALLBACK_CAPTION_STYLE, resolveCaptionStyle, stylesEqual } from "../captions/styleCatalog";
import { timeline } from "./timeline.svelte";

function isWordChar(ch: string | undefined): boolean {
  return !!ch && /[\p{L}\p{N}_]/u.test(ch);
}

function isWholeWordMatch(haystack: string, idx: number, len: number): boolean {
  const before = haystack[idx - 1];
  const after = haystack[idx + len];
  return !isWordChar(before) && !isWordChar(after);
}

/** Client-side mirror of `timeline::captions::replace_in_text`'s matching
 * rules (case-sensitivity, whole-word) — used only to show a live match
 * count in the Find & Replace UI before committing, never to perform the
 * actual replacement (that's always the real backend command, so undo/redo
 * and the word-count/timestamp-preservation policy stay authoritative on
 * the Rust side). */
function countMatches(text: string, find: string, caseSensitive: boolean, wholeWord: boolean): number {
  if (find === "") return 0;
  const haystack = caseSensitive ? text : text.toLowerCase();
  const needle = caseSensitive ? find : find.toLowerCase();
  let count = 0;
  let from = 0;
  for (;;) {
    const idx = haystack.indexOf(needle, from);
    if (idx === -1) break;
    if (!wholeWord || isWholeWordMatch(haystack, idx, needle.length)) count++;
    from = idx + Math.max(1, needle.length);
  }
  return count;
}

class CaptionsStore {
  // -------------------------------------------------------------------
  // Style catalog (master prompt §26)
  // -------------------------------------------------------------------

  templates = $state<CaptionStyle[]>([]);
  templatesLoading = $state(false);
  templatesError = $state<string | null>(null);

  projectStyles = $derived<CaptionStyle[]>(timeline.project?.caption_styles ?? []);
  catalog = $derived<CaptionStyle[]>(buildStyleCatalog(this.templates, this.projectStyles));

  /** The style currently being edited in the Style section — always a full,
   * independent `CaptionStyle` so every override field always has a live
   * value to bind to. Seeded from a catalog entry via `editStyle()`. */
  draft = $state<CaptionStyle>(cloneStyle(FALLBACK_CAPTION_STYLE));
  draftSourceId = $state<string | null>(null);

  draftCatalogEntry = $derived<CaptionStyle | null>(
    this.draftSourceId ? (this.catalog.find((s) => s.id === this.draftSourceId) ?? null) : null,
  );
  /** See class doc comment's "Style workflow" section. */
  draftDirty = $derived(!this.draftCatalogEntry || !stylesEqual(this.draft, this.draftCatalogEntry));
  draftIsCustom = $derived(this.projectStyles.some((s) => s.id === this.draftSourceId));

  savingStyle = $state(false);
  styleError = $state<string | null>(null);

  // -------------------------------------------------------------------
  // Caption selection (multi-select — merge / bulk style)
  // -------------------------------------------------------------------

  selectedCaptionIds = $state<Set<string>>(new Set());

  // -------------------------------------------------------------------
  // Generation (master prompt §26)
  // -------------------------------------------------------------------

  genTrackId = $state<string | null>(null);
  genGrouping = $state<CaptionGroupingMode>("sentence");
  genMaxWordsPerLine = $state(6);
  genMaxCharsPerLine = $state(30);
  generating = $state(false);
  generateError = $state<string | null>(null);

  // -------------------------------------------------------------------
  // Find & replace (master prompt §28)
  // -------------------------------------------------------------------

  findText = $state("");
  replaceText = $state("");
  caseSensitive = $state(false);
  wholeWord = $state(false);
  findReplaceBusy = $state(false);
  findReplaceError = $state<string | null>(null);

  // -------------------------------------------------------------------
  // Correction (split / merge / retime) transient state
  // -------------------------------------------------------------------

  correctionError = $state<string | null>(null);
  busyCaptionId = $state<string | null>(null);

  constructor() {
    void this.ensureTemplatesLoaded();
  }

  // -------------------------------------------------------------------
  // Derived over the shared project
  // -------------------------------------------------------------------

  captions = $derived<Caption[]>(timeline.project?.captions ?? []);
  captionTracks = $derived(timeline.tracks.filter((t) => t.kind === "caption"));
  effectiveGenTrackId = $derived(this.genTrackId ?? this.captionTracks[0]?.id ?? null);

  hasTranscript = $derived((timeline.project?.transcript.length ?? 0) > 0);

  selectedCaptions = $derived(this.captions.filter((c) => this.selectedCaptionIds.has(c.id)));

  // ---- Karaoke / active-word rendering model (master prompt §27) --------
  // Two *primitive* derived values (id + index), not one object, so a
  // consuming `$derived`/component only re-renders when the caption
  // identity or the active word index actually changes — Svelte 5's
  // `$derived` already skips downstream work when a primitive result is
  // unchanged (`Object.is`), which is exactly the "don't recompute/re-render
  // every playhead tick if the answer hasn't changed" requirement §27 asks
  // for, with no extra memoization machinery needed. The lookups themselves
  // (`findActiveCaption`/`findActiveWordIndex`) are documented in
  // `captions/karaoke.ts`.
  hiddenTrackIds = $derived(hiddenTrackIdSet(timeline.tracks));
  activeCaption = $derived<Caption | null>(findActiveCaption(this.captions, this.hiddenTrackIds, timeline.playheadUs));
  activeCaptionId = $derived(this.activeCaption?.id ?? null);
  activeWordIndex = $derived(this.activeCaption ? findActiveWordIndex(this.activeCaption.words, timeline.playheadUs) : -1);
  activeCaptionStyle = $derived(resolveCaptionStyle(this.catalog, this.activeCaption?.style_id ?? null));

  matchCount = $derived.by((): number => {
    const find = this.findText;
    if (find === "") return 0;
    let count = 0;
    for (const caption of this.captions) {
      count += countMatches(caption.text, find, this.caseSensitive, this.wholeWord);
    }
    return count;
  });

  // -------------------------------------------------------------------
  // Style catalog
  // -------------------------------------------------------------------

  async ensureTemplatesLoaded(): Promise<void> {
    if (this.templates.length > 0 || this.templatesLoading) return;
    this.templatesLoading = true;
    this.templatesError = null;
    try {
      this.templates = await commands.listCaptionTemplates();
      const first = this.templates[0];
      if (!this.draftSourceId && first) this.editStyle(first.id);
    } catch (err) {
      this.templatesError = String(err);
    } finally {
      this.templatesLoading = false;
    }
  }

  /** Seeds `draft` from any catalog entry — analogous to
   * `stores/render.svelte.ts`'s `selectPreset`, except the "preset" here is
   * also the exact thing later applied to captions (`applyStyleToSelected`),
   * not just a one-time form seed. */
  editStyle(styleId: string): void {
    const entry = this.catalog.find((s) => s.id === styleId);
    if (!entry) return;
    this.draft = cloneStyle(entry);
    this.draftSourceId = styleId;
    this.styleError = null;
  }

  // `background`/`outline`/`shadow` are `Option<T>` on the wire — toggling
  // them needs a real object literal (with sensible defaults), not just a
  // checkbox bound straight to a nullable field.
  setBackgroundEnabled(enabled: boolean): void {
    this.draft.background = enabled ? (this.draft.background ?? { color: { r: 0, g: 0, b: 0 }, opacity: 0.6 }) : null;
  }
  setOutlineEnabled(enabled: boolean): void {
    this.draft.outline = enabled ? (this.draft.outline ?? { color: { r: 0, g: 0, b: 0 }, width: 0.08 }) : null;
  }
  setShadowEnabled(enabled: boolean): void {
    this.draft.shadow = enabled
      ? (this.draft.shadow ?? { color: { r: 0, g: 0, b: 0 }, opacity: 0.6, offset_x: 0.01, offset_y: 0.01, blur: 15 })
      : null;
  }

  /** Persists `draft` into the project's `caption_styles` catalog via
   * `set_caption_styles` (a settings-catalog replace, NOT an undo-able
   * timeline command — see that command's own doc comment). Updates the
   * matching entry in place when `draft` was seeded from an existing custom
   * style (`draftIsCustom`); otherwise forks a brand-new entry with a fresh
   * id, since built-in templates (`list_caption_templates`) are a read-only
   * catalog this store never mutates directly. */
  async saveDraftAsProjectStyle(name: string): Promise<void> {
    if (this.savingStyle) return;
    this.savingStyle = true;
    this.styleError = null;
    try {
      const sourceId = this.draftSourceId;
      const id = this.draftIsCustom && sourceId ? sourceId : `style_${crypto.randomUUID()}`;
      const saved: CaptionStyle = { ...cloneStyle(this.draft), id, name };
      const next = this.draftIsCustom
        ? this.projectStyles.map((s) => (s.id === id ? saved : s))
        : [...this.projectStyles, saved];
      const outcome = await timeline.applyExternalProjectResult(commands.setCaptionStyles(next));
      if (outcome.ok) {
        this.draft = cloneStyle(saved);
        this.draftSourceId = id;
      } else {
        this.styleError = outcome.error;
      }
    } finally {
      this.savingStyle = false;
    }
  }

  /** Applies the *saved* catalog entry `draftSourceId` resolves to (never
   * the possibly-unsaved `draft` object itself) to every caption in `ids`.
   * Callers should gate this on `!draftDirty` in the UI (disable the
   * button) — enforced here too as a defensive no-op, not just a UI
   * affordance. */
  async applyStyleToSelected(ids: string[]): Promise<void> {
    const sourceId = this.draftSourceId;
    if (ids.length === 0 || !sourceId || this.draftDirty) return;
    const outcome = await timeline.applyExternalProjectResult(commands.bulkSetCaptionStyle(ids, sourceId));
    if (!outcome.ok) this.styleError = outcome.error;
  }

  /** Per-row quick-apply (any catalog style id, or `null` to clear back to
   * the project default) — independent of the Style section's draft
   * workflow above, for the common "just pick a template for this one
   * caption" case. */
  async setCaptionStyle(captionId: string, styleId: string | null): Promise<void> {
    const outcome = await timeline.applyExternalProjectResult(commands.bulkSetCaptionStyle([captionId], styleId));
    if (!outcome.ok) this.styleError = outcome.error;
  }

  // -------------------------------------------------------------------
  // Selection
  // -------------------------------------------------------------------

  toggleCaptionSelected(id: string, additive: boolean): void {
    const next = new Set(additive ? this.selectedCaptionIds : []);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.selectedCaptionIds = next;
  }

  clearCaptionSelection(): void {
    this.selectedCaptionIds = new Set();
  }

  // -------------------------------------------------------------------
  // Generation (master prompt §26)
  // -------------------------------------------------------------------

  buildGenerationSettings(): CaptionGenerationSettings {
    return {
      max_words_per_line: Math.max(1, Math.round(this.genMaxWordsPerLine)),
      max_chars_per_line: Math.max(1, Math.round(this.genMaxCharsPerLine)),
      grouping: this.genGrouping,
    };
  }

  async generate(): Promise<void> {
    const trackId = this.effectiveGenTrackId;
    if (!trackId || this.generating || !this.hasTranscript) return;
    this.generating = true;
    this.generateError = null;
    try {
      const outcome = await timeline.applyExternalProjectResult(
        commands.generateCaptions(trackId, this.buildGenerationSettings()),
      );
      if (!outcome.ok) this.generateError = outcome.error;
    } finally {
      this.generating = false;
    }
  }

  // -------------------------------------------------------------------
  // Correction: split / merge / retime (master prompt §28)
  // -------------------------------------------------------------------

  /** Splits `caption` at the current playhead — `CaptionRow.svelte` only
   * enables this action while the playhead actually falls inside the
   * caption's span, so a `TimeUs` split point is always meaningful here
   * (see `timeline::captions::split_caption`'s own validation of that). */
  async splitAtPlayhead(caption: Caption): Promise<void> {
    if (this.busyCaptionId) return;
    this.busyCaptionId = caption.id;
    this.correctionError = null;
    try {
      const point: CaptionSplitPoint = { time_us: timeline.playheadUs };
      const outcome = await timeline.applyExternalProjectResult(commands.splitCaption(caption.id, point));
      if (!outcome.ok) this.correctionError = outcome.error;
    } finally {
      this.busyCaptionId = null;
    }
  }

  async mergeSelected(): Promise<void> {
    const ids = Array.from(this.selectedCaptionIds);
    if (ids.length < 2) return;
    this.correctionError = null;
    const outcome = await timeline.applyExternalProjectResult(commands.mergeCaptions(ids));
    if (outcome.ok) this.selectedCaptionIds = new Set();
    else this.correctionError = outcome.error;
  }

  /** Also the backend for a drag-boundary gesture (same primitive, a
   * different UI trigger — see `retime_caption`'s own doc comment). This
   * pass exposes it as explicit numeric start/end inputs plus a
   * "scale words with retime" checkbox (`CaptionRow.svelte`) rather than a
   * timeline drag handle — see `IMPLEMENTATION_PLAN.md` Phase 8 notes for
   * the placement rationale (captions are shown in a dedicated panel here,
   * not as timeline blocks). */
  async retime(caption: Caption, newStartUs: number, newEndUs: number, scaleWords: boolean): Promise<void> {
    if (this.busyCaptionId) return;
    this.busyCaptionId = caption.id;
    this.correctionError = null;
    try {
      const outcome = await timeline.applyExternalProjectResult(
        commands.retimeCaption(caption.id, Math.round(newStartUs), Math.round(newEndUs), scaleWords),
      );
      if (!outcome.ok) this.correctionError = outcome.error;
    } finally {
      this.busyCaptionId = null;
    }
  }

  // -------------------------------------------------------------------
  // Find & replace (master prompt §28)
  // -------------------------------------------------------------------

  buildFindReplaceOptions(): FindReplaceOptions {
    return { case_sensitive: this.caseSensitive, whole_word: this.wholeWord };
  }

  async replaceAll(): Promise<void> {
    if (this.findText === "" || this.findReplaceBusy) return;
    this.findReplaceBusy = true;
    this.findReplaceError = null;
    try {
      const outcome = await timeline.applyExternalProjectResult(
        commands.findReplaceCaptions(this.findText, this.replaceText, this.buildFindReplaceOptions()),
      );
      if (!outcome.ok) this.findReplaceError = outcome.error;
    } finally {
      this.findReplaceBusy = false;
    }
  }
}

export const captionsStore = new CaptionsStore();
