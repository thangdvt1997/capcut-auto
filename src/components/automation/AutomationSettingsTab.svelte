<!--
  Phase D5 (`STUDIO_PLAN.md`): Tab 2 ("Automation & AI Settings") real
  content — replaces the honest `EmptyState` placeholder Phase D2+D3 left in
  `App.svelte`'s `automation` branch.

  ## Scope decision, stated explicitly (per this phase's own task brief)

  This tab is a **hub of real status summaries + real "Open…" actions**, not
  a reimplementation of each dialog's full form. Four settings surfaces
  already exist today as fully-working standalone dialogs, each with its own
  store: `AiSettingsDialog.svelte`/`aiSettingsStore`, `CapCutSettingsDialog
  .svelte`/`capcutStore`, `AutomationRulesDialog.svelte`/`automationStore`,
  `UpdateSettingsDialog.svelte`/`updateSettingsStore`. Re-deriving every field
  of each dialog's own form here (provider/base URL/temperature/timeout,
  detected-installation list + registry hints + manual override, the full
  Create Rule form, the three-mode radio group's install flow, etc.) would
  duplicate a large amount of dialog-internal logic for no real behavioral
  gain and real risk of the two copies drifting apart. Instead, each card
  below reads real `$state`/`$derived` fields straight off the same store the
  dialog itself uses (so there is exactly one source of truth), shows a
  handful of genuinely "trivially safe" quick actions that call an existing
  store method the dialog itself already calls unmodified (Test Connection,
  Re-scan, per-rule enable/disable toggle, Check for Updates Now, the update
  check-mode selector), and ends with a real "Open …" button that calls the
  exact same `xStore.openDialog()`/`openSettings()` method `TopBar.svelte`'s
  own button already calls — opening the identical, unmodified dialog for
  anything not covered here (editing the API key, the full CapCut detected-
  installation list, the Create Rule form, Install & Restart).

  ## Explicit, honest gap (not fabricated)

  `promt.md`'s own settings categories that have **no real dialog or backend
  store anywhere in this codebase** — Translation, Voice, Performance,
  Storage — are NOT represented here, not even as a disabled/inert toggle.
  `STUDIO_PLAN.md`'s own D5 phase note says exactly this needs its own
  gap-check before being scoped in; the bottom "Not built yet" panel states
  the gap in-app, in the same "say so plainly" spirit as `voice::stub`'s
  immediate, honest `NotImplemented` rather than a silent no-op or a fake
  toggle with nothing behind it.
-->
<script lang="ts">
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Button from "../ui/Button.svelte";
  import Badge from "../ui/Badge.svelte";
  import Select from "../ui/Select.svelte";
  import Switch from "../ui/Switch.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { aiSettingsStore, defaultModelFor } from "../../stores/aiSettings.svelte";
  import { capcutStore } from "../../stores/capcut.svelte";
  import { automationStore } from "../../stores/automation.svelte";
  import { updateSettingsStore, UPDATE_CHECK_MODES } from "../../stores/updateSettings.svelte";
  import type { AiProviderKind, AutomationRule, UpdateCheckMode } from "../../types/bindings";

  // Real data load for the two stores whose status this tab shows before
  // their own dialog has ever been opened this session — mirrors each
  // store's own "ensure loaded once" guard (`detectedOnce`/`loaded`), so
  // calling this unconditionally on every mount is safe and never
  // re-fetches once real data is already in hand.
  $effect(() => {
    void capcutStore.ensureDetected();
    void automationStore.ensureLoaded();
  });

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

  function updateModeLabel(mode: UpdateCheckMode): string {
    switch (mode) {
      case "automatically_check":
        return t("updateSettings.modeAutomatic");
      case "notify_only":
        return t("updateSettings.modeNotifyOnly");
      case "disabled":
        return t("updateSettings.modeDisabled");
    }
  }

  function updateStatusLine(): string {
    const outcome = updateSettingsStore.lastOutcome;
    if (!outcome) return t("updateSettings.statusIdle");
    switch (outcome.status) {
      case "disabled":
        return t("updateSettings.statusDisabled");
      case "up_to_date":
        return t("updateSettings.statusUpToDate");
      case "available":
        return t("updateSettings.statusAvailable", { version: outcome.version });
      case "deferred":
        return t("updateSettings.statusDeferred", { version: outcome.version });
      case "check_failed":
        return t("updateSettings.statusCheckFailed", { message: outcome.message });
      case "installing":
        return t("updateSettings.statusInstalling");
    }
  }

  /** Composes two already-real store calls (open the dialog, then arm its
   * Create Rule form) into one "New Rule…" quick action — no new store
   * logic, just the same two calls a user would otherwise make as two
   * separate clicks (open dialog, then click "New Rule…" inside it). */
  function quickNewRule(): void {
    automationStore.openDialog();
    automationStore.openCreateForm();
  }

  const enabledRulesCount = $derived(automationStore.rules.filter((r) => r.enabled).length);
  const updateModeOptions = $derived(
    UPDATE_CHECK_MODES.map((mode) => ({ value: mode, label: updateModeLabel(mode) })),
  );
