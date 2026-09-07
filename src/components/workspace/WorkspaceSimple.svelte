<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace")'s "Simple" mode —
  promt.md §3/§28's own mockup shape (video preview | script/subtitle
  editor, a pipeline stepper below both, a Job Queue below that). This is
  the real, non-placeholder Tab 1 content the task asked for; see
  `WorkspaceTab.svelte`'s own doc comment for why this coexists with the
  pre-existing full editor (`WorkspaceAdvanced.svelte`) via a mode toggle
  rather than replacing it.

  Owns the one piece of state `ScriptEditor.svelte` and `PipelineStepper
  .svelte` both need — the real (never mutates `project.captions` itself,
  per `ai::translate::translate_captions`'s own doc comment) translation
  proposal this pass wires in as the "Translate" column/step, backed by the
  real, already-existing `translate_captions` command + `aiSettingsStore`
  (no new backend surface, frontend-only). Lifted here (not owned by either
  child alone) since the stepper needs to reflect the same translate-in-
  flight/success/error state the editor's own Translate button drives.
-->
<script lang="ts">
  import ResizableSplit from "../layout/ResizableSplit.svelte";
  import CenterPreview from "../layout/CenterPreview.svelte";
  import ScriptEditor from "./ScriptEditor.svelte";
  import PipelineStepper from "./PipelineStepper.svelte";
  import JobQueuePanel from "./JobQueuePanel.svelte";
  import { captionsStore } from "../../stores/captions.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { commands } from "../../types/bindings";

  let targetLanguage = $state("vi");
  let translating = $state(false);
  let translateError = $state<string | null>(null);
  let translations = $state<Record<string, string>>({});

  async function translateAll(): Promise<void> {
    if (translating || captionsStore.captions.length === 0) return;
    translating = true;
    translateError = null;
    try {
      const result = await commands.translateCaptions(
        captionsStore.captions,
        null,
        targetLanguage.trim() || "vi",
        null,
        aiSettingsStore.settingsSnapshot(),
      );
      if (result.status === "ok") {
        const map: Record<string, string> = {};
        for (const tc of result.data) map[tc.caption_id] = tc.translated_text;
        translations = map;
      } else {
        translateError = result.error.message;
      }
    } catch (err) {
      translateError = String(err);
    } finally {
      translating = false;
    }
  }
</script>

<div class="simple">
  <div class="top-split">
    <ResizableSplit
      direction="horizontal"
      initial={0.45}
      min={0.25}
      max={0.7}
      storageKey="ave:split:workspace-simple"
    >
      {#snippet a()}
        <CenterPreview />
      {/snippet}
      {#snippet b()}
        <ScriptEditor
          bind:targetLanguage
          {translating}
          {translateError}
          {translations}
          onTranslate={translateAll}
        />
      {/snippet}
    </ResizableSplit>
  </div>

  <PipelineStepper {translating} {translateError} hasTranslated={Object.keys(translations).length > 0} />

  <JobQueuePanel />
</div>

<style>
  .simple {
    height: 100%;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto auto;
    gap: var(--space-3);
    padding: var(--space-3);
  }
  .top-split {
    min-height: 0;
    height: 100%;
  }
</style>
