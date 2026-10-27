<script lang="ts">
  import { untrack } from "svelte";
  import { api, type FileQuery, type FileRow } from "./api";
  import { formatBytes, formatCount, formatDate } from "./format";

  let {
    query,
    refreshKey,
    labels,
    onError,
  }: {
    query: FileQuery;
    refreshKey: number;
    labels: Map<number, string>;
    onError: (message: string) => void;
  } = $props();

  // Virtualized list: only the rows in view are rendered, loaded in pages on demand.
  const ROW_HEIGHT = 30;
  const PAGE_SIZE = 200;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewHeight = $state(600);
  let total = $state(0);
  let pages = $state<Record<number, FileRow[]>>({});
  let selectedId = $state<number | null>(null);
  let generation = 0;
  const loading = new Set<number>();

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 5));
  const last = $derived(Math.min(total, Math.ceil((scrollTop + viewHeight) / ROW_HEIGHT) + 5));
  const visible = $derived(
    Array.from({ length: Math.max(0, last - first) }, (_, i) => {
      const index = first + i;
      return { index, row: pages[Math.floor(index / PAGE_SIZE)]?.[index % PAGE_SIZE] };
    }),
  );
  const selected = $derived(
    selectedId === null ? null : Object.values(pages).flat().find((r) => r.id === selectedId) ?? null,
  );

  async function loadPage(page: number, gen: number) {
    if (loading.has(page)) return;
    loading.add(page);
    try {
      const result = await api.queryFiles(query, page * PAGE_SIZE, PAGE_SIZE);
      if (gen !== generation) return; // a newer query replaced this one
      total = result.total;
      pages[page] = result.rows;
    } catch (e) {
      if (gen === generation) onError(String(e));
    } finally {
      loading.delete(page);
    }
  }

  function reset(scrollToTop: boolean) {
    generation++;
    loading.clear();
    pages = {};
    if (scrollToTop && viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
    loadPage(Math.floor(scrollTop / ROW_HEIGHT / PAGE_SIZE), generation);
  }

  // A new filter starts at the top; a catalog refresh (scan progress) keeps the position.
  // untrack: reset reads scroll state, which must not re-trigger these effects.
  $effect(() => {
    JSON.stringify(query);
    untrack(() => reset(true));
  });
  let seenRefresh = untrack(() => refreshKey);
  $effect(() => {
    if (refreshKey !== seenRefresh) {
      seenRefresh = refreshKey;
      untrack(() => reset(false));
    }
  });

  $effect(() => {
    const firstPage = Math.floor(first / PAGE_SIZE);
    const lastPage = Math.floor(Math.max(first, last - 1) / PAGE_SIZE);
    for (let p = firstPage; p <= lastPage; p++) {
      if (!pages[p] && p * PAGE_SIZE < Math.max(total, 1)) loadPage(p, generation);
    }
  });

  async function act(action: (id: number) => Promise<void>, row: FileRow | null) {
    if (!row) return;
    try {
      await action(row.id);
    } catch (e) {
      onError(String(e));
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") act(api.openFile, selected);
  }

  const kindLabel = { image: "Image", video: "Video", raw: "RAW" } as const;
</script>

<div class="list">
  <div class="header row">
    <span>Name</span><span>Type</span><span class="num">Size</span><span>Modified</span><span>Drive</span><span>Folder</span>
  </div>

  <div
    class="viewport"
    role="listbox"
    aria-label="Files"
    bind:this={viewport}
    bind:clientHeight={viewHeight}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    onkeydown={onKey}
    tabindex="0"
  >
    <div class="spacer" style:height="{total * ROW_HEIGHT}px">
      {#each visible as { index, row } (index)}
        <div class="slot" style:top="{index * ROW_HEIGHT}px" style:height="{ROW_HEIGHT}px">
          {#if row}
            <button
              class="row"
              class:selected={row.id === selectedId}
              class:offline={!row.absPath}
              class:missing={row.missing}
              onclick={() => (selectedId = row.id)}
              ondblclick={() => act(api.openFile, row)}
              title={row.absPath ?? `Offline — plug in ${labels.get(row.volumeId) ?? "the drive"}`}
            >
              <span class="name">{row.name}</span>
              <span><span class="badge {row.kind}">{row.kind ? kindLabel[row.kind] : row.ext}</span></span>
              <span class="num">{formatBytes(row.size)}</span>
              <span>{formatDate(row.mtime)}</span>
              <span>{labels.get(row.volumeId) ?? "?"}</span>
              <span class="dir">{row.dir || "/"}</span>
            </button>
          {:else}
            <div class="row placeholder"><span class="muted">Loading…</span></div>
          {/if}
        </div>
      {/each}
    </div>
    {#if total === 0}
      <div class="empty muted">No files match. Add a drive on the left, or change the filters.</div>
    {/if}
  </div>

  <footer>
    <span class="muted">{formatCount(total)} files</span>
    {#if selected}
      <span class="sel" title={selected.absPath ?? ""}>
        {#if selected.absPath}{selected.absPath}{:else}Offline — plug in “{labels.get(selected.volumeId)}” to open{/if}
      </span>
      <button disabled={!selected.absPath} onclick={() => act(api.openFile, selected)}>Open</button>
      <button disabled={!selected.absPath} onclick={() => act(api.revealFile, selected)}>Show in folder</button>
    {/if}
  </footer>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--panel);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(180px, 2fr) 70px 80px 150px minmax(80px, 0.7fr) minmax(120px, 2fr);
    gap: 12px;
    align-items: center;
    width: 100%;
    height: 100%;
    padding: 0 12px;
    border: none;
    border-radius: 0;
    background: none;
    text-align: left;
  }
  .row > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header {
    height: 30px;
    flex: none;
    font-weight: 600;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
  }
  .viewport {
    flex: 1;
    overflow-y: auto;
    position: relative;
    outline: none;
  }
  .spacer {
    position: relative;
  }
  .slot {
    position: absolute;
    left: 0;
    right: 0;
  }
  button.row:hover {
    background: var(--row-hover);
  }
  button.row.selected {
    background: var(--row-selected);
  }
  .offline {
    color: var(--muted);
  }
  .missing .name {
    text-decoration: line-through;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .dir {
    color: var(--muted);
    direction: rtl;
    text-align: left;
  }
  .badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 3px;
    color: #fff;
    background: var(--muted);
  }
  .badge.image {
    background: var(--badge-image);
  }
  .badge.video {
    background: var(--badge-video);
  }
  .badge.raw {
    background: var(--badge-raw);
  }
  .empty {
    position: absolute;
    inset: 40px 0 auto;
    text-align: center;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-top: 1px solid var(--border);
    flex: none;
  }
  .sel {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
