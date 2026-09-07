// Svelte 5 runes-based store for Smart Automation rules (upgrade spec §27,
// `UPGRADE_PLAN.md` Phase U4 — the backend rule engine already shipped in an
// earlier pass: `src-tauri/src/automation/`, `commands/automation.rs`).
// Structurally mirrors `stores/assets.svelte.ts`'s "list a catalog, add via a
// real backend command, remove with a two-step confirm" shape (a handful of
// user-defined rules, not a paginated table like History), with one more
// per-row action `assets.svelte.ts` doesn't need: an immediate,
// no-confirmation enable/disable toggle (`set_automation_rule_enabled`'s own
// doc comment: reversible, unlike delete).
//
// Reuses `stores/templates.svelte.ts`'s already-loaded template catalog
// (`templatesStore.allTemplates`) for both the Create Rule form's
// single/multi-template picker and the rule list's action-summary name
// lookup, rather than a second `list_templates` fetch — the exact same
// catalog `StartBatchDialog.svelte` already reads from.
//
// Create Rule form scope (documented judgment call, task brief): mirrors
// `StartBatchDialog.svelte`'s own established "single template vs.
// multi-template" toggle and export-preset picker verbatim (a rule's action
// is literally the same `BatchPipelineConfig`/`template_ids` shape that
// dialog already builds) — but, unlike that dialog, this form does NOT
// expose silence-removal or caption generation. A rule doesn't need every
// batch knob on day one; §27's own worked example ("new video -> apply
// template -> export") never mentions per-rule silence/caption tuning, and a
// user who needs that level of control can already reach it via Batch's own
// Start Batch dialog once the file exists. `buildConfig()` below always
// sends `remove_silence: null, captions: null` — a deliberate, honest scope
// decision, not a hidden limitation (see `UPGRADE_PLAN.md`'s Phase U4
// frontend writeup for the same note).
//
// `condition` can only be *set* through this store's Create Rule form, never
// *cleared* on an existing rule — `update_automation_rule`'s own doc comment
// explains why (no way to disambiguate "leave unchanged" from "clear" over
// the Tauri IPC boundary without a second flag the real backend doesn't
// have). This store does not attempt to work around that; a rule's
// condition is fixed at creation time. In practice this store doesn't even
// call `update_automation_rule` at all today — only create/toggle/delete are
// wired, matching exactly what the Create-Rule-only, no-in-place-edit v1
// scope in this pass's own task brief asks for.
//
// `.svelte.ts` (not `.ts`) is required for `$state`/`$derived` to work
// outside a `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type {
  AutomationAction,
  AutomationCondition,
  AutomationRule,
  AutomationTrigger,
  BatchPipelineConfig,
  RenderPreset,
} from "../types/bindings";
import { templatesStore } from "./templates.svelte";

class AutomationStore {
  open = $state(false);

  rules = $state<AutomationRule[]>([]);
  loading = $state(false);
  loadError = $state<string | null>(null);
  private loaded = false;

  // ---- Per-row enable/disable toggle (immediate, no confirmation — see
  //      module doc comment) -------------------------------------------

  togglingById = $state<Record<string, boolean>>({});
  toggleErrorById = $state<Record<string, string | null>>({});

  // ---- Delete (two-step confirm, same arm/cancel/confirm shape
  //      `stores/assets.svelte.ts`'s own remove already uses — deleting a
  //      rule stops a live watcher, deliberately not a single-click action) -

  pendingDeleteId = $state<string | null>(null);
  deletingId = $state<string | null>(null);
  deleteError = $state<string | null>(null);

  // ---- Create Rule form ------------------------------------------------

  showCreateForm = $state(false);
  createName = $state("");
  createFolderPath = $state<string | null>(null);

  /** Stored as whole minutes at the UI boundary, converted to/from the real
   * `min_seconds: f64` field only in `buildCondition()` — matching
   * `StartBatchDialog.svelte`'s own established ms<->µs
   * conversion-at-the-boundary convention. */
  createConditionEnabled = $state(false);
  createMinDurationMinutes = $state(5);

  /** Single/multi-template toggle — inlines
   * `StartBatchDialog.svelte`'s own established pattern verbatim (see module
   * doc comment). */
  createMultiTemplateMode = $state(false);
  createTemplateId = $state<string | null>(null);
  createTemplateIds = $state<string[]>([]);
  createExportPresetId = $state<string | null>(null);

  creating = $state(false);
  createError = $state<string | null>(null);

  presets = $state<RenderPreset[]>([]);
  private presetsLoaded = false;

  /** The shared template catalog `StartBatchDialog.svelte` also reads from —
   * see module doc comment for why this store never fetches its own copy. */
  get templates() {
    return templatesStore.allTemplates;
  }

