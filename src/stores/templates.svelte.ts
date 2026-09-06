// Svelte 5 runes-based store for the Phase 11 Templates panel (master
// prompt §36/§37, `src-tauri/src/templates/` /
// `src-tauri/src/commands/templates.rs`). Mounted as `LeftPanel.svelte`'s
// real "Templates" tab (replacing that tab's Phase-11 placeholder) — the
// exact spot the master-prompt §48 layout already reserved for it, unlike
// the other three Phase 11 passes in this same task which needed a new
// placement decision.
//
// "Apply to Project" scope (documented judgment call, task brief: "your
// call on exactly which settings get pushed into the live project vs. just
// used as defaults"): `Template.canvas`/`caption_style` are real, persisted
// `ProjectV1` fields (`ProjectV1::canvas`/`ProjectV1::caption_styles`,
// per `Template`'s own doc comment) — Apply pushes both into the *live*
// session project for real (`timeline.setCanvas` + the existing
// `set_caption_styles` command). `zoom_intensity`/`silence_settings`/
// `export_preset_id` are NOT persisted `ProjectV1` fields anywhere in this
// codebase (`zoom`/`vad::cutlist`/`render::presets`'s own doc comments: they
// are one-shot command parameters, not project-wide "current setting"
// state) — Apply instead pre-fills the *already-built* controls that
// consume them this session: `stores/autoZoom.svelte.ts`'s intensity
// picker, `stores/silenceDetector.svelte.ts`'s ms sliders, and
// `stores/render.svelte.ts`'s selected preset. `transition_settings`/
// `ai_prompt_config`/`sports_overlay` are honestly structural-only even in
// `templates` itself (module doc comment: no consuming render/AI-prompt
// mechanism exists yet) — Apply does not pretend to do anything with them
// beyond having captured them in the template.
//
// Save as Template mirrors that same read/write split: it *captures*
// zoom_intensity/silence_settings/transition_settings/export_preset_id/
// ai_prompt_config/sports_overlay from this session's own current form
// state (the same controls Apply pre-fills), since none of those are
// readable off `ProjectV1` itself — only `canvas`/`caption_style_id` are
// read directly from the live project (`save_as_template`'s own contract).
//
// Upgrade U3 pass: added the intro/outro/watermark/background-music
// asset-by-id pickers (upgrade spec §17/§3, reading from the shared
// `stores/assets.svelte.ts` catalog), an "edit an existing custom template
// in place" flow (`openEditForm`/`editingTemplateId`, calling the real
// `updateCustomTemplate` command instead of `saveAsTemplate` — upgrade spec
// §20), and a version-history viewer (`openHistory`, stepping through
// `getTemplateVersion` one version number at a time — see that method's own
// doc comment for why: no "list template versions" command exists on the
// real backend surface to enumerate them in one call).

import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type {
  AudioRole,
  CaptionStyle,
  DuckingSettings,
  SaveAsTemplateInput,
  SmartEditCategory,
  SportsOverlaySettings,
  Template,
  TemplateCatalog,
  TransitionType,
  WatermarkPosition,
  ZoomIntensity,
} from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { captionsStore } from "./captions.svelte";
import { renderStore } from "./render.svelte";
import { silenceDetector } from "./silenceDetector.svelte";
import { autoZoom } from "./autoZoom.svelte";
import { assetsStore } from "./assets.svelte";

export const ALL_WATERMARK_POSITIONS: WatermarkPosition[] = [
  "top_left",
  "top_right",
  "bottom_left",
  "bottom_right",
  "center",
];

/** `Template::version` is `#[serde(default = "default_template_version")]`
 * on the Rust side, which `specta` conservatively types as `number |
 * undefined` in `bindings.ts` (the field is always actually present once
 * serialized — the `serde(default)` only ever matters for *deserializing* an
 * old on-disk JSON file that predates this field) — this helper is the one
 * place that `?? 1` fallback lives, reused by both this store and
 * `TemplatesPanel.svelte` so every version display/comparison agrees. */
export function templateVersion(template: Template): number {
  return template.version ?? 1;
}

export const ALL_SMART_EDIT_CATEGORIES: SmartEditCategory[] = [
  "repetition",
  "false_start",
  "off_topic",
  "weak_sentence",
  "long_pause",
  "filler_word",
  "unnecessary_intro",
  "duplicate_idea",
  "boring_section",
];

function snap<T>(value: T): T {
  return $state.snapshot(value) as T;
}

class TemplatesStore {
  catalog = $state<TemplateCatalog>({ built_in: [], custom: [] });
  loading = $state(false);
  loadError = $state<string | null>(null);

  applyingId = $state<string | null>(null);
  applyError = $state<string | null>(null);
  lastAppliedName = $state<string | null>(null);

