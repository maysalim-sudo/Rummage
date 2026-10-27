<script lang="ts">
  import { type FileRow } from "./api";
  import { formatBytes, formatDate, formatDuration } from "./format";
  import type { PagedFiles } from "./paged.svelte";

  let {
    files,
    selected = $bindable(),
    labels,
    onOpen,
  }: {
    files: PagedFiles;
    selected: { id: number; index: number } | null;
    labels: Map<number, string>;
    onOpen: (row: FileRow) => void;
  } = $props();

  // Virtualized: only rows in view are rendered.
  const ROW_HEIGHT = 30;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewHeight = $state(600);

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 5));
  const last = $derived(Math.min(files.total, Math.ceil((scrollTop + viewHeight) / ROW_HEIGHT) + 5));
  const visible = $derived(
    Array.from({ length: Math.max(0, last - first) }, (_, i) => ({ index: first + i, row: files.get(first + i) })),
  );

  $effect(() => files.ensure(first, Math.max(last, first + 1)));

  // A new query starts at the top.
  $effect(() => {
    files.queryVersion;
    if (viewport) viewport.scrollTop = 0;
  });

  function select(index: number) {
    const row = files.get(index);
    if (!row) return;
    selected = { id: row.id, index };
    // Keep the selection in view when moving with the keyboard.
    if (viewport) {
      const top = index * ROW_HEIGHT;
      if (top < viewport.scrollTop) viewport.scrollTop = top;
      else if (top + ROW_HEIGHT > viewport.scrollTop + viewHeight) viewport.scrollTop = top + ROW_HEIGHT - viewHeight;
    }
  }

  function onKey(e: KeyboardEvent) {
    const index = selected?.index ?? -1;
    const move = (to: number) => {
      e.preventDefault();
      select(Math.max(0, Math.min(files.total - 1, to)));
    };
    if (e.key === "ArrowDown") move(index + 1);
    else if (e.key === "ArrowUp") move(index - 1);
    else if (e.key === "PageDown") move(index + Math.floor(viewHeight / ROW_HEIGHT));
    else if (e.key === "PageUp") move(index - Math.floor(viewHeight / ROW_HEIGHT));
    else if (e.key === "Enter" && selected) {
      const row = files.get(selected.index);
      if (row) onOpen(row);
    }
  }

  const kindLabel = { image: "Image", video: "Video", raw: "RAW", design: "Design" } as const;

  function size(row: FileRow) {
    return row.width && row.height ? `${row.width}×${row.height}` : "";
  }
</script>

<div class="list">
  <div class="header row">
    <span>Name</span><span>Type</span><span class="num">Size</span><span>Dimensions</span><span>Date</span><span>Drive</span><span>Folder</span>
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
    <div class="spacer" style:height="{files.total * ROW_HEIGHT}px">
      {#each visible as { index, row } (index)}
        <div class="slot" style:top="{index * ROW_HEIGHT}px" style:height="{ROW_HEIGHT}px">
          {#if row}
            <button
              class="row"
              class:selected={row.id === selected?.id}
              class:offline={!row.absPath}
              class:missing={row.missing}
              onclick={() => select(index)}
              ondblclick={() => onOpen(row)}
              title={row.absPath ?? `Offline — plug in ${labels.get(row.volumeId) ?? "the drive"}`}
            >
              <span class="name">{row.name}</span>
              <span><span class="badge {row.kind}">{row.kind ? kindLabel[row.kind] : row.ext}</span></span>
              <span class="num">{formatBytes(row.size)}</span>
              <span>{size(row)}{row.durationMs ? ` · ${formatDuration(row.durationMs)}` : ""}</span>
              <span>{formatDate(row.date)}</span>
              <span>{labels.get(row.volumeId) ?? "?"}</span>
              <span class="dir">{row.dir || "/"}</span>
            </button>
          {:else}
            <div class="row placeholder"><span class="muted">Loading…</span></div>
          {/if}
        </div>
      {/each}
    </div>
    {#if files.loaded && files.total === 0}
      <div class="empty muted">No files match. Add a drive on the left, or change the filters.</div>
    {/if}
  </div>
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
    grid-template-columns: minmax(180px, 2fr) 60px 72px 130px 140px minmax(80px, 0.7fr) minmax(120px, 2fr);
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
  .badge.design {
    background: var(--badge-design);
  }
  .empty {
    position: absolute;
    inset: 40px 0 auto;
    text-align: center;
  }
</style>
