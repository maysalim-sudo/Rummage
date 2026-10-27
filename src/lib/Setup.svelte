<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type SetupInfo } from "./api";
  import { formatBytes } from "./format";

  let { info, onDone }: { info: SetupInfo; onDone: () => void } = $props();

  // Catalog + thumbnails for ~1M files need roughly 20–40 GB.
  const RECOMMENDED_FREE = 40 * 1024 ** 3;

  // Starts from the default; the user may then pick another folder.
  let dir = $state(untrack(() => info.defaultDir));
  let free = $state<number | null>(untrack(() => info.defaultFreeBytes));
  let busy = $state(false);
  let error = $state("");

  async function choose() {
    const picked = await open({ directory: true, title: "Choose where to keep the catalog" });
    if (typeof picked === "string") {
      dir = picked;
      free = await api.freeSpace(picked);
    }
  }

  async function confirm() {
    busy = true;
    error = "";
    try {
      await api.setCatalogDir(dir);
      onDone();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="setup">
  <div class="card">
    <h1>Welcome to Media Catalog</h1>
    <p>
      Choose where to keep the catalog. It stores the file list and, later, thumbnails and search
      data for all your drives. Pick a fast SSD with plenty of free space.
    </p>

    <div class="path">
      <code>{dir}</code>
      <button onclick={choose} disabled={busy}>Choose folder…</button>
    </div>
    <p class="muted">
      Free space: {free === null ? "unknown" : formatBytes(free)}
      {#if free !== null && free < RECOMMENDED_FREE}
        <span class="warn"> · Less than 40 GB free; a large library may not fit.</span>
      {/if}
    </p>

    {#if error}<p class="error">{error}</p>{/if}

    <button class="primary" onclick={confirm} disabled={busy}>Use this folder</button>
  </div>
</div>

<style>
  .setup {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 16px;
  }
  .card {
    max-width: 560px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 24px 28px;
  }
  h1 {
    font-size: 20px;
    margin: 0 0 8px;
  }
  .path {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 16px 0 4px;
  }
  code {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow-wrap: anywhere;
  }
  .warn,
  .error {
    color: var(--danger);
  }
</style>
