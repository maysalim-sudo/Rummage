<script lang="ts">
  import { untrack } from "svelte";
  import { api, type Folder } from "./api";
  import { formatCount } from "./format";
  import FolderTree from "./FolderTree.svelte";

  // One level of a drive's folder tree; expanded folders render the next level lazily.
  let {
    volumeId,
    parent,
    depth,
    refreshKey,
    includeMissing,
    selectedPath,
    onSelect,
  }: {
    volumeId: number;
    parent: string;
    depth: number;
    refreshKey: number;
    includeMissing: boolean;
    /** The selected folder on this drive, or null. */
    selectedPath: string | null;
    onSelect: (volumeId: number, path: string) => void;
  } = $props();

  let folders = $state<Folder[] | null>(null);
  let expanded = $state<Record<string, boolean>>({});

  $effect(() => {
    const [v, p, missing] = [volumeId, parent, includeMissing];
    refreshKey;
    api.listFolders(v, p, missing).then(
      (list) => {
        folders = list;
        // A lone subfolder (e.g. home → torinou) opens by itself.
        const only = list.length === 1 ? list[0] : null;
        if (only?.hasChildren && untrack(() => expanded[only.path]) === undefined) expanded[only.path] = true;
      },
      () => (folders = []),
    );
  });

  // Open the folders leading to the selection (e.g. after clicking a breadcrumb).
  $effect(() => {
    const sel = selectedPath;
    if (!sel || !folders) return;
    for (const f of folders) {
      if (sel.startsWith(f.path + "/") && f.hasChildren && !untrack(() => expanded[f.path])) expanded[f.path] = true;
    }
  });
</script>

{#if folders === null}
  <div class="node muted" style:padding-left="{depth * 14 + 18}px">…</div>
{:else}
  {#each folders as f (f.path)}
    <div class="node" class:active={selectedPath === f.path} style:padding-left="{depth * 14}px">
      {#if f.hasChildren}
        <button
          class="chevron"
          aria-label={expanded[f.path] ? `Collapse ${f.name}` : `Expand ${f.name}`}
          aria-expanded={!!expanded[f.path]}
          onclick={() => (expanded[f.path] = !expanded[f.path])}>{expanded[f.path] ? "▾" : "▸"}</button
        >
      {:else}
        <span class="chevron"></span>
      {/if}
      <button class="label" title={f.path} onclick={() => onSelect(volumeId, f.path)}>
        <span class="name">{f.name}</span>
        <span class="count">{formatCount(f.count)}</span>
      </button>
    </div>
    {#if expanded[f.path]}
      <FolderTree
        {volumeId}
        parent={f.path}
        depth={depth + 1}
        {refreshKey}
        {includeMissing}
        {selectedPath}
        {onSelect}
      />
    {/if}
  {/each}
{/if}

<style>
  .node {
    display: flex;
    align-items: center;
    border-radius: 4px;
    min-height: 24px;
  }
  .node:hover {
    background: var(--row-hover);
  }
  .node.active {
    background: var(--row-selected);
  }
  button {
    border: none;
    background: none;
    padding: 0;
  }
  .chevron {
    flex: none;
    width: 18px;
    text-align: center;
    color: var(--muted);
  }
  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    padding: 3px 8px 3px 0;
    text-align: left;
  }
  .name {
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
</style>
