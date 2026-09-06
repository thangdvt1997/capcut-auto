// Svelte 5 runes-based store for the AI Auto Template dialog (upgrade spec
// §7, `UPGRADE_PLAN.md` Phase U2, `src-tauri/src/commands/auto_template.rs`'s
// `suggest_template_for_media`). Same source track/clip picker shape as
// `stores/highlightDetection.svelte.ts`/`stores/smartEdit.svelte.ts` (the
// dialog always analyzes "the currently selected timeline clip's underlying
// media"), reusing the exact same `timeline.project?.transcript` scoping
// convention for the transcript passed to the backend — no second transcript
// source.
//
// Unlike `highlightDetection`'s own AI signal (optional — that feature works
// fine on local signals alone), `suggest_template_for_media` genuinely
// requires `AiProviderSettings` (its own Rust signature takes
// `ai_settings: AiProviderSettings`, not `Option<...>` — there is no
// non-AI fallback for "recommend a template", unlike "score these
// highlights"). So this store never gates the Suggest button on
// `aiConfigured` (mirroring `stores/aiNlCommand.svelte.ts`'s own posture:
// just call the backend and surface whatever clear error it returns if no
// key is configured), but still shows the same "AI not configured" hint
// `highlightDetection`/`smartEdit` already show, so the user isn't surprised
// by the failure.
//
// ## Accept / Change Template / Customize / Run (upgrade spec §7's own UI)
//
// This module deliberately does not build a second "apply a template"
// mechanism. Every one of the four actions below routes through mechanisms
// this codebase already has real, working implementations of
// (`stores/templates.svelte.ts`/`stores/render.svelte.ts`):
//
//   - **Accept**: resolves the recommended `template_id` against
//     `templatesStore.allTemplates` (the same real catalog
//     `TemplatesPanel.svelte` renders) and calls
//     `templatesStore.applyToProject` — the *exact* apply mechanism that
//     panel's own "Apply to Project" button uses, never a second one.
//   - **Change Template**: rather than inventing a tab-switch/deep-link into
//     `LeftPanel.svelte`'s own local (non-exported) tab state, this dialog
//     exposes the real, full catalog (`templatesStore.allTemplates`) inline
//     via `browsingCatalog`, so "pick a different one" applies through that
//     same `templatesStore.applyToProject` call without ever leaving this
//     dialog or needing a second picker UI.
//   - **Customize**: once a template has been applied this session (Accept,
//     or a pick from the Change Template list), `openCustomize()` opens the
//     existing `templatesStore.openSaveForm()` — the same session-local
//     zoom/silence/transition/export-preset controls `applyToProject` itself
//     already pre-filled (`stores/templates.svelte.ts`'s own class doc
//     comment) become editable there, and "Save Template" persists a
//     tweaked variant if the user wants one. No second settings form.
//   - **Run**: once a template has been applied this session, `openRun()`
//     opens the real Export/Render dialog (`renderStore.openDialog()`) —
//     the same "Export…" action `Timeline.svelte`'s own toolbar/`TopBar`'s
//     File menu already expose, pre-loaded with whatever export preset was
//     just pre-filled.
//
// "A way back out without applying anything" (task brief) is simply
// `close()` — nothing is applied unless Accept/a Change-Template pick was
// explicitly clicked.

import { commands } from "../types/bindings";
import type { AiTemplateRecommendation, Clip, MediaItem, Template, Track, TranscriptEntry } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { templatesStore } from "./templates.svelte";
import { renderStore } from "./render.svelte";
import { aiSettingsStore, currentAiProviderSettings, keyRequirementFor } from "./aiSettings.svelte";

class AutoTemplateStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);

  suggesting = $state(false);
  result = $state<AiTemplateRecommendation | null>(null);
  lastError = $state<string | null>(null);

  /** Toggled by "Change Template" — shows the real full catalog inline (see
   * module doc comment) instead of a second picker mechanism. */
  browsingCatalog = $state(false);

  /** Name of whatever template was actually applied this session (via
   * Accept or a Change Template pick) — drives whether Customize/Run are
   * enabled, and what note to show. `null` until something real has been
   * applied. */
  appliedTemplateName = $state<string | null>(null);

  /** Surfaces `templatesStore.applyError` at the moment Accept/a
   * Change-Template pick was clicked — read once right after the call
   * rather than derived continuously, since `templatesStore.applyError` is
   * shared, cross-dialog state (`TemplatesPanel.svelte` can set/clear it
   * too). */
  applyError = $state<string | null>(null);

  // ---- Derived selection context (same shape as sibling detectors) ------

  eligibleTracks = $derived.by((): Track[] => {
    return timeline.tracks.filter((t) => t.kind === "audio" || t.kind === "video");
  });

  clipsForSelectedTrack = $derived.by((): Clip[] => {
    if (!this.trackId) return [];
    return (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null);
  });

  selectedClip = $derived.by((): Clip | null => {
    if (!this.clipId) return null;
    return this.clipsForSelectedTrack.find((c) => c.id === this.clipId) ?? null;
  });

  selectedMedia = $derived.by((): MediaItem | null => {
    const clip = this.selectedClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  transcriptEntries = $derived.by((): TranscriptEntry[] => {
    const media = this.selectedMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? [])
      .filter((e) => e.media_id === media.id)
      .sort((a, b) => a.start_us - b.start_us);
  });

  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );

  /** The recommended template resolved against the real catalog
   * (`templatesStore.allTemplates`) — `null` while no result exists yet, or
   * on the (should-never-happen, since the backend validates this itself)
   * chance the id no longer resolves locally. */
  recommendedTemplate = $derived.by((): Template | null => {
    if (!this.result) return null;
    return templatesStore.allTemplates.find((t) => t.id === this.result?.template_id) ?? null;
  });

  canSuggest = $derived(this.selectedMedia !== null && !this.suggesting);
  canCustomizeOrRun = $derived(this.appliedTemplateName !== null);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openFor(opts: { trackId?: string; clipId?: string } = {}): void {
    this.resetResults();
    this.open = true;
    void templatesStore.ensureLoaded();
    const tracks = this.eligibleTracks;
    this.trackId = opts.trackId ?? tracks[0]?.id ?? null;
    const clips = this.trackId ? (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null) : [];
    this.clipId = opts.clipId ?? clips[0]?.id ?? null;
  }

  close(): void {
    this.open = false;
    this.resetResults();
  }

  private resetResults(): void {
    this.result = null;
    this.lastError = null;
    this.browsingCatalog = false;
    this.appliedTemplateName = null;
    this.applyError = null;
  }

  setTrack(trackId: string): void {
    if (trackId === this.trackId) return;
    this.trackId = trackId;
    const clips = (timeline.clipsByTrack.get(trackId) ?? []).filter((c) => c.media_id !== null);
    this.clipId = clips[0]?.id ?? null;
    this.resetResults();
  }

  setClip(clipId: string): void {
    if (clipId === this.clipId) return;
    this.clipId = clipId;
    this.resetResults();
  }

  // -------------------------------------------------------------------
  // Suggest
  // -------------------------------------------------------------------

  async suggest(): Promise<void> {
    const media = this.selectedMedia;
    if (!this.canSuggest || !media) return;
    this.suggesting = true;
    this.lastError = null;
    this.result = null;
    this.browsingCatalog = false;
    try {
      const settings = currentAiProviderSettings();
      const result = await commands.suggestTemplateForMedia(media.source_path, this.transcriptEntries, settings);
      if (result.status === "ok") {
        this.result = result.data;
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.suggesting = false;
    }
  }

  // -------------------------------------------------------------------
  // Accept / Change Template (both apply through templatesStore.applyToProject)
  // -------------------------------------------------------------------

  async accept(): Promise<void> {
    const template = this.recommendedTemplate;
    if (!template) return;
    await this.applyTemplate(template);
  }

  toggleBrowseCatalog(): void {
    this.browsingCatalog = !this.browsingCatalog;
  }

  async applyTemplate(template: Template): Promise<void> {
    this.applyError = null;
    await templatesStore.applyToProject(template);
    if (templatesStore.applyError) {
      this.applyError = templatesStore.applyError;
      return;
    }
    this.appliedTemplateName = template.name;
    this.browsingCatalog = false;
  }

  // -------------------------------------------------------------------
  // Customize / Run (see module doc comment)
  // -------------------------------------------------------------------

  openCustomize(): void {
    if (!this.canCustomizeOrRun) return;
    templatesStore.openSaveForm();
  }

  openRun(): void {
    if (!this.canCustomizeOrRun) return;
    renderStore.openDialog();
  }
}

export const autoTemplate = new AutoTemplateStore();