</script>

{#snippet ruleRow(rule: AutomationRule)}
  <li class="ast-rule-item">
    <span class="ast-rule-name ast-truncate" title={rule.name}>{rule.name}</span>
    {#if automationStore.toggleErrorById[rule.id]}
      <span class="ast-status-line ast-status-fail ast-rule-toggle-error">
        {automationStore.toggleErrorById[rule.id]}
      </span>
    {/if}
    <Switch
      checked={rule.enabled}
      disabled={automationStore.togglingById[rule.id] ?? false}
      ariaLabel={rule.name}
      onchange={(checked) => void automationStore.setEnabled(rule, checked)}
    />
  </li>
{/snippet}

<div class="automation-settings-tab">
  <div class="ast-scroll">
    <p class="ast-explainer muted-2">{t("automationSettingsTab.explainer")}</p>

    <div class="ast-grid">
      <Panel title={t("automationSettingsTab.ai.title")}>
        <Card>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.ai.providerRowLabel")}</span>
            <span class="ast-row-value">{providerLabel(aiSettingsStore.provider)}</span>
          </div>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.ai.modelRowLabel")}</span>
            <span class="ast-row-value mono ast-truncate">
              {aiSettingsStore.model || defaultModelFor(aiSettingsStore.provider) || "—"}
            </span>
          </div>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.ai.keyRowLabel")}</span>
            <Badge variant={aiSettingsStore.hasKeyConfigured ? "pos" : "neutral"}>
              {aiSettingsStore.hasKeyConfigured ? t("aiSettings.keyConfigured") : t("aiSettings.keyNotConfigured")}
            </Badge>
          </div>
          {#if aiSettingsStore.testResult}
            <p
              class="ast-status-line"
              class:ast-status-ok={aiSettingsStore.testResult.success}
              class:ast-status-fail={!aiSettingsStore.testResult.success}
            >
              {aiSettingsStore.testResult.message}
            </p>
          {/if}
          <div class="ast-actions">
            <Button size="sm" disabled={aiSettingsStore.testing} onclick={() => void aiSettingsStore.testConnection()}>
              {aiSettingsStore.testing ? t("aiSettings.testing") : t("aiSettings.testButton")}
            </Button>
            <Button size="sm" variant="primary" onclick={() => aiSettingsStore.openDialog()}>
              {t("automationSettingsTab.ai.openButton")}
            </Button>
          </div>
        </Card>
      </Panel>

      <Panel title={t("automationSettingsTab.capcut.title")}>
        <Card>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.capcut.detectedRowLabel")}</span>
            <span class="ast-row-value">{capcutStore.installations.length}</span>
          </div>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.capcut.effectivePathRowLabel")}</span>
            {#if capcutStore.effectiveDraftRoot}
              <span class="ast-row-value mono ast-truncate" title={capcutStore.effectiveDraftRoot}>
                {capcutStore.effectiveDraftRoot}
              </span>
            {:else}
              <span class="ast-row-value muted-2">{t("capcutSettings.effectivePathNone")}</span>
            {/if}
          </div>
          {#if capcutStore.detectError}
            <p class="ast-status-line ast-status-fail">
              {t("capcutSettings.detectFailed", { error: capcutStore.detectError })}
            </p>
          {/if}
          <div class="ast-actions">
            <Button size="sm" disabled={capcutStore.detectLoading} onclick={() => void capcutStore.rescan()}>
              {capcutStore.detectLoading ? t("capcutSettings.detecting") : t("capcutSettings.rescanButton")}
            </Button>
            <Button size="sm" variant="primary" onclick={() => capcutStore.openSettings()}>
              {t("automationSettingsTab.capcut.openButton")}
            </Button>
          </div>
        </Card>
      </Panel>

      <Panel title={t("automationSettingsTab.automation.title")}>
        <Card>
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.automation.summaryLabel")}</span>
            <span class="ast-row-value">
              {t("automationSettingsTab.automation.summary", {
                enabled: enabledRulesCount,
                total: automationStore.rules.length,
              })}
            </span>
          </div>
          {#if automationStore.loadError}
            <p class="ast-status-line ast-status-fail">
              {t("automationRules.loadFailed", { error: automationStore.loadError })}
            </p>
          {/if}
          {#if automationStore.loading && automationStore.rules.length === 0}
            <p class="muted-2 ast-empty">{t("automationRules.loading")}</p>
          {:else if automationStore.rules.length === 0}
            <p class="muted-2 ast-empty">{t("automationRules.noRules")}</p>
          {:else}
            <ul class="ast-rule-list">
              {#each automationStore.rules as rule (rule.id)}
                {@render ruleRow(rule)}
              {/each}
            </ul>
          {/if}
          <div class="ast-actions">
            <Button size="sm" onclick={quickNewRule}>{t("automationRules.newRuleButton")}</Button>
            <Button size="sm" variant="primary" onclick={() => automationStore.openDialog()}>
              {t("automationSettingsTab.automation.openButton")}
            </Button>
          </div>
        </Card>
      </Panel>

      <Panel title={t("automationSettingsTab.update.title")}>
        <Card>
          <Select
            label={t("updateSettings.modeSectionTitle")}
            value={updateSettingsStore.mode}
            options={updateModeOptions}
            onchange={(v) => updateSettingsStore.setMode(v as UpdateCheckMode)}
          />
          <div class="ast-row">
            <span class="ast-row-label muted-2">{t("automationSettingsTab.update.statusRowLabel")}</span>
            <span
              class="ast-row-value"
              class:ast-status-ok={updateSettingsStore.lastOutcome?.status === "available"}
            >
              {updateStatusLine()}
            </span>
          </div>
          {#if updateSettingsStore.lastError}
            <p class="ast-status-line ast-status-fail">{updateSettingsStore.lastError}</p>
          {/if}
          <div class="ast-actions">
            <Button
              size="sm"
              disabled={updateSettingsStore.checking || updateSettingsStore.mode === "disabled"}
              onclick={() => void updateSettingsStore.checkNow()}
            >
              {updateSettingsStore.checking ? t("updateSettings.checking") : t("updateSettings.checkButton")}
            </Button>
            <Button size="sm" variant="primary" onclick={() => updateSettingsStore.openDialog()}>
              {t("automationSettingsTab.update.openButton")}
            </Button>
          </div>
        </Card>
      </Panel>
    </div>

    <Panel title={t("automationSettingsTab.gaps.title")}>
      <p class="muted-2 ast-gaps-desc">{t("automationSettingsTab.gaps.desc")}</p>
    </Panel>
  </div>
</div>

<style>
  .automation-settings-tab {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .ast-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-1) var(--space-6);
  }
  .ast-explainer {
    margin: 0;
    max-width: 720px;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .ast-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--space-4);
    align-items: start;
  }
  .ast-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .ast-row-label {
    font-size: 11px;
    flex-shrink: 0;
    min-width: 120px;
  }
  .ast-row-value {
    font-size: 11.5px;
    min-width: 0;
  }
  .ast-truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ast-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }
  .ast-status-line {
    margin: 0;
    font-size: 11px;
  }
  .ast-status-ok {
    color: var(--pos, #3fb950);
  }
  .ast-status-fail {
    color: var(--neg);
  }
  .ast-empty {
    margin: 0;
    font-size: 11px;
  }
  .ast-rule-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-height: 160px;
    overflow-y: auto;
  }
  .ast-rule-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .ast-rule-name {
    font-size: 11.5px;
    flex: 1;
    min-width: 0;
  }
  .ast-rule-toggle-error {
    flex-shrink: 0;
  }
  .ast-gaps-desc {
    margin: 0;
    max-width: 720px;
    font-size: 11px;
    line-height: 1.5;
  }
</style>
