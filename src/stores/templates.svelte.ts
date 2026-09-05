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
  ZoomIntensity,
} from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { captionsStore } from "./captions.svelte";
import { renderStore } from "./render.svelte";
import { silenceDetector } from "./silenceDetector.svelte";
import { autoZoom } from "./autoZoom.svelte";

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
  }

  closeSaveForm(): void {
    this.saveFormOpen = false;
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
      const result = await commands.saveAsTemplate(snap(project), input);
      if (result.status === "ok") {
        this.saveFormOpen = false;
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
}

export const templatesStore = new TemplatesStore();
