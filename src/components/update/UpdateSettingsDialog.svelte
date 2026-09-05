<!--
  Update Settings dialog (Phase 12, master prompt §62): the three
  `UpdateCheckMode` radio options (Automatically check / Notify only /
  Disabled), a real "Check for Updates Now" button calling
  `commands.checkForUpdate`, a real status display, and a real
  "Install & Restart" action once an update is actually available.

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`): mirrors
  `AiSettingsDialog.svelte`/`CapCutSettingsDialog.svelte`'s own placement
  precedent exactly — no master prompt §46 Settings surface exists yet to
  host this as a section, so this is a standalone dialog, mounted once in
  `App.svelte`, reachable from an "Updates…" button in `TopBar.svelte` next
  to the other standalone-dialog buttons.

  "Never update mid-render" (the one piece of this feature with real logic)
  is enforced entirely on the backend (`commands::update::check_for_update`/
  `install_available_update`, consulting the real render/batch job
  registries) — this dialog only ever *displays* whatever real status the
  backend reports (including `"deferred"`), it never second-guesses it.

  Pure UI over `stores/updateSettings.svelte.ts`.
-->
<script lang="ts">
  import { updateSettingsStore, UPDATE_CHECK_MODES } from "../../stores/updateSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { UpdateCheckMode } from "../../types/bindings";

  function modeLabel(mode: UpdateCheckMode): string {
    switch (mode) {
      case "automatically_check":
        return t("updateSettings.modeAutomatic");
      case "notify_only":
        return t("updateSettings.modeNotifyOnly");
      case "disabled":
        return t("updateSettings.modeDisabled");
    }
  }

  function statusLine(): string {
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

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      updateSettingsStore.close();
    }
  }
</script>

{#if updateSettingsStore.open}
  <div class="us-backdrop" role="presentation" onclick={() => updateSettingsStore.close()}>
    <div
      class="us-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("updateSettings.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="us-header">
        <span class="us-title">{t("updateSettings.title")}</span>
        <button class="btn btn-ghost" onclick={() => updateSettingsStore.close()} title={t("updateSettings.close")}>×</button>
      </div>

      <div class="us-body">
        <p class="us-explainer muted-2">{t("updateSettings.explainer")}</p>

        <section class="us-section">
          <h3 class="us-section-title">{t("updateSettings.modeSectionTitle")}</h3>
          {#each UPDATE_CHECK_MODES as mode (mode)}
            <label class="us-radio-row">
              <input
                type="radio"
                name="update-check-mode"
                value={mode}
                checked={updateSettingsStore.mode === mode}
                onchange={() => updateSettingsStore.setMode(mode)}
              />
              <span>{modeLabel(mode)}</span>
            </label>
          {/each}
        </section>

        <section class="us-section">
          <h3 class="us-section-title">{t("updateSettings.checkSectionTitle")}</h3>
          <div class="us-row">
            <button
              class="btn"
              disabled={updateSettingsStore.checking || updateSettingsStore.mode === "disabled"}
              onclick={() => void updateSettingsStore.checkNow()}
            >
              {updateSettingsStore.checking ? t("updateSettings.checking") : t("updateSettings.checkButton")}
            </button>
            {#if updateSettingsStore.lastOutcome?.status === "available"}
              <button
                class="btn btn-ghost"
                disabled={updateSettingsStore.installing}
                onclick={() => void updateSettingsStore.installNow()}
              >
                {updateSettingsStore.installing ? t("updateSettings.installing") : t("updateSettings.installButton")}
              </button>
            {/if}
          </div>
          <p
            class="us-status"
            class:us-status-available={updateSettingsStore.lastOutcome?.status === "available"}
            class:us-status-deferred={updateSettingsStore.lastOutcome?.status === "deferred"}
          >
            {statusLine()}
          </p>
          {#if updateSettingsStore.lastError}
            <div class="us-error">{updateSettingsStore.lastError}</div>
          {/if}
        </section>
      </div>

      <div class="us-footer">
        <span class="us-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => updateSettingsStore.close()}>{t("updateSettings.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .us-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .us-dialog {
    width: min(520px, 94vw);
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
  .us-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .us-title {
    font-size: 13px;
    font-weight: 600;
  }
  .us-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .us-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .us-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .us-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .us-radio-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    cursor: pointer;
  }
  .us-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .us-status {
    margin: 0;
    font-size: 11.5px;
    color: var(--muted);
  }
  .us-status-available {
    color: var(--pos, #3fb950);
    font-weight: 600;
  }
  .us-status-deferred {
    color: var(--warn, #d29922);
    font-weight: 600;
  }
  .us-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .us-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .us-footer-spacer {
    flex: 1;
  }
</style>