  importing = $state(false);
  importError = $state<string | null>(null);
  exportingId = $state<string | null>(null);
  exportError = $state<string | null>(null);
  pendingDeleteId = $state<string | null>(null);
  deletingId = $state<string | null>(null);
  deleteError = $state<string | null>(null);

  // ---- Save as Template form (see class doc comment) ---------------------

  saveFormOpen = $state(false);
  saveName = $state("");
  saveDescription = $state("");
  saveCaptionStyleId = $state<string | null>(null);
  saveZoomIntensity = $state<ZoomIntensity>("medium");
  saveTransitionType = $state<TransitionType>("cut");
  saveTransitionDurationMs = $state(150);
  saveExportPresetId = $state<string | null>(null);
  saveEmphasizedCategories = $state<Set<SmartEditCategory>>(new Set());
  saveSystemPromptPrefix = $state("");
  saveIncludeSportsOverlay = $state(false);
  saving = $state(false);
  saveError = $state<string | null>(null);

  // -- Upgrade spec §17: asset-by-id references on the save/edit form ------
  saveIntroAssetId = $state<string | null>(null);
  saveOutroAssetId = $state<string | null>(null);
  saveWatermarkAssetId = $state<string | null>(null);
  saveWatermarkPosition = $state<WatermarkPosition>("top_right");
  saveMusicAssetId = $state<string | null>(null);
  saveMusicVolume = $state(1.0);

  // -- Upgrade spec §20: editing an existing custom template in place ------
  /** `null` = the save form is building a brand-new template (`saveAsTemplate`).
   * Set to an existing custom template's id by `openEditForm` — `submitSave`
   * then calls `updateCustomTemplate` instead, bumping that template's
   * version rather than creating a second, separate template. */
  editingTemplateId = $state<string | null>(null);
  lastUpdatedVersion = $state<number | null>(null);

  // -- Upgrade spec §20: version history viewer -----------------------------
  historyTemplateId = $state<string | null>(null);
  historyEntries = $state<Template[]>([]);
  historyLoading = $state(false);
  historyError = $state<string | null>(null);

  allTemplates = $derived<Template[]>([...this.catalog.built_in, ...this.catalog.custom]);

  constructor() {
    void this.ensureLoaded();
  }

  async ensureLoaded(): Promise<void> {
    if (this.catalog.built_in.length > 0 || this.loading) return;
    await this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading = true;
    this.loadError = null;
    try {
      const result = await commands.listTemplates();
      if (result.status === "ok") {
        this.catalog = result.data;
      } else {
        this.loadError = result.error.message;
      }
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loading = false;
    }
  }

  // -------------------------------------------------------------------
  // Apply to project (see class doc comment for the real-vs-prefill split)
  // -------------------------------------------------------------------

  async applyToProject(template: Template): Promise<void> {
    if (!timeline.project || this.applyingId !== null) return;
    this.applyingId = template.id;
    this.applyError = null;
    this.lastAppliedName = null;
    try {
      await timeline.setCanvas(template.canvas);

      const existingStyles = timeline.project?.caption_styles ?? [];
      const mergedStyles: CaptionStyle[] = existingStyles.some((s) => s.id === template.caption_style.id)
        ? existingStyles.map((s) => (s.id === template.caption_style.id ? template.caption_style : s))
        : [...existingStyles, template.caption_style];
      const outcome = await timeline.applyExternalProjectResult(commands.setCaptionStyles(mergedStyles));
      if (!outcome.ok) {
        this.applyError = outcome.error;
        return;
      }
      captionsStore.editStyle(template.caption_style.id);

      // Pre-fill session-local controls that have no persisted ProjectV1
      // home (see class doc comment).
      autoZoom.intensity = template.zoom_intensity;
      silenceDetector.paddingBeforeMs = Math.round(template.silence_settings.padding_before_us / 1000);
      silenceDetector.paddingAfterMs = Math.round(template.silence_settings.padding_after_us / 1000);
      silenceDetector.mergeGapMs = Math.round(template.silence_settings.merge_gap_us / 1000);
      await renderStore.ensurePresetsLoaded();
      renderStore.selectPreset(template.export_preset_id);

      this.lastAppliedName = template.name;
    } catch (err) {
      this.applyError = String(err);
    } finally {
      this.applyingId = null;
    }
  }

  // -------------------------------------------------------------------
  // Import / Export / Delete
  // -------------------------------------------------------------------

  async importTemplate(): Promise<void> {
    if (this.importing) return;
    const chosen = await open({ multiple: false, filters: [{ name: "Template", extensions: ["json"] }] });
    if (!chosen || typeof chosen !== "string") return;
    this.importing = true;
    this.importError = null;
    try {
      const result = await commands.importTemplate(chosen);
      if (result.status === "ok") {
        await this.refresh();
      } else {
        this.importError = result.error.message;
      }
    } catch (err) {
      this.importError = String(err);
    } finally {
      this.importing = false;
    }
  }

