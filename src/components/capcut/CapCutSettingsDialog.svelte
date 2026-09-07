<!--
  CapCut / Jianying Settings dialog (Phase 9, master prompt §30): shows every
  confirmed installation `detect_capcut_installations` found (product,
  user profile, draft directory, which marker confirmed it), plus
  `detect_capcut_registry_hints`'s best-effort registry results clearly
  labeled as supplementary/lower-confidence, and a manual-override text
  input + native directory picker for when nothing was auto-detected or the
  user keeps drafts somewhere else. Read-only with respect to the
  filesystem — this dialog only detects and lets the user choose a path; it
  never writes anything (the confirmation-before-overwrite requirement lives
  in `CapCutExportDialog.svelte`, the one place this app actually writes a
  draft).

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`): mirrors
  `ModelManagerDialog.svelte`'s own placement precedent exactly — no master
  prompt §46 Settings surface exists yet to host this as a section, so this
  is a standalone dialog, mounted once in `App.svelte`, reachable from a
  "CapCut…" toolbar button in `TopBar.svelte` placed right next to the
  existing "Models…" button.

  Pure UI over `stores/capcut.svelte.ts`.

  Phase D7b retrofit: the hand-rolled backdrop/dialog/header/footer shell,
  section headers, card rows, status pill, and buttons now come from the
  Design System (`Modal`/`Panel`/`Card`/`Badge`/`Button`/`Input`/
  `EmptyState`/`ErrorState`, Phase D1) — every store call, `disabled`
  condition, and conditional-render expression below is byte-for-byte the
  same logic the original hand-rolled markup used. What's left bespoke, and
  why, is documented in this file's own `<style>` block and in
  `STUDIO_PLAN.md`'s Phase D7b section.
-->
<script lang="ts">
  import { capcutStore } from "../../stores/capcut.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { CapCutRegistryHint, DetectedCapCutInstallation } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import Input from "../ui/Input.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";

  function productLabel(product: DetectedCapCutInstallation["product"] | CapCutRegistryHint["product"]): string {
    return product === "jianying" ? t("capcutSettings.productJianying") : t("capcutSettings.productCapCut");
  }

  let overrideDraft = $state(capcutStore.manualDraftRoot ?? "");

  $effect(() => {
    // Keep the text input in sync when the override changes from outside
    // this component (e.g. "Use this path" on a detected row, or "Clear
    // override") without fighting the user's own typing — only resync when
    // the store's value and the local draft have actually diverged.
    if (capcutStore.manualDraftRoot !== overrideDraft && document.activeElement?.id !== "cc-override-input") {
      overrideDraft = capcutStore.manualDraftRoot ?? "";
    }
  });

  function commitOverrideDraft(): void {
    capcutStore.setManualDraftRoot(overrideDraft);
  }

  function clearOverride(): void {
    overrideDraft = "";
    capcutStore.setManualDraftRoot(null);
  }
</script>

<Modal
  open={capcutStore.settingsOpen}
  title={t("capcutSettings.title")}
  width={640}
  onClose={() => capcutStore.closeSettings()}
>
  <p class="cs-explainer muted-2">{t("capcutSettings.explainer")}</p>

  <Panel title={t("capcutSettings.detectedSectionTitle")}>
    {#snippet actions()}
      <Button variant="ghost" size="sm" disabled={capcutStore.detectLoading} onclick={() => void capcutStore.rescan()}>
        {capcutStore.detectLoading ? t("capcutSettings.detecting") : t("capcutSettings.rescanButton")}
      </Button>
    {/snippet}

    {#if capcutStore.detectError}
      <ErrorState message={t("capcutSettings.detectFailed", { error: capcutStore.detectError })} />
    {/if}
    {#if capcutStore.openCapcutError}
      <ErrorState message={t("capcutSettings.openCapcutFailed", { error: capcutStore.openCapcutError })} />
    {/if}

    {#if capcutStore.installations.length === 0 && !capcutStore.detectLoading && !capcutStore.detectError}
      <EmptyState title={t("capcutSettings.noneDetected")} />
    {/if}

    <div class="cs-list">
      {#each capcutStore.installations as inst, i (inst.draft_root)}
        <Card>
          <div class="cs-card-main">
            <div class="cs-card-info">
              <span class="cs-name">{productLabel(inst.product)}</span>
              <span class="cs-meta muted-2">{t("capcutSettings.userProfileLabel")}: {inst.user_profile}</span>
              <span class="cs-path muted-2" title={inst.draft_root}>{inst.draft_root}</span>
              <span class="cs-marker">
                {inst.has_root_meta_info ? t("capcutSettings.markerRootMetaInfo") : t("capcutSettings.markerRecycleBin")}
              </span>
            </div>
            <div class="cs-card-actions">
              {#if capcutStore.manualDraftRoot === inst.draft_root || (!capcutStore.manualDraftRoot && i === 0)}
                <Badge variant="accent">{t("capcutSettings.inUseBadge")}</Badge>
              {:else}
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => {
                    overrideDraft = inst.draft_root;
                    capcutStore.setManualDraftRoot(inst.draft_root);
                  }}
                >
                  {t("capcutSettings.useThisPathButton")}
                </Button>
              {/if}
              <Button
                variant="ghost"
                size="sm"
                disabled={capcutStore.openingCapcutFor === inst.draft_root}
                onclick={() => void capcutStore.openCapcutApp(inst)}
                title={t("capcutSettings.openCapcutTooltip")}
              >
                {capcutStore.openingCapcutFor === inst.draft_root
                  ? t("capcutSettings.openingCapcut")
                  : t("capcutSettings.openCapcutButton")}
              </Button>
            </div>
          </div>
        </Card>
      {/each}
    </div>
  </Panel>

  <Panel title={t("capcutSettings.registrySectionTitle")}>
    <p class="cs-explainer muted-2">{t("capcutSettings.registryExplainer")}</p>
    {#if capcutStore.registryHints.length === 0 && !capcutStore.detectLoading}
      <EmptyState title={t("capcutSettings.noneInRegistry")} />
    {/if}
    <div class="cs-list">
      {#each capcutStore.registryHints as hint (hint.display_name)}
        <Card>
          <div class="cs-card-info">
            <span class="cs-name">{hint.display_name}</span>
            <span class="cs-meta muted-2">{productLabel(hint.product)}</span>
            <span class="cs-meta muted-2">
              {t("capcutSettings.versionLabel")}: {hint.display_version ?? t("capcutSettings.unknownValue")}
            </span>
            <span class="cs-meta muted-2">
              {t("capcutSettings.installLocationLabel")}: {hint.install_location ?? t("capcutSettings.unknownValue")}
            </span>
          </div>
        </Card>
      {/each}
    </div>
  </Panel>

  <Panel title={t("capcutSettings.overrideSectionTitle")}>
    <p class="cs-explainer muted-2">{t("capcutSettings.overrideExplainer")}</p>
    <div class="cs-row">
      <Input
        id="cc-override-input"
        placeholder={t("capcutSettings.overridePlaceholder")}
        bind:value={overrideDraft}
        onblur={commitOverrideDraft}
        onkeydown={(e) => {
          if (e.key === "Enter") commitOverrideDraft();
        }}
      />
      <Button size="sm" onclick={() => void capcutStore.browseManualDraftRoot()}>
        {t("capcutSettings.browseButton")}
      </Button>
      {#if capcutStore.manualDraftRoot}
        <Button variant="ghost" size="sm" onclick={clearOverride}>{t("capcutSettings.clearButton")}</Button>
      {/if}
    </div>
  </Panel>

  <Panel title={t("capcutSettings.effectivePathLabel")}>
    {#if capcutStore.effectiveDraftRoot}
      <Card padding="sm"><span class="mono cs-path-text">{capcutStore.effectiveDraftRoot}</span></Card>
    {:else}
      <EmptyState title={t("capcutSettings.effectivePathNone")} />
    {/if}
  </Panel>

  {#snippet footer()}
    <Button variant="ghost" onclick={() => capcutStore.closeSettings()}>{t("capcutSettings.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* Below: a card-list layout grid + the info/actions split inside each
     Card, plus small explainer/path text classes — none of these have a
     Design System equivalent yet (kept, matching BatchJobsDialog's own
     retrofit precedent of leaving only genuinely unmatched layout rules in
     place). Colors reuse existing shared tokens (var(--muted), var(--pos))
     rather than a per-dialog literal. */
  .cs-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .cs-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .cs-card-main {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
    min-width: 0;
  }
  .cs-card-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .cs-card-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }
  .cs-name {
    font-size: 12.5px;
    font-weight: 600;
  }
  .cs-meta {
    font-size: 10.5px;
  }
  .cs-path {
    font-size: 10.5px;
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }
  .cs-marker {
    font-size: 10.5px;
    color: var(--pos);
  }
  .cs-path-text {
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }
  /* Input has no Design System equivalent for a text field that must grow to
     fill a row alongside fixed-width buttons (`.ui-field` itself doesn't
     declare flex:1) — a small, scoped growth rule, not a new visual
     recipe. */
  .cs-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    min-width: 0;
  }
  .cs-row :global(.ui-field) {
    flex: 1;
    min-width: 0;
  }
</style>
