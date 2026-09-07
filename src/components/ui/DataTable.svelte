<!--
  Design System foundation (Phase D1, promt.md §19): a real, first, honestly-
  scoped DataTable — sortable columns + a row-slot/render-prop for custom
  cell content (status badges, action buttons), matching the plain
  `<table>` markup BatchJobsDialog.svelte's own Jobs table already uses
  (`.bj-table`/`thead`/`tbody`, one `<tr>` per row) but as a real reusable
  component instead of each table hand-rolling its own header/sort/empty
  logic.

  Honest scope limits (deliberately not attempted this pass, per the task
  brief's "a real first version is fine, don't cover every possible table
  need"): no pagination, no column resizing, no multi-column sort, no row
  selection/checkboxes. Sorting is client-side only (the whole `rows` array
  is sorted in memory) — fine for every existing table-shaped view in this
  app (Batch Jobs, History, Asset Library — all bounded, in-memory lists),
  not designed for a server-paginated dataset.

  Svelte 5 generic component (`generics="T"`) so callers get real per-column
  typing (`DataTableColumn<Job>`, etc.) rather than `any`.
-->
<script module lang="ts">
  import type { Snippet } from "svelte";

  export type DataTableColumn<T> = {
    key: string;
    label: string;
    sortable?: boolean;
    align?: "left" | "right" | "center";
    /** Sort key extractor. Required for `sortable` columns — a column
     * marked `sortable` with no `accessor` simply never reorders (documented
     * behavior, not a silent bug: `toggleSort` only flips `sortKey`/`sortDir`,
     * and `sortedRows` falls back to the unsorted `rows` when the resolved
     * column has no `accessor`). */
    accessor?: (row: T) => string | number;
    /** Custom cell content. Falls back to `accessor(row)` rendered as plain
     * text when omitted. */
    cell?: Snippet<[T]>;
  };
</script>

<script lang="ts" generics="T">
  /* eslint-disable no-undef -- `T` is the real Svelte 5 `generics="T"` type
     parameter above (this codebase's installed svelte-eslint-parser@0.43.0
     doesn't thread it into ESLint's own scope analysis for core `no-undef`,
     even though svelte-check/tsc both resolve it correctly — confirmed via
     `pnpm run check` reporting 0 errors for this exact file). Scoped to
     just the four real uses below, not a blanket rule change. */
  let {
    columns,
    rows,
    rowKey,
    emptyMessage,
  }: {
    columns: DataTableColumn<T>[];
    rows: T[];
    rowKey: (row: T) => string;
    emptyMessage?: string;
  } = $props();
  /* eslint-enable no-undef */

  let sortKey = $state<string | null>(null);
  let sortDir = $state<"asc" | "desc">("asc");

  // eslint-disable-next-line no-undef -- see the disable comment above.
  function toggleSort(col: DataTableColumn<T>): void {
    if (!col.sortable) return;
    if (sortKey === col.key) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortKey = col.key;
      sortDir = "asc";
    }
  }

  let sortedRows = $derived.by(() => {
    if (!sortKey) return rows;
    const col = columns.find((c) => c.key === sortKey);
    const accessor = col?.accessor;
    if (!accessor) return rows;
    const dir = sortDir === "asc" ? 1 : -1;
    return [...rows].sort((a, b) => {
      const av = accessor(a);
      const bv = accessor(b);
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return 0;
    });
  });
</script>

<div class="ui-table-wrap">
  <table class="ui-table">
    <thead>
      <tr>
        {#each columns as col (col.key)}
          <th
            class:ui-table-sortable={col.sortable}
            class:ui-table-align-right={col.align === "right"}
            class:ui-table-align-center={col.align === "center"}
            onclick={() => toggleSort(col)}
          >
            {col.label}
            {#if col.sortable && sortKey === col.key}
              <span class="ui-table-sort-indicator">{sortDir === "asc" ? "▲" : "▼"}</span>
            {/if}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each sortedRows as row (rowKey(row))}
        <tr>
          {#each columns as col (col.key)}
            <td class:ui-table-align-right={col.align === "right"} class:ui-table-align-center={col.align === "center"}>
              {#if col.cell}
                {@render col.cell(row)}
              {:else if col.accessor}
                {col.accessor(row)}
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  {#if rows.length === 0 && emptyMessage}
    <p class="ui-table-empty muted-2">{emptyMessage}</p>
  {/if}
</div>

<style>
  .ui-table-wrap {
    overflow-x: auto;
    min-width: 0;
  }
  .ui-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }
  .ui-table th {
    text-align: left;
    padding: 6px var(--space-2);
    color: var(--muted);
    font-weight: 600;
    font-size: 10.5px;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    user-select: none;
  }
  .ui-table-sortable {
    cursor: pointer;
  }
  .ui-table-sortable:hover {
    color: var(--foreground);
  }
  .ui-table-sort-indicator {
    margin-left: 4px;
    font-size: 8px;
  }
  .ui-table td {
    padding: 6px var(--space-2);
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .ui-table tbody tr:hover {
    background: var(--surface-2);
  }
  .ui-table-align-right {
    text-align: right;
  }
  .ui-table-align-center {
    text-align: center;
  }
  .ui-table-empty {
    margin: 0;
    padding: var(--space-3);
    text-align: center;
    font-size: 11.5px;
  }
</style>