  canSubmitCreate = $derived(
    this.createName.trim().length > 0 &&
      this.createFolderPath !== null &&
      !this.creating &&
      (this.createMultiTemplateMode
        ? this.createTemplateIds.length > 0
        : this.createTemplateId !== null || this.createExportPresetId !== null),
  );

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    this.pendingDeleteId = null;
    this.showCreateForm = false;
    void this.ensureLoaded();
    void templatesStore.ensureLoaded();
    void this.ensurePresetsLoaded();
  }

  close(): void {
    this.open = false;
    this.pendingDeleteId = null;
    this.showCreateForm = false;
  }

  async ensureLoaded(): Promise<void> {
    if (this.loaded || this.loading) return;
    await this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading = true;
    this.loadError = null;
    try {
      const result = await commands.listAutomationRules();
      if (result.status === "ok") {
        this.rules = result.data;
        this.loaded = true;
      } else {
        this.loadError = result.error.message;
      }
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loading = false;
    }
  }

  private async ensurePresetsLoaded(): Promise<void> {
    if (this.presetsLoaded) return;
    this.presets = await commands.listRenderPresets();
    this.presetsLoaded = true;
    if (this.createExportPresetId === null) {
      const [firstPreset] = this.presets;
      if (firstPreset) {
        const default1080 = this.presets.find((p) => p.id === "p1080");
        this.createExportPresetId = (default1080 ?? firstPreset).id;
      }
    }
  }

  /** Resolves a template id against the shared catalog — used by both the
   * rule list's action summary and, indirectly, by callers that already have
   * a `Template` in hand elsewhere. Falls back to the raw id if the template
   * was since deleted (a rule can outlive the custom template it referenced —
   * `templates::delete_custom_template` never cascades into automation
   * rules, since this module doesn't reach into that one). */
  templateName(templateId: string): string {
    return this.templates.find((tpl) => tpl.id === templateId)?.name ?? templateId;
  }

  // -------------------------------------------------------------------
  // Enable/disable toggle
  // -------------------------------------------------------------------

  async setEnabled(rule: AutomationRule, enabled: boolean): Promise<void> {
    if (this.togglingById[rule.id]) return;
    this.togglingById[rule.id] = true;
    this.toggleErrorById[rule.id] = null;
    try {
      const result = await commands.setAutomationRuleEnabled(rule.id, enabled);
      if (result.status === "ok") {
        this.rules = this.rules.map((r) => (r.id === rule.id ? result.data : r));
      } else {
        this.toggleErrorById[rule.id] = result.error.message;
      }
    } catch (err) {
      this.toggleErrorById[rule.id] = String(err);
    } finally {
      this.togglingById[rule.id] = false;
    }
  }

  // -------------------------------------------------------------------
  // Create Rule form
  // -------------------------------------------------------------------

  openCreateForm(): void {
    this.createName = "";
    this.createFolderPath = null;
    this.createConditionEnabled = false;
    this.createMinDurationMinutes = 5;
    this.createMultiTemplateMode = false;
    this.createTemplateId = null;
    this.createTemplateIds = [];
    this.createError = null;
    // `createExportPresetId` deliberately not reset — keeps whatever default
    // `ensurePresetsLoaded()` already picked (or the user's last choice)
    // across repeated rule creation in one dialog session.
    this.showCreateForm = true;
  }

  closeCreateForm(): void {
    this.showCreateForm = false;
  }

  /** Real native folder picker (`@tauri-apps/plugin-dialog`), the same
   * `open({ directory: true })` pattern `MediaLibrary.svelte`'s own
   * `pickFolder` already uses — a *folder* picker, not a file picker, since
   * `AutomationTrigger::WatchFolder` watches a directory. */
  async pickFolder(): Promise<void> {
    const selected = await open({ directory: true });
    if (selected && typeof selected === "string") {
      this.createFolderPath = selected;
    }
  }

  toggleCreateTemplateSelection(id: string): void {
    this.createTemplateIds = this.createTemplateIds.includes(id)
      ? this.createTemplateIds.filter((existing) => existing !== id)
      : [...this.createTemplateIds, id];
  }

  /** Deliberately omits `remove_silence`/`captions` — see module doc
   * comment's scope note. */
  private buildConfig(): BatchPipelineConfig {
    return {
      remove_silence: null,
      captions: null,
      transcription_model_id: null,
      transcription_language: null,
      template_id: this.createMultiTemplateMode ? null : this.createTemplateId,
      export_preset_id: this.createExportPresetId,
      output_suffix: null,
    };
  }

  private buildCondition(): AutomationCondition | null {
    if (!this.createConditionEnabled) return null;
    const minutes = Math.max(0, this.createMinDurationMinutes);
    return { kind: "min_duration_seconds", min_seconds: minutes * 60 };
  }

  async submitCreate(): Promise<void> {
    if (!this.canSubmitCreate || !this.createFolderPath) return;
    this.creating = true;
    this.createError = null;
    try {
      const trigger: AutomationTrigger = { kind: "watch_folder", path: this.createFolderPath };
      const action: AutomationAction = {
        kind: "run_pipeline",
        config: this.buildConfig(),
        template_ids: this.createMultiTemplateMode ? this.createTemplateIds : null,
      };
      const result = await commands.createAutomationRule(
        this.createName.trim(),
        trigger,
        this.buildCondition(),
        action,
      );
      if (result.status === "ok") {
        this.rules = [...this.rules, result.data];
        this.closeCreateForm();
      } else {
        this.createError = result.error.message;
      }
    } catch (err) {
      this.createError = String(err);
    } finally {
      this.creating = false;
    }
  }

  // -------------------------------------------------------------------
  // Delete (arm/confirm — module doc comment)
  // -------------------------------------------------------------------

  armDelete(ruleId: string): void {
    this.pendingDeleteId = ruleId;
  }

  cancelDelete(): void {
    this.pendingDeleteId = null;
  }

  async confirmDelete(ruleId: string): Promise<void> {
    if (this.pendingDeleteId !== ruleId || this.deletingId !== null) return;
    this.deletingId = ruleId;
    this.deleteError = null;
    try {
      const result = await commands.deleteAutomationRule(ruleId);
      if (result.status === "ok") {
        this.pendingDeleteId = null;
        this.rules = this.rules.filter((r) => r.id !== ruleId);
      } else {
        this.deleteError = result.error.message;
      }
    } catch (err) {
      this.deleteError = String(err);
    } finally {
      this.deletingId = null;
    }
  }
}

export const automationStore = new AutomationStore();
