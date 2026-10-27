<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    api,
    onJobsProgress,
    type CameraCount,
    type FileQuery,
    type FileRow,
    type MediaKind,
    type Orientation,
    type SetupInfo,
    type Sort,
    type Volume,
  } from "./lib/api";
  import { formatCount } from "./lib/format";
  import { PagedFiles } from "./lib/paged.svelte";
  import Setup from "./lib/Setup.svelte";
  import DriveList from "./lib/DriveList.svelte";
  import FileList from "./lib/FileList.svelte";
  import FileGrid from "./lib/FileGrid.svelte";
  import Inspector from "./lib/Inspector.svelte";

  type View = "grid" | "list";
  type Duration = "" | "short" | "medium" | "long";

  let setup = $state<SetupInfo | null>(null);
  let volumes = $state<Volume[]>([]);
  let cameras = $state<CameraCount[]>([]);
  let error = $state("");

  // Filters
  let selectedVolume = $state<number | null>(null);
  let selectedFolder = $state<string | null>(null);
  let includeSubfolders = $state(true);
  let text = $state("");
  let searchText = $state("");
  let kind = $state<MediaKind | null>(null);
  let sort = $state<Sort>("newest");
  let includeMissing = $state(false);
  let minSide = $state<number | null>(null);
  let orientation = $state<Orientation | null>(null);
  let camera = $state("");
  let dateFrom = $state("");
  let dateTo = $state("");
  let duration = $state<Duration>("");

  // View
  let view = $state<View>(loadPref("view", "grid") as View);
  let tileSize = $state(Number(loadPref("tileSize", "180")));
  let selected = $state<{ id: number; index: number } | null>(null);
  let refreshKey = $state(0);

  const files = new PagedFiles(showError);

  const labels = $derived(new Map(volumes.filter((v) => v.id !== null).map((v) => [v.id!, v.label])));

  /** "2024-05-01" → unix seconds at the start (or end) of that day, in camera-clock time. */
  function day(value: string, endOfDay = false): number | null {
    if (!value) return null;
    const [y, m, d] = value.split("-").map(Number);
    return Date.UTC(y, m - 1, d) / 1000 + (endOfDay ? 86_399 : 0);
  }

  const durationRanges: Record<Duration, [number | null, number | null]> = {
    "": [null, null],
    short: [null, 30_000],
    medium: [30_000, 300_000],
    long: [300_000, null],
  };

  const query = $derived<FileQuery>({
    text: searchText,
    volumeId: selectedVolume,
    kind,
    includeMissing,
    sort,
    minSide,
    orientation,
    camera: camera || null,
    dateFrom: day(dateFrom),
    dateTo: day(dateTo, true),
    minDurationMs: durationRanges[duration][0],
    maxDurationMs: durationRanges[duration][1],
    folder: selectedVolume !== null ? selectedFolder : null,
    directOnly: selectedFolder !== null && !includeSubfolders,
  });

  /** Breadcrumb segments for the selected folder: [label, path]. */
  const crumbs = $derived(
    selectedFolder
      ? selectedFolder.split("/").map((name, i, parts) => [name, parts.slice(0, i + 1).join("/")] as const)
      : [],
  );

  const filtersActive = $derived(
    !!(searchText || kind || minSide || orientation || camera || dateFrom || dateTo || duration || includeMissing),
  );

  // untrack: setQuery reads the result count, which must not re-trigger the query.
  $effect(() => {
    const q = query;
    untrack(() => {
      files.setQuery(q);
      selected = null;
    });
  });

  // Search while typing, but not on every keystroke.
  $effect(() => {
    const value = text;
    const timer = setTimeout(() => (searchText = value.trim()), 250);
    return () => clearTimeout(timer);
  });

  $effect(() => savePref("view", view));
  $effect(() => savePref("tileSize", String(tileSize)));

  function loadPref(key: string, fallback: string): string {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  }

  function savePref(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Preferences are a convenience; ignore storage errors.
    }
  }

  function showError(message: string) {
    error = message;
  }

  function catalogChanged() {
    files.refresh();
    refreshKey++;
  }

  function clearFilters() {
    text = searchText = "";
    kind = minSide = orientation = null;
    camera = dateFrom = dateTo = duration = "";
    includeMissing = false;
  }

  async function open(row: FileRow) {
    try {
      await api.openFile(row.id);
    } catch (e) {
      showError(String(e));
    }
  }

  async function loadCameras() {
    try {
      cameras = await api.listCameras();
    } catch {
      // Not critical; the list fills in on the next try.
    }
  }

  onMount(() => {
    let lastRefresh = 0;
    let lastCameras = 0;
    api.getSetup().then((s) => {
      setup = s;
      if (s.catalogDir) loadCameras();
    });
    // While thumbnails are being made, refresh the visible files every few seconds so they appear.
    const unlisten = onJobsProgress(() => {
      const now = Date.now();
      if (now - lastRefresh > 3000) {
        lastRefresh = now;
        catalogChanged();
      }
      if (now - lastCameras > 20_000) {
        lastCameras = now;
        loadCameras();
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  });
</script>

{#if !setup}
  <div></div>
{:else if !setup.catalogDir}
  <Setup info={setup} onDone={async () => (setup = await api.getSetup())} />
{:else}
  <div class="app" class:with-inspector={selected !== null}>
    <aside>
      <DriveList
        bind:volumes
        bind:selected={selectedVolume}
        bind:folder={selectedFolder}
        {includeMissing}
        onCatalogChanged={catalogChanged}
        onError={showError}
      />
    </aside>

    <main>
      <div class="toolbar">
        <input class="search" type="search" placeholder="Search file and folder names…" bind:value={text} />
        <div class="segmented" role="group" aria-label="View">
          <button class:on={view === "grid"} onclick={() => (view = "grid")}>Grid</button>
          <button class:on={view === "list"} onclick={() => (view = "list")}>List</button>
        </div>
        {#if view === "grid"}
          <input type="range" min="110" max="360" step="10" bind:value={tileSize} aria-label="Thumbnail size" />
        {/if}
        <select bind:value={sort} aria-label="Sort">
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="name">Name</option>
          <option value="largest">Largest first</option>
        </select>
        <span class="count muted">{formatCount(files.total)} files</span>
      </div>

      <div class="toolbar filters">
        <select bind:value={kind} aria-label="File type">
          <option value={null}>All types</option>
          <option value="image">Images</option>
          <option value="video">Videos</option>
          <option value="raw">RAW photos</option>
        </select>
        <select bind:value={minSide} aria-label="Resolution">
          <option value={null}>Any resolution</option>
          <option value={1280}>HD and up (1280+)</option>
          <option value={1920}>Full HD and up (1920+)</option>
          <option value={3840}>4K and up (3840+)</option>
          <option value={6000}>Very large (6000+)</option>
        </select>
        <select bind:value={orientation} aria-label="Orientation">
          <option value={null}>Any orientation</option>
          <option value="landscape">Landscape</option>
          <option value="portrait">Portrait</option>
          <option value="square">Square</option>
        </select>
        <select bind:value={camera} aria-label="Camera">
          <option value="">Any camera</option>
          {#each cameras as c (c.camera)}
            <option value={c.camera}>{c.camera} ({formatCount(c.count)})</option>
          {/each}
        </select>
        <select bind:value={duration} aria-label="Video length">
          <option value="">Any length</option>
          <option value="short">Under 30 s</option>
          <option value="medium">30 s – 5 min</option>
          <option value="long">Over 5 min</option>
        </select>
        <label>From <input type="date" bind:value={dateFrom} /></label>
        <label>To <input type="date" bind:value={dateTo} /></label>
        <label><input type="checkbox" bind:checked={includeMissing} /> Show deleted</label>
        {#if filtersActive}
          <button onclick={clearFilters}>Clear filters</button>
        {/if}
      </div>

      {#if selectedVolume !== null && selectedFolder !== null}
        <nav class="toolbar crumbs" aria-label="Folder">
          <button class="crumb" onclick={() => (selectedFolder = null)}>{labels.get(selectedVolume) ?? "Drive"}</button>
          {#each crumbs as [name, path], i (path)}
            <span class="muted">/</span>
            {#if i === crumbs.length - 1}
              <span class="crumb current">{name}</span>
            {:else}
              <button class="crumb" onclick={() => (selectedFolder = path)}>{name}</button>
            {/if}
          {/each}
          <label class="subfolders"><input type="checkbox" bind:checked={includeSubfolders} /> Include subfolders</label>
          <button onclick={() => (selectedFolder = null)} aria-label="Show the whole drive">✕</button>
        </nav>
      {/if}

      {#if error}
        <div class="error">
          <span>{error}</span>
          <button onclick={() => (error = "")}>Dismiss</button>
        </div>
      {/if}

      <div class="content">
        {#if view === "grid"}
          <FileGrid {files} bind:selected {tileSize} {labels} onOpen={open} />
        {:else}
          <FileList {files} bind:selected {labels} onOpen={open} />
        {/if}
      </div>
    </main>

    {#if selected}
      <Inspector fileId={selected.id} {refreshKey} {labels} onError={showError} />
    {/if}
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    height: 100%;
  }
  .app.with-inspector {
    grid-template-columns: 280px minmax(0, 1fr) 320px;
  }
  aside {
    min-height: 0;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .filters {
    padding-top: 0;
  }
  .search {
    flex: 1;
    min-width: 160px;
  }
  .segmented {
    display: flex;
  }
  .segmented button {
    border-radius: 0;
  }
  .segmented button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .segmented button:last-child {
    border-radius: 0 4px 4px 0;
    margin-left: -1px;
  }
  .segmented button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .count {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .crumbs {
    gap: 4px;
    padding-top: 0;
  }
  .crumb {
    border: none;
    background: none;
    padding: 2px 4px;
    color: var(--accent);
  }
  .crumb.current {
    color: var(--text);
    font-weight: 600;
  }
  .subfolders {
    margin-left: auto;
  }
  label {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--danger);
    color: #fff;
  }
  .error button {
    background: none;
    border-color: #fff;
    color: #fff;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
</style>
