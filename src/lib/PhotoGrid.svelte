<script lang="ts">
  import { untrack } from "svelte";
  import type { SvelteSet } from "svelte/reactivity";
  import { api, thumbUrl, type FileQuery, type FileRow } from "./api";
  import { formatCount, formatDay, formatDuration } from "./format";
  import { computeLayout, displayRatio, parseLayout, rowAt, rowOf, GROUP_HEADER, type LayoutData } from "./layout";
  import type { PagedFiles } from "./paged.svelte";
  import Timeline from "./Timeline.svelte";

  let {
    files,
    query,
    density,
    selection,
    focused = $bindable(),
    labels,
    emptyMessage,
    onOpen,
    onError,
  }: {
    files: PagedFiles;
    query: FileQuery;
    /** Target row height in pixels. */
    density: number;
    selection: SvelteSet<number>;
    /** The file the keyboard is on, and where the view returns after the viewer. */
    focused: number | null;
    labels: Map<number, string>;
    emptyMessage: string;
    onOpen: (index: number) => void;
    onError: (message: string) => void;
  } = $props();

  // Virtualized by rows: only rows in view exist, however many files match.
  const GAP = 6;
  const OVERSCAN = 700;
  const SIDE_PADDING = 24;
  const TIMELINE_WIDTH = 56;
  /** Narrower tiles show only the spinner, leaving room for the file type. */
  const MIN_WAITING_LABEL = 130;
  /** How many files one shift-click may select. */
  const MAX_RANGE = 5000;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewWidth = $state(1000);
  let viewHeight = $state(700);
  let data = $state.raw<LayoutData | null>(null);
  let anchor: number | null = null;
  let generation = 0;

  const grouped = $derived(query.sort === "newest" || query.sort === "oldest");
  const width = $derived(Math.max(200, viewWidth - SIDE_PADDING - TIMELINE_WIDTH));
  const layout = $derived(data ? computeLayout(data, width, density, GAP, grouped) : null);

  const tiles = $derived.by(() => {
    const list: { index: number; row: FileRow | undefined; top: number; left: number; width: number; height: number }[] = [];
    if (!layout || !data || !layout.rowCount) return list;
    const bottom = scrollTop + viewHeight + OVERSCAN;
    for (let r = rowAt(layout, scrollTop - OVERSCAN); r < layout.rowCount && layout.rowTop[r] < bottom; r++) {
      const height = layout.rowHeight[r];
      let left = 0;
      for (let i = layout.rowStart[r]; i < layout.rowStart[r + 1]; i++) {
        const w = displayRatio(data.ratios[i]) * height;
        list.push({ index: i, row: files.get(i), top: layout.rowTop[r], left, width: w, height });
        left += w + GAP;
      }
    }
    return list;
  });

  const headers = $derived(
    (layout?.groups ?? []).filter((g) => g.top + GROUP_HEADER > scrollTop - OVERSCAN && g.top < scrollTop + viewHeight + OVERSCAN),
  );

  $effect(() => {
    if (tiles.length) files.ensure(tiles[0].index, tiles[tiles.length - 1].index + 1);
  });

  async function load(q: FileQuery) {
    const mine = ++generation;
    try {
      const buffer = await api.fileLayout(q);
      if (mine === generation) data = parseLayout(buffer);
    } catch (e) {
      if (mine === generation) onError(String(e));
    }
  }

  // A new query starts at the top.
  $effect(() => {
    const q = query;
    files.queryVersion;
    untrack(() => {
      load(q);
      anchor = null;
      if (viewport) viewport.scrollTop = 0;
    });
  });

  // Files are appearing or leaving (a scan, the trash): lay out again once it settles.
  $effect(() => {
    const total = files.total;
    if (!files.loaded || !data || total === data.dates.length) return;
    const timer = setTimeout(() => load(untrack(() => query)), 1500);
    return () => clearTimeout(timer);
  });

  // Keep the keyboard's file in view, also when the viewer moved it.
  $effect(() => {
    const index = focused;
    if (index === null) return;
    untrack(() => reveal(index));
  });

  function reveal(index: number) {
    if (!layout || !viewport || !layout.rowCount) return;
    const r = rowOf(layout, index);
    const top = layout.rowTop[r];
    const bottom = top + layout.rowHeight[r];
    if (top - GROUP_HEADER < viewport.scrollTop) viewport.scrollTop = top - GROUP_HEADER;
    else if (bottom > viewport.scrollTop + viewHeight) viewport.scrollTop = bottom - viewHeight + GAP;
  }

  function toggle(id: number) {
    if (!selection.delete(id)) selection.add(id);
  }

  async function selectRange(index: number) {
    const from = Math.min(anchor ?? index, index);
    const count = Math.min(Math.abs((anchor ?? index) - index) + 1, MAX_RANGE);
    try {
      for (const id of await api.fileIds(query, from, count)) selection.add(id);
    } catch (e) {
      onError(String(e));
    }
  }

  function onTile(e: MouseEvent, index: number, row: FileRow) {
    focused = index;
    if (e.shiftKey) selectRange(index);
    else if (e.ctrlKey || e.metaKey || selection.size) toggle(row.id);
    else onOpen(index);
    if (!e.shiftKey) anchor = index;
  }

  function onCheck(e: MouseEvent, index: number, row: FileRow) {
    e.stopPropagation();
    focused = index;
    anchor = index;
    toggle(row.id);
  }

  function onKey(e: KeyboardEvent) {
    if (!layout || !files.total) return;
    const index = focused ?? -1;
    const move = (to: number) => {
      e.preventDefault();
      focused = Math.max(0, Math.min(files.total - 1, to));
    };
    const vertical = (delta: number) => {
      const r = rowOf(layout, Math.max(index, 0));
      const next = Math.max(0, Math.min(layout.rowCount - 1, r + delta));
      const offset = Math.max(index, 0) - layout.rowStart[r];
      move(Math.min(layout.rowStart[next] + offset, layout.rowStart[next + 1] - 1));
    };
    if (e.key === "ArrowRight") move(index + 1);
    else if (e.key === "ArrowLeft") move(index - 1);
    else if (e.key === "ArrowDown") vertical(1);
    else if (e.key === "ArrowUp") vertical(-1);
    else if (e.key === "PageDown") vertical(Math.ceil(viewHeight / density));
    else if (e.key === "PageUp") vertical(-Math.ceil(viewHeight / density));
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(files.total - 1);
    else if (e.key === "Enter" && focused !== null) onOpen(focused);
    else if (e.key === " " && focused !== null) {
      e.preventDefault();
      const row = files.get(focused);
      if (row) toggle(row.id);
    } else if (e.key === "Escape") selection.clear();
  }

  const monthName = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric", timeZone: "UTC" });
  const groupTitle = (year: number, month: number) => monthName.format(Date.UTC(year, month, 15));

  function chip(row: FileRow): string | null {
    if (row.kind === "video") return row.durationMs ? formatDuration(row.durationMs) : "Video";
    if (row.kind === "raw") return "RAW";
    if (row.kind === "design") return row.ext.toUpperCase();
    return null;
  }