  async exportTemplate(template: Template): Promise<void> {
    if (this.exportingId !== null) return;
    const chosen = await save({
      filters: [{ name: "Template", extensions: ["json"] }],
      defaultPath: `${template.id}.json`,
    });
    if (!chosen) return;
    this.exportingId = template.id;
    this.exportError = null;
    try {
      const result = await commands.exportTemplate(template.id, chosen);
      if (result.status !== "ok") this.exportError = result.error.message;
    } catch (err) {
      this.exportError = String(err);
    } finally {
      this.exportingId = null;
    }
  }

  armDelete(templateId: string): void {
    this.pendingDeleteId = templateId;
  }

  cancelDelete(): void {
    this.pendingDeleteId = null;
  }

  async confirmDelete(templateId: string): Promise<void> {
    if (this.pendingDeleteId !== templateId || this.deletingId !== null) return;
    this.deletingId = templateId;
    this.deleteError = null;
    try {
      const result = await commands.deleteCustomTemplate(templateId);
      if (result.status === "ok") {
        this.pendingDeleteId = null;
        await this.refresh();
      } else {
        this.deleteError = result.error.message;
      }
    } catch (err) {
      this.deleteError = String(err);
    } finally {
      this.deletingId = null;
    }
  }

  // -------------------------------------------------------------------
  // Save as Template
  // -------------------------------------------------------------------

  openSaveForm(): void {
    this.saveFormOpen = true;
    this.editingTemplateId = null;
    this.saveError = null;
    this.saveName = "";
    this.saveDescription = "";
    this.saveCaptionStyleId = captionsStore.draftSourceId ?? captionsStore.catalog[0]?.id ?? null;
    this.saveZoomIntensity = autoZoom.intensity;
    this.saveExportPresetId = renderStore.selectedPresetId;
    this.saveTransitionType = "cut";
    this.saveTransitionDurationMs = 150;
    this.saveEmphasizedCategories = new Set();
    this.saveSystemPromptPrefix = "";
    this.saveIncludeSportsOverlay = false;
    this.saveIntroAssetId = null;
    this.saveOutroAssetId = null;
    this.saveWatermarkAssetId = null;
    this.saveWatermarkPosition = "top_right";
    this.saveMusicAssetId = null;
    this.saveMusicVolume = 1.0;
    void assetsStore.ensureLoaded();
  }

  /** Upgrade spec §20's "edit an existing custom template" flow: opens the
   * same save-form dialog, pre-filled from `template`'s own current values,
   * with `editingTemplateId` set so `submitSave` calls `updateCustomTemplate`
   * (bumping `version`) instead of creating a brand-new template. Refuses to
   * open for a built-in (mirrors the backend's own `CannotEditBuiltIn`
   * guard — a built-in's "Edit" action is never shown in the UI at all, see
   * `TemplatesPanel.svelte`). Note: `silence_settings` is, same as
   * `openSaveForm`/`submitSave`'s own long-standing convention (see class
   * doc comment), captured from the *live* Silence Detector session state at
   * submit time, not read back from the template being edited — there is no
   * persisted "current silence setting" this form could pre-fill it from. */
  openEditForm(template: Template): void {
    if (template.is_built_in) return;
    this.saveFormOpen = true;
    this.editingTemplateId = template.id;
    this.saveError = null;
    this.saveName = template.name;
    this.saveDescription = template.description;
    this.saveCaptionStyleId = template.caption_style.id;
    this.saveZoomIntensity = template.zoom_intensity;
    this.saveExportPresetId = template.export_preset_id;
    this.saveTransitionType = template.transition_settings.transition_type;
    this.saveTransitionDurationMs = Math.round(template.transition_settings.duration_us / 1000);
    this.saveEmphasizedCategories = new Set(template.ai_prompt_config.emphasized_categories);
    this.saveSystemPromptPrefix = template.ai_prompt_config.system_prompt_prefix ?? "";
    this.saveIncludeSportsOverlay = template.sports_overlay !== null;
    this.saveIntroAssetId = template.intro?.asset_id ?? null;
    this.saveOutroAssetId = template.outro?.asset_id ?? null;
    this.saveWatermarkAssetId = template.watermark?.asset_id ?? null;
    this.saveWatermarkPosition = template.watermark?.position ?? "top_right";
    this.saveMusicAssetId = template.background_music?.asset_id ?? null;
    this.saveMusicVolume = template.background_music?.volume ?? 1.0;
    void assetsStore.ensureLoaded();
  }

  closeSaveForm(): void {
    this.saveFormOpen = false;
    this.editingTemplateId = null;
  }

  toggleEmphasizedCategory(category: SmartEditCategory): void {
    const next = new Set(this.saveEmphasizedCategories);
    if (next.has(category)) next.delete(category);
    else next.add(category);
    this.saveEmphasizedCategories = next;
  }

