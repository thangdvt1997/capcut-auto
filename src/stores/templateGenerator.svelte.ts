// Svelte 5 runes-based store for the AI Template Generator dialog (upgrade
// spec §8, `UPGRADE_PLAN.md` Phase U2): "Natural language -> Template
// Definition -> Validate -> Template Builder -> Preview -> Save Template".
// `generate_template_from_prompt` (`src-tauri/src/commands/ai.rs`) already
// chains Generate -> Validate -> Template Builder server-side (its own doc
// comment: returns a real, ready-to-preview `Template`, or a clear
// `AppErrorPayload` — never a partially-built one) — this store's own job is
// just Preview (a structured, human-readable summary, not a JSON dump) and
// the explicit, separate Save Template step
// (`commands::templates::save_generated_template`), mirroring
// `stores/aiNlCommand.svelte.ts`'s own "never a second, parallel validation
// pass" discipline.
//
// ## Asset name resolution reuses the real Asset Library store
//
// `generate_template_from_prompt` resolves asset ids against the real Asset
// Library server-side already, but the *frontend* preview still wants to
// show a human name, not a bare `asset_...` id, for any `intro`/`outro`/
// `watermark`/`background_music` reference the AI actually picked. The
// concurrently-built Asset Library pass (upgrade spec §17) added a real
// shared catalog store, `stores/assets.svelte.ts` (`assetsStore`) —
// `stores/templates.svelte.ts`'s own save/edit form already reads from it
// the same way. This store does the same (`assetsStore.ensureLoaded()` +
// `assetsStore.assets`), never a second, duplicated asset fetch.
//
// ## Why Save calls `templatesStore.refresh()`
//
// `save_generated_template` persists to the same on-disk custom-template
// directory `commands::templates::save_as_template`/`import_template`
// already write to — so the newly saved template belongs in the exact same
// gallery `TemplatesPanel.svelte` renders. Rather than duplicating that
// panel's own catalog state, this store reuses the existing
// `templatesStore` singleton and calls its real `refresh()` after a
// successful save, so the new custom template shows up there immediately,
// the same list every other apply/export/delete action already reads from.

import { commands } from "../types/bindings";
import type { Template } from "../types/bindings";
import { templatesStore } from "./templates.svelte";
import { renderStore } from "./render.svelte";
import { assetsStore } from "./assets.svelte";
import { currentAiProviderSettings } from "./aiSettings.svelte";

class TemplateGeneratorStore {
  open = $state(false);

  nlPrompt = $state("");

  generating = $state(false);
  generatedTemplate = $state<Template | null>(null);
  lastError = $state<string | null>(null);

  saving = $state(false);
  saveError = $state<string | null>(null);
  /** Set once Save has succeeded this session, for an honest "already
   * saved" note — `generatedTemplate` itself is left in place afterwards so
   * the preview stays visible instead of vanishing. */
  savedTemplateName = $state<string | null>(null);

  canGenerate = $derived(this.nlPrompt.trim().length > 0 && !this.generating);
  canSave = $derived(this.generatedTemplate !== null && !this.saving);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    void renderStore.ensurePresetsLoaded();
    void assetsStore.ensureLoaded();
  }

  close(): void {
    this.open = false;
  }

  reset(): void {
    this.nlPrompt = "";
    this.generatedTemplate = null;
    this.lastError = null;
    this.saveError = null;
    this.savedTemplateName = null;
  }

  /** A human label for an asset id referenced by a generated template's
   * `intro`/`outro`/`watermark`/`background_music` — falls back to the bare
   * id if `assetsStore`'s shared catalog hasn't resolved it yet. */
  assetLabel(assetId: string): string {
    const asset = assetsStore.assets.find((a) => a.id === assetId);
    return asset ? asset.name : assetId;
  }

  presetLabel(presetId: string): string {
    return renderStore.presets.find((p) => p.id === presetId)?.name ?? presetId;
  }

  // -------------------------------------------------------------------
  // Generate -> Validate -> Template Builder (already chained server-side)
  // -------------------------------------------------------------------

  async generate(): Promise<void> {
    if (!this.canGenerate) return;
    this.generating = true;
    this.lastError = null;
    this.generatedTemplate = null;
    this.saveError = null;
    this.savedTemplateName = null;
    try {
      const settings = currentAiProviderSettings();
      const result = await commands.generateTemplateFromPrompt(this.nlPrompt, settings);
      if (result.status === "ok") {
        this.generatedTemplate = result.data;
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.generating = false;
    }
  }

  // -------------------------------------------------------------------
  // Save Template (separate, explicit step — upgrade spec §8's own pipeline)
  // -------------------------------------------------------------------

  async saveTemplate(): Promise<void> {
    const template = this.generatedTemplate;
    if (!this.canSave || !template) return;
    this.saving = true;
    this.saveError = null;
    try {
      const result = await commands.saveGeneratedTemplate(template);
      if (result.status === "ok") {
        this.savedTemplateName = result.data.name;
        await templatesStore.refresh();
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

export const templateGenerator = new TemplateGeneratorStore();
