<script lang="ts">
  import { thumbUrl, type FileRow } from "./api";
  import { formatDay, formatDuration } from "./format";
  import type { PagedFiles } from "./paged.svelte";

  let {
    files,
    selected = $bindable(),
    tileSize,
    labels,
    emptyMessage,
    onOpen,
  }: {
    files: PagedFiles;
    selected: { id: number; index: number } | null;
    tileSize: number;
    labels: Map<number, string>;
    emptyMessage: string;
    onOpen: (row: FileRow) => void;
  } = $props();

  // Virtualized by rows of tiles: only rows in view are rendered.
  const GAP = 8;
  const CAPTION = 38;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewWidth = $state(800);
  let viewHeight = $state(600);

  const cols = $derived(Math.max(1, Math.floor((viewWidth - GAP) / (tileSize + GAP))));
  const rowHeight = $derived(tileSize + CAPTION + GAP);
  const rowCount = $derived(Math.ceil(files.total / cols));
  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - 2));
  const lastRow = $derived(Math.min(rowCount, Math.ceil((scrollTop + viewHeight) / rowHeight) + 2));
  const tiles = $derived.by(() => {
    const list: { index: number; row: FileRow | undefined; top: number; left: number }[] = [];
    for (let r = firstRow; r < lastRow; r++) {
      for (let c = 0; c < cols; c++) {
        const index = r * cols + c;
        if (index >= files.total) break;
        list.push({ index, row: files.get(index), top: GAP + r * rowHeight, left: GAP + c * (tileSize + GAP) });
      }
    }
    return list;
  });

  $effect(() => files.ensure(firstRow * cols, Math.max(lastRow * cols, 1)));

  $effect(() => {
    files.queryVersion;
    if (viewport) viewport.scrollTop = 0;
  });

  function select(index: number) {
    const row = files.get(index);
    if (!row) return;
    selected = { id: row.id, index };
    if (viewport) {
      const top = GAP + Math.floor(index / cols) * rowHeight;
      if (top < viewport.scrollTop) viewport.scrollTop = top - GAP;
      else if (top + rowHeight > viewport.scrollTop + viewHeight) viewport.scrollTop = top + rowHeight - viewHeight;
    }
  }

  function onKey(e: KeyboardEvent) {
    const index = selected?.index ?? -1;
    const move = (to: number) => {
      e.preventDefault();
      select(Math.max(0, Math.min(files.total - 1, to)));
    };
    if (e.key === "ArrowRight") move(index + 1);
    else if (e.key === "ArrowLeft") move(index - 1);
    else if (e.key === "ArrowDown") move(index + cols);
    else if (e.key === "ArrowUp") move(index - cols);
    else if (e.key === "Enter" && selected) {
      const row = files.get(selected.index);
      if (row) onOpen(row);
    }
  }

  const kindIcon = { image: "🖼", video: "🎬", raw: "📷", design: "✏️" } as const;
</script>

<div
  class="viewport"
  role="listbox"
  aria-label="Files"
  bind:this={viewport}
  bind:clientWidth={viewWidth}
  bind:clientHeight={viewHeight}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  onkeydown={onKey}
  tabindex="0"
>
  <div class="spacer" style:height="{rowCount * rowHeight + GAP}px">
    {#each tiles as { index, row, top, left } (index)}
      {#if row}
        {@const url = thumbUrl(row)}
        <button
          class="tile"
          class:selected={row.id === selected?.id}
          class:offline={!row.absPath}
          style:top="{top}px"
          style:left="{left}px"
          style:width="{tileSize}px"
          onclick={() => select(index)}
          ondblclick={() => onOpen(row)}
          title={row.absPath ?? `${row.name} — offline, plug in ${labels.get(row.volumeId) ?? "the drive"}`}
        >
          <div class="thumb" style:height="{tileSize}px">
            {#if url}
              <img src={url} alt="" decoding="async" draggable="false" />
            {:else}
              <span class="placeholder">{row.kind ? kindIcon[row.kind] : "?"}</span>
            {/if}
            {#if row.kind === "video" && row.durationMs}
              <span class="chip">{formatDuration(row.durationMs)}</span>
            {:else if row.kind === "raw"}
              <span class="chip raw">RAW</span>
            {:else if row.kind === "design"}
              <span class="chip design">{row.ext.toUpperCase()}</span>
            {/if}
          </div>
          <div class="caption">
            <span class="name">{row.name}</span>
            <span class="sub muted">{formatDay(row.date)}{row.absPath ? "" : ` · ${labels.get(row.volumeId) ?? "offline"}`}</span>
          </div>
        </button>
      {:else}
        <div class="tile loading" style:top="{top}px" style:left="{left}px" style:width="{tileSize}px">
          <div class="thumb" style:height="{tileSize}px"></div>
        </div>
      {/if}
    {/each}
  </div>
  {#if files.loaded && files.total === 0}
    <div class="empty muted">{emptyMessage}</div>
  {/if}
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    position: relative;
    outline: none;
    background: var(--panel);
  }
  .spacer {
    position: relative;
  }
  .tile {
    position: absolute;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 6px;
    background: none;
    text-align: left;
    overflow: hidden;
  }
  .tile:hover:not(.loading) {
    border-color: var(--border);
  }
  .tile.selected {
    border-color: var(--accent);
    background: var(--row-selected);
  }
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    background: var(--bg);
    border-radius: 4px;
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .offline .thumb img {
    opacity: 0.55;
  }
  .placeholder {
    font-size: 32px;
    opacity: 0.4;
  }
  .chip {
    position: absolute;
    right: 6px;
    bottom: 6px;
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 11px;
    color: #fff;
    background: rgb(0 0 0 / 0.65);
  }
  .chip.raw {
    background: var(--badge-raw);
  }
  .chip.design {
    background: var(--badge-design);
  }
  .caption {
    display: flex;
    flex-direction: column;
    padding: 3px 4px 0;
    line-height: 1.3;
  }
  .caption span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 11px;
  }
  .empty {
    position: absolute;
    inset: 40px 0 auto;
    text-align: center;
  }
</style>