  private buildSaveInput(): SaveAsTemplateInput | null {
    if (!this.saveCaptionStyleId || !this.saveExportPresetId) return null;
    const sportsOverlay: SportsOverlaySettings | null = this.saveIncludeSportsOverlay
      ? {
          score_overlay_suggested: true,
          music_role: "music" as AudioRole,
          music_ducking: { duck_level: 0.25, attack_us: 300_000, release_us: 500_000 } as DuckingSettings,
        }
      : null;
    return {
      name: this.saveName.trim() || "Untitled Template",
      description: this.saveDescription.trim(),
      caption_style_id: this.saveCaptionStyleId,
      zoom_intensity: this.saveZoomIntensity,
      silence_settings: silenceDetector.cutParams,
      transition_settings: { transition_type: this.saveTransitionType, duration_us: this.saveTransitionDurationMs * 1000 },
      export_preset_id: this.saveExportPresetId,
      ai_prompt_config: {
        emphasized_categories: Array.from(this.saveEmphasizedCategories),
        system_prompt_prefix: this.saveSystemPromptPrefix.trim() || null,
      },
      sports_overlay: sportsOverlay,
      // Upgrade spec §3/§17: optional asset-by-id references. `null` (the
      // form's default) is the deliberate, obvious "not set" choice for
      // every one of these — never a picker accidentally left on an
      // arbitrary first catalog entry.
      intro: this.saveIntroAssetId ? { asset_id: this.saveIntroAssetId } : null,
      outro: this.saveOutroAssetId ? { asset_id: this.saveOutroAssetId } : null,
      watermark: this.saveWatermarkAssetId
        ? { asset_id: this.saveWatermarkAssetId, position: this.saveWatermarkPosition }
        : null,
      background_music: this.saveMusicAssetId
        ? { asset_id: this.saveMusicAssetId, volume: this.saveMusicVolume }
        : null,
    };
  }

  async submitSave(): Promise<void> {
    const project = timeline.project;
    if (!project || this.saving) return;
    const input = this.buildSaveInput();
    if (!input) {
      this.saveError = "Pick a caption style and export preset first.";
      return;
    }
    this.saving = true;
    this.saveError = null;
    try {
      // Upgrade spec §20: editing an existing custom template calls
      // `updateCustomTemplate` (bumps `version`, preserves history) instead
      // of `saveAsTemplate` (which always creates a brand-new template) —
      // see `openEditForm`'s own doc comment.
      const result = this.editingTemplateId
        ? await commands.updateCustomTemplate(this.editingTemplateId, snap(project), input)
        : await commands.saveAsTemplate(snap(project), input);
      if (result.status === "ok") {
        this.saveFormOpen = false;
        this.lastUpdatedVersion = this.editingTemplateId ? templateVersion(result.data) : null;
        this.editingTemplateId = null;
        await this.refresh();
      } else {
        this.saveError = result.error.message;
      }
    } catch (err) {
      this.saveError = String(err);
    } finally {
      this.saving = false;
    }
  }

  // -------------------------------------------------------------------
  // Version history (upgrade spec §20)
  // -------------------------------------------------------------------

  /** Steps through every prior version of a custom template one at a time
   * via `getTemplateVersion` (versions `1..template.version - 1` — the
   * current version is already `template` itself, no need to re-fetch it).
   * No "list versions" command exists on the real backend surface
   * (`commands::templates` only has `get_template_version(id, version)`,
   * checked directly against `src-tauri/src/commands/templates.rs` before
   * writing this) — this is the documented workaround: every version number
   * from 1 up to (but excluding) the current one is known to exist for a
   * template that has been updated at least once (each `update_custom_template`
   * call appends exactly one history entry per prior version), so stepping
   * through by number and collecting whichever resolve is the only
   * available option. */
  async openHistory(template: Template): Promise<void> {
    this.historyTemplateId = template.id;
    this.historyEntries = [];
    this.historyError = null;
    const currentVersion = templateVersion(template);
    if (currentVersion <= 1) return;
    this.historyLoading = true;
    try {
      const versions = Array.from({ length: currentVersion - 1 }, (_, i) => i + 1);
      const results = await Promise.all(versions.map((v) => commands.getTemplateVersion(template.id, v)));
      const entries: Template[] = [];
      for (const result of results) {
        if (result.status === "ok") {
          entries.push(result.data);
        } else {
          this.historyError = result.error.message;
        }
      }
      this.historyEntries = entries;
    } catch (err) {
      this.historyError = String(err);
    } finally {
      this.historyLoading = false;
    }
  }

  closeHistory(): void {
    this.historyTemplateId = null;
    this.historyEntries = [];
    this.historyError = null;
  }
}

export const templatesStore = new TemplatesStore();
