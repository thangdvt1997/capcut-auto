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

  Phase D7b retrofit: shell/sections/buttons now come from the Design
  System (`Modal`/`Panel`/`Button`/`ErrorState`, Phase D1). Every store call
  (`setMode`/`checkNow`/`installNow`) and every `disabled`/conditional-
  render expression is unchanged. The status line stays a plain paragraph
  (its text is a full sentence, not a short label, so `Badge` — built for
  short pills — isn't a fit) with its color reusing the shared `--pos`/
  `--warn` tokens instead of a hex-fallback literal. See `STUDIO_PLAN.md`'s
  Phase D7b section.

  **Phase D9 gap-fill retrofit (`STUDIO_PLAN.md`):** the three check-mode
  options now use the new `RadioGroup.svelte` (`orientation="vertical"`,
  matching this dialog's own original one-row-per-line layout) — same
  `name`/`checked`/`onchange` semantics as the hand-rolled rows it replaces,
  via `updateSettingsStore.setMode`. The status line stays bespoke — it's
  multi-state (idle/disabled/up-to-date/available/deferred/check-failed/
  installing), not a single positive outcome, so the new `SuccessBanner`
  isn't a fit for it either, for the same "not just a short pill" reason
  `Badge` wasn't.
-->
<script lang="ts">
  import { updateSettingsStore, UPDATE_CHECK_MODES } from "../../stores/updateSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { UpdateCheckMode } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import RadioGroup from "../ui/RadioGroup.svelte";

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
</script>

<Modal
  open={updateSettingsStore.open}
  title={t("updateSettings.title")}
  width={520}
  onClose={() => updateSettingsStore.close()}
>
  <p class="us-explainer muted-2">{t("updateSettings.explainer")}</p>

  <Panel title={t("updateSettings.modeSectionTitle")}>
    <RadioGroup
      name="update-check-mode"
      orientation="vertical"
      value={updateSettingsStore.mode}
      options={UPDATE_CHECK_MODES.map((mode) => ({ value: mode, label: modeLabel(mode) }))}
      onchange={(v) => updateSettingsStore.setMode(v as UpdateCheckMode)}
    />
  </Panel>

  <Panel title={t("updateSettings.checkSectionTitle")}>
    <div class="us-row">
      <Button disabled={updateSettingsStore.checking || updateSettingsStore.mode === "disabled"} onclick={() => void updateSettingsStore.checkNow()}>
        {updateSettingsStore.checking ? t("updateSettings.checking") : t("updateSettings.checkButton")}
      </Button>
      {#if updateSettingsStore.lastOutcome?.status === "available"}
        <Button variant="ghost" disabled={updateSettingsStore.installing} onclick={() => void updateSettingsStore.installNow()}>
          {updateSettingsStore.installing ? t("updateSettings.installing") : t("updateSettings.installButton")}
        </Button>
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
      <ErrorState message={updateSettingsStore.lastError} />
    {/if}
  </Panel>

  {#snippet footer()}
    <Button variant="ghost" onclick={() => updateSettingsStore.close()}>{t("updateSettings.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* No Design System paragraph-typography primitive exists for a small
     explainer sentence (Panel/EmptyState don't include one) — kept as a
     tiny local class, unchanged in size/spacing from the original. */
  .us-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .us-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .us-status {
    margin: 0;
    font-size: 11.5px;
    color: var(--muted);
  }
  .us-status-available {
    color: var(--pos);
    font-weight: 600;
  }
  .us-status-deferred {
    color: var(--warn);
    font-weight: 600;
  }
</style>
