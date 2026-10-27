<script lang="ts" module>
  export interface Option {
    value: string;
    label: string;
    count?: number;
  }
</script>

<script lang="ts">
  import { formatCount } from "./format";

  // A dropdown of checkboxes; an empty selection means "any".
  let {
    placeholder,
    noun,
    options,
    selected = $bindable([]),
    searchable = false,
  }: {
    /** Button text when nothing is selected, e.g. "All types". */
    placeholder: string;
    /** Plural used in the summary, e.g. "cameras" → "3 cameras". */
    noun: string;
    options: Option[];
    selected?: string[];
    searchable?: boolean;
  } = $props();

  let open = $state(false);
  let search = $state("");
  let root = $state<HTMLDivElement>();
  let searchInput = $state<HTMLInputElement>();

  // Keep chosen values visible even if the current filters no longer list them.
  const allOptions = $derived([
    ...selected.filter((v) => !options.some((o) => o.value === v)).map((v) => ({ value: v, label: v, count: 0 })),
    ...options,
  ]);
  const shown = $derived(
    search ? allOptions.filter((o) => o.label.toLowerCase().includes(search.toLowerCase())) : allOptions,
  );
  const labelOf = (value: string) => allOptions.find((o) => o.value === value)?.label ?? value;
  const summary = $derived(
    selected.length === 0
      ? placeholder
      : selected.length === 1
        ? labelOf(selected[0])
        : selected.length === 2 && (labelOf(selected[0]) + labelOf(selected[1])).length < 22
          ? `${labelOf(selected[0])}, ${labelOf(selected[1])}`
          : `${selected.length} ${noun}`,
  );

  function toggle(value: string) {
    selected = selected.includes(value) ? selected.filter((v) => v !== value) : [...selected, value];
  }

  function onWindowPointer(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }

  $effect(() => {
    if (open) {
      search = "";
      searchInput?.focus();
    }
  });
</script>

<svelte:window onpointerdown={onWindowPointer} />

<div class="multi" bind:this={root}>
  <button
    class="trigger"
    class:active={selected.length > 0}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => (open = !open)}
    onkeydown={(e) => e.key === "Escape" && (open = false)}
  >
    <span class="summary">{summary}</span>
    <span class="arrow">▾</span>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="popover" onkeydown={(e) => e.key === "Escape" && (open = false)}>
      {#if searchable}
        <input class="search" type="search" placeholder="Search…" bind:value={search} bind:this={searchInput} />
      {/if}
      <div class="options" role="listbox" aria-multiselectable="true">
        {#each shown as o (o.value)}
          <label class="option">
            <input type="checkbox" checked={selected.includes(o.value)} onchange={() => toggle(o.value)} />
            <span class="label">{o.label}</span>
            {#if o.count !== undefined}<span class="count">{formatCount(o.count)}</span>{/if}
          </label>
        {:else}
          <div class="empty muted">Nothing found</div>
        {/each}
      </div>
      {#if selected.length}
        <div class="footer">
          <button onclick={() => (selected = [])}>Clear</button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .multi {
    position: relative;
  }
  .trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 240px;
  }
  .trigger.active {
    border-color: var(--accent);
  }
  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arrow {
    color: var(--muted);
    font-size: 11px;
  }
  .popover {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    min-width: 220px;
    max-width: 320px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.25);
    padding: 6px;
  }
  .search {
    width: 100%;
    margin-bottom: 6px;
  }
  .options {
    max-height: 320px;
    overflow-y: auto;
  }
  .option {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 4px;
    cursor: pointer;
  }
  .option:hover {
    background: var(--row-hover);
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    padding: 6px;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    border-top: 1px solid var(--border);
    margin-top: 6px;
    padding-top: 6px;
  }
  .footer button {
    font-size: 12px;
    padding: 2px 8px;
  }
</style>
