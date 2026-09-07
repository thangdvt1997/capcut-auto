<!--
  Activity / Log panel (`promt.md` §15 "LOG / ACTIVITY PANEL",
  `STUDIO_PLAN.md` Phase D8b). A real, filterable INFO/WARNING/ERROR log of
  batch pipeline stage transitions — built entirely on `stores/activityLog.svelte.ts`,
  which is itself built on the one real, already-flowing event stream that
  actually matches §15's own worked example (the real `batch:progress` Tauri
  event's per-stage `BatchJob.stage` transitions) — see that store's own doc
  comment for the full "what real signal this is, and isn't, built on"
  write-up. This component only renders what that store already computed; it
  invents no data of its own.

  Placement decision: docked as a 4th row of the whole app shell
  (`App.svelte`'s `.shell` grid), below the tab strip and the active tab's
  own content — not inside `WorkspaceTab`/Tab 1 specifically. Reasoning:
  batch jobs (the one real source this panel has) can be started and watched
  from multiple places already (Tab 1's `JobQueuePanel`, the `BatchJobsDialog`,
  a Smart-Automation-triggered batch, a History "Re-run") — a log of their
  stage transitions is genuinely cross-cutting, not Tab-1-specific, so it
  stays visible (collapsed, per the spec's own "collapsible" ask) no matter
  which of the 3 top-level tabs is active, exactly like `TopBar`/the dialogs
  mounted in `App.svelte` already are.

  Collapsed by default (task brief + `promt.md` §15's own "collapsible", and
  its explicit "không spam popup cho mọi event" instruction — a log panel
  that pops open uninvited would be exactly the spam that line rules out).
  The collapsed header still honestly surfaces real warning/error counts (a
  small `Badge` each) so a real problem is never hidden behind a collapsed
  panel with no visible signal at all — genuinely important errors still also
  reach the user via the existing Toast/Dialog mechanisms (untouched by this
  pass), this panel is the secondary, on-demand detail view.
-->
<script lang="ts">
  import { activityLogStore, type ActivityLogLevel } from "../../stores/activityLog.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Panel from "../ui/Panel.svelte";
  import Badge from "../ui/Badge.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";

  const levelOptions: SelectOption[] = [
    { value: "all", label: t("activityLog.filterAll") },
    { value: "info", label: t("activityLog.filterInfo") },
    { value: "warning", label: t("activityLog.filterWarning") },
    { value: "error", label: t("activityLog.filterError") },
  ];

  const badgeVariant: Record<ActivityLogLevel, "accent" | "warn" | "neg"> = {
    info: "accent",
    warning: "warn",
    error: "neg",
  };

  const levelLabel: Record<ActivityLogLevel, string> = {
    info: t("activityLog.filterInfo"),
    warning: t("activityLog.filterWarning"),
    error: t("activityLog.filterError"),
  };

  function formatClock(ms: number): string {
    return new Date(ms).toLocaleTimeString();
  }
</script>

<div class="al-shell">
  <Panel title={t("activityLog.title")}>
    {#snippet actions()}
      {#if activityLogStore.counts.error > 0}
        <Badge variant="neg">{activityLogStore.counts.error}</Badge>
      {/if}
      {#if activityLogStore.counts.warning > 0}
        <Badge variant="warn">{activityLogStore.counts.warning}</Badge>
      {/if}
      {#if !activityLogStore.collapsed}
        <Select
          value={activityLogStore.levelFilter}
          options={levelOptions}
          onchange={(v) => activityLogStore.setLevelFilter(v as "all" | ActivityLogLevel)}
        />
        <Button
          variant="ghost"
          size="sm"
          disabled={activityLogStore.entries.length === 0}
          onclick={() => activityLogStore.clear()}
        >
          {t("activityLog.clear")}
        </Button>
      {/if}
      <IconButton
        ariaLabel={activityLogStore.collapsed ? t("activityLog.expand") : t("activityLog.collapse")}
        onclick={() => activityLogStore.toggleCollapsed()}
      >
        {activityLogStore.collapsed ? "▼" : "▲"}
      </IconButton>
    {/snippet}
    {#if !activityLogStore.collapsed}
      <div class="al-body">
        {#if activityLogStore.filteredEntries.length === 0}
          <EmptyState
            title={activityLogStore.entries.length === 0
              ? t("activityLog.empty")
              : t("activityLog.emptyFiltered")}
          />
        {:else}
          <ul class="al-list">
            {#each activityLogStore.filteredEntries as entry (entry.id)}
              <li class="al-entry">
                <span class="al-entry-time muted-2">{formatClock(entry.timestampMs)}</span>
                <Badge variant={badgeVariant[entry.level]}>{levelLabel[entry.level]}</Badge>
                <span class="al-entry-message">{entry.message}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
  </Panel>
</div>

<style>
  .al-shell {
    border-top: 1px solid var(--border);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
  }
  .al-body {
    max-height: 220px;
    overflow-y: auto;
  }
  .al-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .al-entry {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    padding: 2px 0;
    font-size: 11.5px;
    border-bottom: 1px solid var(--border);
  }
  .al-entry:last-child {
    border-bottom: none;
  }
  .al-entry-time {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .al-entry-message {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
