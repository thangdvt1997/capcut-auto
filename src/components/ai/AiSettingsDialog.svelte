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

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      aiSettingsStore.close();
    }
  }
</script>

{#if aiSettingsStore.open}
  <div class="as-backdrop" role="presentation" onclick={() => aiSettingsStore.close()}>
    <div
      class="as-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("aiSettings.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="as-header">
        <span class="as-title">{t("aiSettings.title")}</span>
        <button class="btn btn-ghost" onclick={() => aiSettingsStore.close()} title={t("aiSettings.close")}>×</button>
      </div>

      <div class="as-body">
        <p class="as-explainer muted-2">{t("aiSettings.explainer")}</p>

        <section class="as-section">
          <h3 class="as-section-title">{t("aiSettings.providerSectionTitle")}</h3>
          <div class="as-row">
            <label class="as-label" for="as-provider">{t("aiSettings.providerLabel")}</label>
            <select
              id="as-provider"
              class="as-select"
              value={aiSettingsStore.provider}
              onchange={(e) => aiSettingsStore.setProvider((e.target as HTMLSelectElement).value as AiProviderKind)}
            >
              {#each AI_PROVIDER_KINDS as kind (kind)}
                <option value={kind}>{providerLabel(kind)}</option>
              {/each}
            </select>
          </div>
          <div class="as-row">
            <label class="as-label" for="as-base-url">{t("aiSettings.baseUrlLabel")}</label>
            <input
              id="as-base-url"
              class="as-input"
              type="text"
              placeholder={defaultBaseUrlFor(aiSettingsStore.provider) || t("aiSettings.baseUrlPlaceholder")}
              value={aiSettingsStore.baseUrl}
              onchange={(e) => aiSettingsStore.setBaseUrl((e.target as HTMLInputElement).value)}
            />
          </div>
          <div class="as-row">
            <label class="as-label" for="as-model">{t("aiSettings.modelLabel")}</label>
            <input
              id="as-model"
              class="as-input"
              type="text"
              placeholder={defaultModelFor(aiSettingsStore.provider) || t("aiSettings.modelPlaceholder")}
              value={aiSettingsStore.model}
              onchange={(e) => aiSettingsStore.setModel((e.target as HTMLInputElement).value)}
            />
          </div>
        </section>

        <section class="as-section">
          <h3 class="as-section-title">{t("aiSettings.paramsSectionTitle")}</h3>
          <div class="as-slider-row">
            <label class="as-label" for="as-temperature">{t("aiSettings.temperatureLabel")}</label>
            <input
              id="as-temperature"
              type="range"
              min="0"
              max="2"
              step="0.05"
              value={aiSettingsStore.temperature}
              oninput={(e) => aiSettingsStore.setTemperature(Number((e.target as HTMLInputElement).value))}
            />
            <span class="as-value mono">{aiSettingsStore.temperature.toFixed(2)}</span>
          </div>
          <div class="as-row">
            <label class="as-label" for="as-timeout">{t("aiSettings.timeoutLabel")}</label>
            <input
              id="as-timeout"
              class="as-input as-input-narrow"
              type="number"
              min="1000"
              step="1000"
              value={aiSettingsStore.timeoutMs}
              onchange={(e) => aiSettingsStore.setTimeoutMs(Math.max(1000, Number((e.target as HTMLInputElement).value) || 1000))}
            />
            <span class="as-hint muted-2">ms</span>
          </div>
        </section>

        <section class="as-section">
          <h3 class="as-section-title">{t("aiSettings.credentialsSectionTitle")}</h3>
          <p class="as-hint muted-2">{requirementLabel()}</p>
          <div class="as-row">
            <span class="as-key-status" class:as-key-status-ok={aiSettingsStore.hasKeyConfigured}>
              {aiSettingsStore.hasKeyConfigured ? t("aiSettings.keyConfigured") : t("aiSettings.keyNotConfigured")}
            </span>
          </div>
          <div class="as-row">
            <input
              class="as-input"
              type="password"
              autocomplete="off"
              placeholder={t("aiSettings.keyInputPlaceholder")}
              bind:value={aiSettingsStore.apiKeyDraft}
            />
            <button
              class="btn btn-sm"
              disabled={aiSettingsStore.apiKeyDraft.trim() === "" || aiSettingsStore.savingKey}
              onclick={() => void aiSettingsStore.saveApiKey()}
            >
              {aiSettingsStore.savingKey ? t("aiSettings.savingKey") : t("aiSettings.saveKeyButton")}
            </button>
            {#if aiSettingsStore.hasKeyConfigured}
              <button
                class="btn btn-ghost btn-sm"
                disabled={aiSettingsStore.savingKey}
                onclick={() => void aiSettingsStore.deleteApiKey()}
              >
                {t("aiSettings.deleteKeyButton")}
              </button>
            {/if}
          </div>
          <p class="as-hint muted-2">{t("aiSettings.keyNeverRedisplayedNote")}</p>
          {#if aiSettingsStore.keyActionError}
            <div class="as-error">{aiSettingsStore.keyActionError}</div>
          {/if}
        </section>

        <section class="as-section">
          <h3 class="as-section-title">{t("aiSettings.testSectionTitle")}</h3>
          <div class="as-row">
            <button class="btn" disabled={aiSettingsStore.testing} onclick={() => void aiSettingsStore.testConnection()}>
              {aiSettingsStore.testing ? t("aiSettings.testing") : t("aiSettings.testButton")}
            </button>
          </div>
          {#if aiSettingsStore.testResult}
            <div class="as-test-result" class:as-test-ok={aiSettingsStore.testResult.success} class:as-test-fail={!aiSettingsStore.testResult.success}>
              {aiSettingsStore.testResult.message}
            </div>
          {/if}
        </section>
      </div>

      <div class="as-footer">
        <span class="as-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => aiSettingsStore.close()}>{t("aiSettings.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .as-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .as-dialog {
    width: min(640px, 94vw);
    max-height: 88vh;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .as-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .as-title {
    font-size: 13px;
    font-weight: 600;
  }
  .as-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .as-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .as-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .as-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .as-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .as-slider-row {
    display: grid;
    grid-template-columns: 130px 1fr 56px;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .as-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 100px;
  }
  .as-select {
    flex: 1;
    min-width: 0;
    height: 28px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .as-input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .as-input-narrow {
    flex: none;
    width: 110px;
  }
  .as-value {
    font-size: 11px;
    text-align: right;
    color: var(--muted);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .as-hint {
    margin: 0;
    font-size: 10.5px;
  }
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
  }
  .as-key-status {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--muted);
  }
  .as-key-status-ok {
    color: var(--pos, #3fb950);
  }
  .as-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .as-test-result {
    padding: 8px 10px;
    font-size: 11.5px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
  }
  .as-test-ok {
    color: var(--pos, #3fb950);
    background: hsl(140 60% 50% / 0.08);
    border-color: hsl(140 60% 50% / 0.3);
  }
  .as-test-fail {
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border-color: hsl(0 84% 65% / 0.3);
  }
  .as-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .as-footer-spacer {
    flex: 1;
  }
</style>
