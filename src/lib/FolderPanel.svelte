<script lang="ts">
  import type { Drives } from "./drives.svelte";
  import { formatCount } from "./format";
  import FolderTree from "./FolderTree.svelte";

  let {
    drives,
    selected = $bindable(),
    folder = $bindable(),
    includeMissing,
  }: {
    drives: Drives;
    selected: number | null;
    /** Selected folder on the selected drive ('' = root), or null for the whole drive. */
    folder: string | null;
    includeMissing: boolean;
  } = $props();

  let expanded = $state<Record<number, boolean>>({});

  function pickFolder(volumeId: number, path: string) {
    selected = volumeId;
    folder = path;
  }
</script>

<aside aria-label="Folders">
  <h2>Folders</h2>
  {#each drives.catalogued as v (v.id)}
    <div class="drive-row" class:active={selected === v.id && folder === null}>
      <button
        class="chevron"
        aria-label={expanded[v.id!] ? `Hide folders of ${v.label}` : `Show folders of ${v.label}`}
        aria-expanded={!!expanded[v.id!]}
        onclick={() => (expanded[v.id!] = !expanded[v.id!])}>{expanded[v.id!] ? "▾" : "▸"}</button
      >
      <button class="drive" onclick={() => ((selected = v.id), (folder = null))}>
        <span class="name">{v.label}</span>
        <span class="count">{formatCount(v.fileCount)}</span>
      </button>
    </div>
    {#if expanded[v.id!]}
      <div class="tree">
        <FolderTree
          volumeId={v.id!}
          parent=""
          depth={0}
          refreshKey={drives.treeKey}
          {includeMissing}
          selectedPath={selected === v.id ? folder : null}
          onSelect={pickFolder}
        />
      </div>
    {/if}
  {/each}
</aside>

<style>
  aside {
    flex: none;
    width: 272px;
    padding: 4px 8px 16px 24px;
    overflow-y: auto;
  }
  h2 {
    margin: 4px 8px 10px;
    font-size: 15px;
    font-weight: 600;
  }
  .drive-row {
    display: flex;
    align-items: center;
    min-height: 30px;
    border-radius: 6px;
  }
  .drive-row:hover {
    background: var(--row-hover);
  }
  .drive-row.active {
    background: var(--row-selected);
  }
  button {
    border: none;
    background: none;
    padding: 0;
  }
  .chevron {
    flex: none;
    width: 22px;
    color: var(--muted);
  }
  .drive {
    display: flex;
    flex: 1;
    gap: 6px;
    min-width: 0;
    padding: 5px 8px 5px 0;
    text-align: left;
  }
  .name {
    flex: 1;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .tree {
    margin: 2px 0 8px 12px;
  }
</style>