</script>

<div class="grid">
  <div
    class="viewport"
    role="listbox"
    aria-label="Files"
    aria-multiselectable="true"
    bind:this={viewport}
    bind:clientWidth={viewWidth}
    bind:clientHeight={viewHeight}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    onkeydown={onKey}
    tabindex="0"
  >
    <div class="spacer" style:height="{layout?.height ?? 0}px" style:width="{width}px">
      {#each headers as g (g.index)}
        <div class="header" style:top="{g.top}px">
          <h2>{groupTitle(g.year, g.month)}</h2>
          <span class="muted">{formatCount(g.count)} {g.count === 1 ? "file" : "files"}</span>
        </div>
      {/each}
      {#each tiles as t (t.index)}
        {#if t.row}
          {@const row = t.row}
          {@const url = thumbUrl(row)}
          {@const label = chip(row)}
          {@const picked = selection.has(row.id)}
          <div
            class="tile"
            class:picked
            class:focus={focused === t.index}
            class:offline={!row.absPath}
            class:deleted={row.missing}
            class:selecting={selection.size > 0}
            style:top="{t.top}px"
            style:left="{t.left}px"
            style:width="{t.width}px"
            style:height="{t.height}px"
            role="option"
            aria-selected={picked}
            tabindex="-1"
            title={row.absPath ?? `${row.name}: offline, plug in ${labels.get(row.volumeId) ?? "the drive"}`}
            onclick={(e) => onTile(e, t.index, row)}
            ondblclick={() => {}}
            onkeydown={() => {}}
          >
            {#if url}
              <img src={url} alt="" decoding="async" draggable="false" />
            {:else if !row.processed}
              <div class="waiting">
                <span class="ring"></span>
                {#if t.width >= MIN_WAITING_LABEL}<span>Making thumbnail</span>{/if}
              </div>
            {:else}
              <div class="waiting"><span class="ext">{row.ext.toUpperCase()}</span><span>No preview</span></div>
            {/if}
            {#if label}<span class="chip">{label}</span>{/if}
            <button
              class="check"
              aria-label={picked ? `Deselect ${row.name}` : `Select ${row.name}`}
              tabindex="-1"
              onclick={(e) => onCheck(e, t.index, row)}>✓</button
            >
            <div class="caption">
              <span class="name">{row.name}</span>
              <span class="sub">{row.camera ? `${row.camera} · ` : ""}{formatDay(row.date)}{row.absPath ? "" : ` · ${labels.get(row.volumeId) ?? "offline"}`}</span>
            </div>
          </div>
        {:else}
          <div class="tile loading" style:top="{t.top}px" style:left="{t.left}px" style:width="{t.width}px" style:height="{t.height}px"></div>
        {/if}
      {/each}
    </div>
    {#if files.loaded && files.total === 0}
      <div class="empty muted">{emptyMessage}</div>
    {/if}
  </div>
  {#if layout && grouped && layout.groups.length > 1}
    <Timeline {layout} {scrollTop} {viewHeight} onJump={(top) => viewport && (viewport.scrollTop = top)} />
  {/if}
</div>

<style>
  .grid {
    position: relative;
    height: 100%;
  }
  .viewport {
    height: 100%;
    overflow-x: hidden;
    overflow-y: auto;
    padding-left: 24px;
    outline: none;
    scrollbar-width: none;
  }
  .spacer {
    position: relative;
  }
  .header {
    position: absolute;
    left: 0;
    display: flex;
    gap: 12px;
    align-items: baseline;
    padding-top: 12px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  .tile {
    position: absolute;
    overflow: hidden;
    border-radius: 6px;
    background: var(--panel);
    cursor: pointer;
  }
  .tile.loading {
    cursor: default;
  }
  .tile img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .tile.focus {
    outline: 2px solid var(--muted);
    outline-offset: 1px;
  }
  .tile.picked {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  .tile.offline img {
    opacity: 0.6;
  }
  .tile.deleted img {
    opacity: 0.35;
    filter: grayscale(1);
  }
  .waiting {
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 6px;
    background: var(--raised);
    color: var(--muted);
    font-size: 11px;
    text-align: center;
  }
  .ring {
    position: relative;
    width: 26px;
    height: 26px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 1.4s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation: none;
    }
  }
  .ext {
    font-size: 15px;
    font-weight: 700;
  }
  .chip {
    position: absolute;
    bottom: 8px;
    left: 8px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgb(0 0 0 / 0.65);
    color: #fff;
    font-size: 10px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .check {
    position: absolute;
    top: 8px;
    left: 8px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1.5px solid rgb(255 255 255 / 0.85);
    border-radius: 50%;
    background: rgb(0 0 0 / 0.35);
    color: transparent;
    font-size: 11px;
    font-weight: 800;
    line-height: 1;
    opacity: 0;
  }
  .tile:hover .check,
  .tile.picked .check,
  .tile.selecting .check {
    opacity: 1;
  }
  .tile.picked .check {
    border-color: var(--accent);
    background: var(--accent);
    color: #06222f;
  }
  .caption {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    display: none;
    flex-direction: column;
    padding: 18px 10px 7px;
    background: linear-gradient(transparent, rgb(0 0 0 / 0.72));
    color: #fff;
    pointer-events: none;
  }
  .tile:hover .caption {
    display: flex;
  }
  .tile:hover .chip {
    display: none;
  }
  .caption span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-size: 12px;
    font-weight: 500;
  }
  .sub {
    color: rgb(255 255 255 / 0.75);
    font-size: 11px;
  }
  .empty {
    padding: 48px 24px;
    text-align: center;
  }
</style>
