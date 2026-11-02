<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    api,
    onJobsProgress,
    type CameraCount,
    type FileDetails,
    type FileQuery,
    type MediaKind,
    type Orientation,
    type SetupInfo,
    type Sort,
  } from "./lib/api";
  import { Drives } from "./lib/drives.svelte";
  import { durationBuckets, kindOptions, orientationOptions, ranges, sideBuckets } from "./lib/filters";
  import { formatCount } from "./lib/format";
  import { PagedFiles } from "./lib/paged.svelte";
  import Compare from "./lib/Compare.svelte";
  import DriveStrip from "./lib/DriveStrip.svelte";
  import DrivesView from "./lib/DrivesView.svelte";
  import DupesView from "./lib/DupesView.svelte";
  import FileList from "./lib/FileList.svelte";
  import FilterMenu from "./lib/FilterMenu.svelte";
  import FolderPanel from "./lib/FolderPanel.svelte";
  import type { Option } from "./lib/MultiSelect.svelte";
  import PhotoGrid from "./lib/PhotoGrid.svelte";
  import Rail, { type Screen } from "./lib/Rail.svelte";
  import SelectionBar from "./lib/SelectionBar.svelte";
  import Setup from "./lib/Setup.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import UpdateBanner from "./lib/UpdateBanner.svelte";
  import Viewer from "./lib/Viewer.svelte";

  type View = "grid" | "list";

  let setup = $state<SetupInfo | null>(null);
  let cameras = $state<CameraCount[]>([]);
  let error = $state("");
  let notice = $state("");

  // Filters
  let selectedVolume = $state<number | null>(null);
  let selectedFolder = $state<string | null>(null);
  let includeSubfolders = $state(true);
  let text = $state("");
  let searchText = $state("");
  let kinds = $state<string[]>([]);
  let sort = $state<Sort>("newest");
  let includeMissing = $state(false);
  let sides = $state<string[]>([]);
  let orientations = $state<string[]>([]);
  let chosenCameras = $state<string[]>([]);
  let dateFrom = $state("");
  let dateTo = $state("");
  let durations = $state<string[]>([]);
  /** Showing the pictures that look like one file. */
  let similar = $state<{ name: string; ids: number[] } | null>(null);

  // View
  let screen = $state<Screen>("library");
  let showFolders = $state(false);
  let view = $state<View>(loadPref("view", "grid") as View);
  let density = $state(Number(loadPref("density", "200")));
  let focused = $state<number | null>(null);
  let listSelected = $state<{ id: number; index: number } | null>(null);
  let viewerIndex = $state<number | null>(null);
  let comparing = $state<FileDetails[] | null>(null);
  let refreshKey = $state(0);

  const selection = new SvelteSet<number>();
  const files = new PagedFiles(showError);
  const drives = new Drives(catalogChanged, showError);

  const labels = $derived(new Map(drives.catalogued.map((v) => [v.id!, v.label])));

  /** "2024-05-01" → unix seconds at the start (or end) of that day, in camera-clock time. */
  function day(value: string, endOfDay = false): number | null {
    if (!value) return null;
    const [y, m, d] = value.split("-").map(Number);
    return Date.UTC(y, m - 1, d) / 1000 + (endOfDay ? 86_399 : 0);
  }

  const query = $derived<FileQuery>({
    text: searchText,
    volumeId: selectedVolume,
    kinds: kinds as MediaKind[],
    includeMissing,
    sort,
    sideRanges: ranges(sideBuckets, sides),
    orientations: orientations as Orientation[],
    cameras: chosenCameras,
    dateFrom: day(dateFrom),
    dateTo: day(dateTo, true),
    durationRanges: ranges(durationBuckets, durations),
    folder: selectedVolume !== null ? selectedFolder : null,
    directOnly: selectedFolder !== null && !includeSubfolders,
    ids: similar?.ids ?? null,
  });

  const cameraOptions = $derived<Option[]>(cameras.map((c) => ({ value: c.camera, label: c.camera, count: c.count })));

  /** The filters in force, shown as removable tags in the search bar. */
  const tags = $derived.by(() => {
    const list: { key: string; label: string; clear: () => void }[] = [];
    const add = (prefix: string, opts: Option[], chosen: string[], clear: (value: string) => void) => {
      for (const value of chosen) {
        list.push({
          key: `${prefix}:${value}`,
          label: opts.find((o) => o.value === value)?.label.replace(/ \(.*/, "") ?? value,
          clear: () => clear(value),
        });
      }
    };
    if (similar) list.push({ key: "similar", label: `Like ${similar.name}`, clear: () => (similar = null) });
    add("kind", kindOptions, kinds, (v) => (kinds = kinds.filter((k) => k !== v)));
    add("camera", cameraOptions, chosenCameras, (v) => (chosenCameras = chosenCameras.filter((k) => k !== v)));
    add("side", sideBuckets, sides, (v) => (sides = sides.filter((k) => k !== v)));
    add("orientation", orientationOptions, orientations, (v) => (orientations = orientations.filter((k) => k !== v)));
    add("length", durationBuckets, durations, (v) => (durations = durations.filter((k) => k !== v)));
    if (dateFrom) list.push({ key: "from", label: `From ${dateFrom}`, clear: () => (dateFrom = "") });
    if (dateTo) list.push({ key: "to", label: `To ${dateTo}`, clear: () => (dateTo = "") });
    if (includeMissing) list.push({ key: "missing", label: "Deleted shown", clear: () => (includeMissing = false) });
    return list;
  });

  /** What is narrowing the results, for the "no files match" message. */
  const activeFilters = $derived.by(() => {
    const list = tags.map((t) => t.label);
    if (selectedVolume !== null) list.unshift(`drive “${labels.get(selectedVolume) ?? "?"}”`);
    if (selectedFolder) list.unshift(`folder “${selectedFolder}”${includeSubfolders ? "" : " (no subfolders)"}`);
    if (searchText) list.unshift(`search “${searchText}”`);
    return list;
  });

  /** Breadcrumb segments for the selected folder: [label, path]. */
  const crumbs = $derived(
    selectedFolder
      ? selectedFolder.split("/").map((name, i, parts) => [name, parts.slice(0, i + 1).join("/")] as const)
      : [],
  );

  const emptyMessage = $derived(
    activeFilters.length
      ? `No files match: ${activeFilters.join(" · ")}.`
      : "No files yet. Open Drives to add one and scan it.",
  );

  // Camera counts follow the other filters (debounced: it is a grouped count over the catalog).
  $effect(() => {
    const q = query;
    const timer = setTimeout(() => loadCameras(q), 300);
    return () => clearTimeout(timer);
  });

  // untrack: setQuery reads the result count, which must not re-trigger the query.
  $effect(() => {
    const q = query;
    untrack(() => {
      files.setQuery(q);
      selection.clear();
      focused = null;
      listSelected = null;
      viewerIndex = null;
    });
  });

  // Search while typing, but not on every keystroke.
  $effect(() => {
    const value = text;
    const timer = setTimeout(() => (searchText = value.trim()), 250);
    return () => clearTimeout(timer);
  });

  $effect(() => savePref("view", view));
  $effect(() => savePref("density", String(density)));

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

  async function findSimilar(id: number) {
    try {
      const [ids, file] = await Promise.all([api.similarFiles(id), api.fileDetails(id)]);
      // The list always starts with the file itself.
      if (ids.length < 2) {
        notice = `No similar pictures found for ${file.name}. Files still waiting for a thumbnail can't be compared yet.`;
        return;
      }
      similar = { name: file.name, ids };
      viewerIndex = null;
      screen = "library";
    } catch (e) {
      showError(String(e));
    }
  }

  function trashed(message: string) {
    notice = message;
    selection.clear();
    files.refresh();
    drives.refresh();
  }

  async function loadCameras(q: FileQuery) {
    try {
      const list = await api.listCameras(q);
      if (q === query) cameras = list; // ignore answers for filters already changed
    } catch {
      // Not critical; the list fills in on the next try.
    }
  }

  onMount(() => {
    let lastRefresh = 0;
    let lastCameras = 0;
    let stopWatching = () => {};
    api.getSetup().then((s) => (setup = s));
    // While thumbnails are being made, refresh the visible files every few seconds so they appear.
    const unlisten = onJobsProgress(() => {
      const now = Date.now();
      if (now - lastRefresh > 3000) {
        lastRefresh = now;
        catalogChanged();
      }
      if (now - lastCameras > 20_000) {
        lastCameras = now;
        loadCameras(untrack(() => query));
      }
    });
    stopWatching = drives.watch();
    return () => {
      stopWatching();
      unlisten.then((off) => off());
    };
  });
</script>

{#if !setup}
  <div></div>
{:else if !setup.catalogDir}
  <Setup
    info={setup}
    onDone={async () => {
      setup = await api.getSetup();
      error = "";
      await drives.refresh();
      files.setQuery(query);
    }}
  />
{:else}
  <div class="app">
    <Rail bind:screen bind:folders={showFolders} />

    <main>
      <UpdateBanner />

      {#if screen === "library"}
        <div class="top">
          <div class="search">
            <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.500" /><path d="m16 16 4.500 4.500" /></svg>
            {#each tags as t (t.key)}
              <button class="tag" title="Remove this filter" onclick={t.clear}>{t.label} <span aria-hidden="true">×</span></button>
            {/each}
            <input type="search" placeholder="Search file and folder names…" aria-label="Search" bind:value={text} />
            <FilterMenu
              bind:kinds
              bind:sides
              bind:orientations
              bind:chosenCameras
              bind:durations
              bind:dateFrom
              bind:dateTo
              bind:includeMissing
              {cameraOptions}
              active={tags.length > 0}
            />
          </div>
          <select bind:value={sort} aria-label="Sort">
            <option value="newest">Newest</option>
            <option value="oldest">Oldest</option>
            <option value="name">Name</option>
            <option value="largest">Largest</option>
          </select>
          {#if view === "grid"}
            <input class="density" type="range" min="110" max="280" step="10" bind:value={density} aria-label="Thumbnail size" />
          {/if}
          <div class="segmented" role="group" aria-label="View">
            <button class:on={view === "grid"} onclick={() => (view = "grid")}>Photos</button>
            <button class:on={view === "list"} onclick={() => (view = "list")}>List</button>
          </div>
        </div>

        <DriveStrip {drives} bind:selected={selectedVolume} bind:folder={selectedFolder} onManage={() => (screen = "drives")} />

        {#if selectedVolume !== null && selectedFolder !== null}
          <nav class="crumbs" aria-label="Folder">
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
      {/if}

      {#if error}
        <div class="message error">
          <span>{error}</span>
          <button onclick={() => (error = "")}>Dismiss</button>
        </div>
      {/if}
      {#if notice}
        <div class="message">
          <span>{notice}</span>
          <button onclick={() => (notice = "")}>Dismiss</button>
        </div>
      {/if}

      <div class="content">
        {#if screen === "dupes"}
          <div class="files"><DupesView {labels} onError={showError} /></div>
        {:else if screen === "drives"}
          <div class="files"><DrivesView {drives} bind:selected={selectedVolume} bind:folder={selectedFolder} /></div>
        {:else}
          {#if showFolders}
            <FolderPanel {drives} bind:selected={selectedVolume} bind:folder={selectedFolder} {includeMissing} />
          {/if}
          <div class="files">
            {#if view === "grid"}
              <PhotoGrid
                {files}
                {query}
                {density}
                {selection}
                bind:focused
                {labels}
                {emptyMessage}
                onOpen={(index) => (viewerIndex = index)}
                onError={showError}
              />
            {:else}
              <FileList
                {files}
                bind:selected={listSelected}
                {labels}
                {emptyMessage}
                onOpen={() => (viewerIndex = listSelected?.index ?? null)}
              />
            {/if}
            {#if selection.size}
              <SelectionBar
                {selection}
                onCompare={(list) => (comparing = list)}
                onSimilar={findSimilar}
                onTrashed={trashed}
                onError={showError}
              />
            {/if}
          </div>
        {/if}
      </div>

      <StatusBar {drives} total={drives.catalogued.reduce((n, v) => n + v.fileCount, 0)} />
    </main>
  </div>

  {#if viewerIndex !== null}
    {#if notice}
      <div class="viewer-notice" role="status">
        <span>{notice}</span>
        <button onclick={() => (notice = "")}>Dismiss</button>
      </div>
    {/if}
    <Viewer
      {files}
      bind:index={viewerIndex}
      {refreshKey}
      {labels}
      onClose={() => {
        focused = viewerIndex;
        viewerIndex = null;
      }}
      onError={showError}
      onSimilar={findSimilar}
    />
  {/if}
  {#if comparing}
    <Compare files={comparing} onClose={() => (comparing = null)} />
  {/if}
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    height: 100%;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .top {
    display: flex;
    flex: none;
    gap: 14px;
    align-items: center;
    padding: 14px 24px 12px;
  }
  .search {
    display: flex;
    flex: 1;
    gap: 6px;
    align-items: center;
    min-width: 0;
    min-height: 44px;
    padding: 4px 8px 4px 14px;
    border: 1px solid var(--border);
    border-radius: 22px;
    background: var(--panel);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search svg {
    flex: none;
    width: 17px;
    height: 17px;
    fill: none;
    stroke: var(--muted);
    stroke-width: 2;
    stroke-linecap: round;
  }
  .search input {
    flex: 1;
    min-width: 80px;
    padding: 4px;
    border: none;
    outline: none;
    background: none;
  }
  .tag {
    flex: none;
    max-width: 190px;
    height: 28px;
    padding: 0 10px;
    overflow: hidden;
    border: none;
    border-radius: 14px;
    background: var(--row-selected);
    color: var(--accent);
    font-size: 12px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .top > select {
    border: none;
    background: none;
    color: var(--muted);
  }
  .density {
    width: 110px;
    padding: 0;
    border: none;
    background: none;
    accent-color: var(--accent);
  }
  .segmented {
    display: flex;
    flex: none;
    padding: 3px;
    border-radius: 8px;
    background: var(--panel);
  }
  .segmented button {
    padding: 3px 10px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    font-size: 12px;
  }
  .segmented button.on {
    background: var(--raised);
    color: var(--text);
  }
  .crumbs {
    display: flex;
    flex: none;
    gap: 4px;
    align-items: center;
    padding: 0 24px 8px;
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
    display: flex;
    gap: 4px;
    align-items: center;
    margin-left: auto;
    white-space: nowrap;
  }
  .message {
    display: flex;
    flex: none;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    margin: 0 24px 8px;
    padding: 7px 12px;
    border-radius: 8px;
    background: var(--raised);
  }
  .message button {
    background: none;
    border-color: currentColor;
    color: inherit;
  }
  .viewer-notice {
    position: fixed;
    bottom: 124px;
    left: 50%;
    z-index: 60;
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--raised);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.5);
    transform: translateX(-50%);
  }
  .error {
    background: var(--danger);
    color: #fff;
  }
  .content {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .content > :global(*) {
    min-width: 0;
  }
  .files {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
  }
</style>
