<script lang="ts">
  import { onMount } from "svelte";
  import { api, type FileQuery, type MediaKind, type SetupInfo, type Sort, type Volume } from "./lib/api";
  import Setup from "./lib/Setup.svelte";
  import DriveList from "./lib/DriveList.svelte";
  import FileList from "./lib/FileList.svelte";

  let setup = $state<SetupInfo | null>(null);
  let volumes = $state<Volume[]>([]);
  let selectedVolume = $state<number | null>(null);
  let text = $state("");
  let searchText = $state("");
  let kind = $state<MediaKind | null>(null);
  let sort = $state<Sort>("newest");
  let includeMissing = $state(false);
  let refreshKey = $state(0);
  let error = $state("");

  const labels = $derived(new Map(volumes.filter((v) => v.id !== null).map((v) => [v.id!, v.label])));
  const query = $derived<FileQuery>({ text: searchText, volumeId: selectedVolume, kind, includeMissing, sort });

  // Search while typing, but not on every keystroke.
  $effect(() => {
    const value = text;
    const timer = setTimeout(() => (searchText = value.trim()), 250);
    return () => clearTimeout(timer);
  });

  function showError(message: string) {
    error = message;
  }

  onMount(async () => {
    setup = await api.getSetup();
  });
</script>

{#if !setup}
  <div></div>
{:else if !setup.catalogDir}
  <Setup info={setup} onDone={async () => (setup = await api.getSetup())} />
{:else}
  <div class="app">
    <aside>
      <DriveList
        bind:volumes
        bind:selected={selectedVolume}
        onCatalogChanged={() => refreshKey++}
        onError={showError}
      />
    </aside>
    <main>
      <div class="toolbar">
        <input class="search" type="search" placeholder="Search file and folder names…" bind:value={text} />
        <select bind:value={kind} aria-label="File type">
          <option value={null}>All types</option>
          <option value="image">Images</option>
          <option value="video">Videos</option>
          <option value="raw">RAW photos</option>
        </select>
        <select bind:value={sort} aria-label="Sort">
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="name">Name</option>
          <option value="largest">Largest first</option>
        </select>
        <label><input type="checkbox" bind:checked={includeMissing} /> Show deleted</label>
      </div>
      {#if error}
        <div class="error">
          <span>{error}</span>
          <button onclick={() => (error = "")}>Dismiss</button>
        </div>
      {/if}
      <FileList {query} {refreshKey} {labels} onError={showError} />
    </main>
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100%;
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
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .search {
    flex: 1;
    min-width: 160px;
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
  main > :global(.list) {
    flex: 1;
    min-height: 0;
  }
</style>
