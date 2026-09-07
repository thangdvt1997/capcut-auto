<!--
  Tab 3 ("Project / Asset / License Management") real content (Phase D6,
  `STUDIO_PLAN.md`'s "UI Redesign Audit + Phase D0-D8 Plan" section, promt.md
  §2/§28). Replaces the honest `EmptyState` placeholder Phase D2+D3 left in
  `App.svelte`'s final `{:else}` branch.

  Deliberate scope, stated plainly (see this component's own STUDIO_PLAN.md
  Phase D6 writeup for the full reasoning): this tab is a real STATUS HUB, not
  a reimplementation of `AssetLibraryDialog.svelte`/`HistoryDialog.svelte`/
  `TemplateGeneratorDialog.svelte`'s own internals. Each card below reads real,
  live state from that dialog's own existing store and opens that exact same
  dialog via its own real, already-working opening method (`openDialog()` on
  each store — the identical call `TopBar.svelte`'s own buttons already make,
  confirmed by reading `TopBar.svelte` before writing this file). Inlining
  each dialog's full UI here instead was deliberately not attempted this pass
  — a well-organized hub of real status + real "open dialog" actions is the
  honestly-scoped deliverable, not a rushed duplicate of already-working,
  tested dialog logic.

  License Management: no license system, store, or backend command exists
  anywhere in this codebase (confirmed via grep before writing this file) and
  no concrete requirement for one exists in any spec read so far — the
  `EmptyState` sub-section below states that gap honestly rather than faking
  a license UI, matching this project's "state an honest gap, never fake it"
  discipline.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { assetsStore } from "../../stores/assets.svelte";
  import { historyStore } from "../../stores/history.svelte";
  import { templateGenerator } from "../../stores/templateGenerator.svelte";

  onMount(() => {
    // Real, lazy status data for the summary cards below — the same lazy-load
    // methods each dialog's own `openDialog()` already calls, just triggered
    // here so a real count is visible before the user ever opens a dialog.
    void assetsStore.ensureLoaded();
    if (historyStore.entries.length === 0 && !historyStore.loading) {
      void historyStore.refresh();
    }
  });
</script>

<div class="pat-page">
  <Panel title={t("appTabs.projectAssetLicense")}>
    <p class="pat-intro muted-2">{t("projectAssetTab.intro")}</p>
  </Panel>

  <div class="pat-grid">
    <Card>
      <div class="pat-card-body">
        <div class="pat-card-header">
          <h3 class="pat-card-title">{t("projectAssetTab.assetLibrary.title")}</h3>
          {#if !assetsStore.loading && !assetsStore.loadError}
            <Badge variant="accent">{t("projectAssetTab.assetLibrary.countLabel", { count: assetsStore.assets.length })}</Badge>
          {/if}
        </div>
        <p class="pat-card-desc muted-2">{t("projectAssetTab.assetLibrary.description")}</p>
        {#if assetsStore.loading}
          <p class="pat-status muted-2">{t("projectAssetTab.assetLibrary.loadingLabel")}</p>
        {:else if assetsStore.loadError}
          <p class="pat-status pat-status-error">
            {t("projectAssetTab.assetLibrary.loadFailedLabel", { error: assetsStore.loadError })}
          </p>
        {/if}
        <Button variant="primary" onclick={() => assetsStore.openDialog()}>
          {t("projectAssetTab.assetLibrary.openButton")}
        </Button>
      </div>
    </Card>

    <Card>
      <div class="pat-card-body">
        <div class="pat-card-header">
          <h3 class="pat-card-title">{t("projectAssetTab.history.title")}</h3>
          {#if !historyStore.loading && !historyStore.loadError}
            <Badge variant="accent">
              {t(historyStore.hasMore ? "projectAssetTab.history.countLabelMore" : "projectAssetTab.history.countLabel", {
                count: historyStore.entries.length,
              })}
            </Badge>
          {/if}
        </div>
        <p class="pat-card-desc muted-2">{t("projectAssetTab.history.description")}</p>
        {#if historyStore.loading}
          <p class="pat-status muted-2">{t("projectAssetTab.history.loadingLabel")}</p>
        {:else if historyStore.loadError}
          <p class="pat-status pat-status-error">
            {t("projectAssetTab.history.loadFailedLabel", { error: historyStore.loadError })}
          </p>
        {/if}
        <Button variant="primary" onclick={() => historyStore.openDialog()}>
          {t("projectAssetTab.history.openButton")}
        </Button>
      </div>
    </Card>

    <Card>
      <div class="pat-card-body">
        <div class="pat-card-header">
          <h3 class="pat-card-title">{t("projectAssetTab.templateGenerator.title")}</h3>
        </div>
        <p class="pat-card-desc muted-2">{t("projectAssetTab.templateGenerator.description")}</p>
        {#if templateGenerator.savedTemplateName}
          <p class="pat-status">
            {t("projectAssetTab.templateGenerator.statusSaved", { name: templateGenerator.savedTemplateName })}
          </p>
        {:else if templateGenerator.generatedTemplate}
          <p class="pat-status muted-2">
            {t("projectAssetTab.templateGenerator.statusGenerated", { name: templateGenerator.generatedTemplate.name })}
          </p>
        {:else}
          <p class="pat-status muted-2">{t("projectAssetTab.templateGenerator.statusNone")}</p>
        {/if}
        <Button variant="primary" onclick={() => templateGenerator.openDialog()}>
          {t("projectAssetTab.templateGenerator.openButton")}
        </Button>
      </div>
    </Card>
  </div>

  <Panel title={t("projectAssetTab.license.title")}>
    <EmptyState
      icon="⚖️"
      title={t("projectAssetTab.license.title")}
      description={t("projectAssetTab.license.description")}
    />
  </Panel>
</div>

<style>
  .pat-page {
    height: 100%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    padding: var(--space-2) var(--space-1) var(--space-6);
  }
  .pat-intro {
    margin: 0;
    max-width: 720px;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .pat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--space-4);
  }
  .pat-card-body {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .pat-card-header {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .pat-card-title {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
  }
  .pat-card-desc {
    margin: 0;
    font-size: 11px;
    line-height: 1.45;
  }
  .pat-status {
    margin: 0;
    font-size: 11px;
  }
  .pat-status-error {
    color: var(--neg);
  }
</style>
