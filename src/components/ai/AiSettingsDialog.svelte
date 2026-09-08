<!--
  AI Settings dialog (Phase 10, master prompt §17): provider picker (the 5
  `AiProviderKind` variants), Base URL, API Key, Model, Temperature, Timeout,
  and a real "Test Connection" button. Read `src-tauri/src/commands/ai.rs`'s
  own module doc comment before touching this file: there is deliberately no
  backend persistence command for the non-secret settings (they live in
  `localStorage` via `stores/aiSettings.svelte.ts`) and no `get_ai_api_key`
  command anywhere — once a key is saved, this dialog can only ever show
  "configured ✓" / "not configured", never the key itself.

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`): mirrors
  `ModelManagerDialog.svelte`/`CapCutSettingsDialog.svelte`'s own placement
  precedent exactly — no master prompt §46 Settings surface exists yet to
  host this as a section, so this is a standalone dialog, mounted once in
  `App.svelte`, reachable from an "AI Settings…" button in `TopBar.svelte`
  right next to the existing "Models…"/"CapCut…" buttons.

  Pure UI over `stores/aiSettings.svelte.ts`.

  Phase D7b retrofit: shell/sections/buttons/provider picker/base URL/model
  fields now come from the Design System (`Modal`/`Panel`/`Select`/`Input`/
  `Slider`/`Badge`/`Button`/`ErrorState`, Phase D1). Every store call
  (`setProvider`/`setBaseUrl`/`setModel`/`setTemperature`/`saveApiKey`/
  `deleteApiKey`/`testConnection`) and every `disabled`/conditional-render
  expression is unchanged. The API key field deliberately stays hand-rolled
  — `Input.svelte` has no `autocomplete` passthrough, and dropping
  `autocomplete="off"` here would be a real behavior change, not just chrome
  — a browser could start offering to save/autofill the secret. See this
  file's own `<style>` block and `STUDIO_PLAN.md`'s Phase D7b section for
  details.

  **Phase D9 gap-fill retrofit (`STUDIO_PLAN.md`):** the Timeout field now
  uses the new `NumberInput.svelte` (the real "no numeric Input variant" gap
  Phase D7b documented — `Slider.svelte` was never a fit here since Timeout
  has no upper bound the way Temperature does), and the test-connection
  success message now uses the new `SuccessBanner.svelte`. Same
  `setTimeoutMs`/clamping logic, same `testResult.message` text — only the
  markup changed.
-->
<script lang="ts">
  import {
    aiSettingsStore,
    AI_PROVIDER_KINDS,
    defaultBaseUrlFor,
    defaultModelFor,
  } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiProviderKind } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Select from "../ui/Select.svelte";
  import Input from "../ui/Input.svelte";
  import Slider from "../ui/Slider.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import NumberInput from "../ui/NumberInput.svelte";
  import SuccessBanner from "../ui/SuccessBanner.svelte";
  import type { SelectOption } from "../ui/Select.svelte";

  function providerLabel(kind: AiProviderKind): string {
    switch (kind) {
      case "open_ai":
        return t("aiSettings.providerOpenAi");
      case "ollama":
        return t("aiSettings.providerOllama");
      case "custom_open_ai_compatible":
        return t("aiSettings.providerCustom");
      case "anthropic":
        return t("aiSettings.providerAnthropic");
      case "gemini":
        return t("aiSettings.providerGemini");
    }
  }

  function requirementLabel(): string {
    switch (aiSettingsStore.keyRequirement) {
      case "required":
        return t("aiSettings.keyRequired");
      case "recommended":
        return t("aiSettings.keyRecommended");
      case "optional":
        return t("aiSettings.keyOptional");
    }
  }

  const providerOptions: SelectOption[] = AI_PROVIDER_KINDS.map((kind) => ({
    value: kind,
    label: providerLabel(kind),
  }));
</script>

<Modal
  open={aiSettingsStore.open}
  title={t("aiSettings.title")}
  width={640}
  onClose={() => aiSettingsStore.close()}
>
  <p class="as-explainer muted-2">{t("aiSettings.explainer")}</p>

  <Panel title={t("aiSettings.providerSectionTitle")}>
    <Select
      id="as-provider"
      label={t("aiSettings.providerLabel")}
      value={aiSettingsStore.provider}
      options={providerOptions}
      onchange={(v) => aiSettingsStore.setProvider(v as AiProviderKind)}
    />
    <Input
      id="as-base-url"
      label={t("aiSettings.baseUrlLabel")}
      placeholder={defaultBaseUrlFor(aiSettingsStore.provider) || t("aiSettings.baseUrlPlaceholder")}
      value={aiSettingsStore.baseUrl}
      onblur={(e) => aiSettingsStore.setBaseUrl((e.target as HTMLInputElement).value)}
    />
    <Input
      id="as-model"
      label={t("aiSettings.modelLabel")}
      placeholder={defaultModelFor(aiSettingsStore.provider) || t("aiSettings.modelPlaceholder")}
      value={aiSettingsStore.model}
      onblur={(e) => aiSettingsStore.setModel((e.target as HTMLInputElement).value)}
    />
  </Panel>

  <Panel title={t("aiSettings.paramsSectionTitle")}>
    <Slider
      id="as-temperature"
      label={t("aiSettings.temperatureLabel")}
      min={0}
      max={2}
      step={0.05}
      value={aiSettingsStore.temperature}
      formatValue={(v) => v.toFixed(2)}
      onchange={(v) => aiSettingsStore.setTemperature(v)}
    />
    <div class="as-row">
      <label class="as-label" for="as-timeout">{t("aiSettings.timeoutLabel")}</label>
      <div class="as-timeout-wrap">
        <NumberInput
          id="as-timeout"
          min={1000}
          step={1000}
          value={aiSettingsStore.timeoutMs}
          onchange={(v) => aiSettingsStore.setTimeoutMs(Math.max(1000, v || 1000))}
        />
      </div>
      <span class="as-hint muted-2">ms</span>
    </div>
  </Panel>

  <Panel title={t("aiSettings.credentialsSectionTitle")}>
    <p class="as-hint muted-2">{requirementLabel()}</p>
    <div class="as-row">
      <Badge variant={aiSettingsStore.hasKeyConfigured ? "pos" : "neutral"}>
        {aiSettingsStore.hasKeyConfigured ? t("aiSettings.keyConfigured") : t("aiSettings.keyNotConfigured")}
      </Badge>
    </div>
    <div class="as-row">
      <input
        class="ui-input"
        type="password"
        autocomplete="off"
        placeholder={t("aiSettings.keyInputPlaceholder")}
        bind:value={aiSettingsStore.apiKeyDraft}
      />
      <Button
        size="sm"
        disabled={aiSettingsStore.apiKeyDraft.trim() === "" || aiSettingsStore.savingKey}
        onclick={() => void aiSettingsStore.saveApiKey()}
      >
        {aiSettingsStore.savingKey ? t("aiSettings.savingKey") : t("aiSettings.saveKeyButton")}
      </Button>
      {#if aiSettingsStore.hasKeyConfigured}
        <Button variant="ghost" size="sm" disabled={aiSettingsStore.savingKey} onclick={() => void aiSettingsStore.deleteApiKey()}>
          {t("aiSettings.deleteKeyButton")}
        </Button>
      {/if}
    </div>
    <p class="as-hint muted-2">{t("aiSettings.keyNeverRedisplayedNote")}</p>
    {#if aiSettingsStore.keyActionError}
      <ErrorState message={aiSettingsStore.keyActionError} />
    {/if}
  </Panel>

  <Panel title={t("aiSettings.testSectionTitle")}>
    <div class="as-row">
      <Button disabled={aiSettingsStore.testing} onclick={() => void aiSettingsStore.testConnection()}>
        {aiSettingsStore.testing ? t("aiSettings.testing") : t("aiSettings.testButton")}
      </Button>
    </div>
    {#if aiSettingsStore.testResult}
      {#if aiSettingsStore.testResult.success}
        <SuccessBanner message={aiSettingsStore.testResult.message} />
      {:else}
        <ErrorState message={aiSettingsStore.testResult.message} />
      {/if}
    {/if}
  </Panel>

  {#snippet footer()}
    <Button variant="ghost" onclick={() => aiSettingsStore.close()}>{t("aiSettings.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* No Design System paragraph-typography primitive exists for a small
     explainer/hint line (Panel/EmptyState don't include one) — kept as a
     tiny local class, unchanged in size/spacing from the original. */
  .as-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .as-hint {
    margin: 0;
    font-size: 10.5px;
  }
  .as-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .as-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  /* Fixed-width wrapper around NumberInput (Phase D9) — NumberInput's own
     `.ui-input` fills its container (width: 100%), so this small,
     layout-only wrapper div reproduces the original `.as-input-narrow`'s
     110px field width without needing a size prop on the shared component
     itself. */
  .as-timeout-wrap {
    flex: none;
    width: 110px;
  }
</style>
