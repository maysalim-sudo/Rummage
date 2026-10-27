<script lang="ts">
  import { onMount } from "svelte";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { formatBytes } from "./format";

  // Checks GitHub Releases once at startup; updates are signature-checked before installing.
  let update = $state<Update | null>(null);
  let dismissed = $state(false);
  let installing = $state(false);
  let downloaded = $state(0);
  let size = $state(0);
  let error = $state("");

  onMount(async () => {
    try {
      update = await check();
    } catch {
      // Offline, or no release published yet: try again next launch.
    }
  });

  async function install() {
    if (!update) return;
    installing = true;
    error = "";
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") size = event.data.contentLength ?? 0;
        else if (event.event === "Progress") downloaded += event.data.chunkLength;
      });
      await relaunch();
    } catch (e) {
      error = String(e);
      installing = false;
    }
  }
</script>

{#if update && !dismissed}
  <div class="banner" role="status">
    <span>
      <strong>Rummage {update.version}</strong> is available.
      {#if installing}
        Downloading{size ? ` ${formatBytes(downloaded)} of ${formatBytes(size)}` : "…"}
      {:else if error}
        <span class="err">Update failed: {error}</span>
      {/if}
    </span>
    {#if !installing}
      <button class="primary" onclick={install}>Install and restart</button>
      <button onclick={() => (dismissed = true)}>Later</button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    background: var(--row-selected);
    border-bottom: 1px solid var(--border);
  }
  .banner > span {
    flex: 1;
  }
  .err {
    color: var(--danger);
  }
</style>
