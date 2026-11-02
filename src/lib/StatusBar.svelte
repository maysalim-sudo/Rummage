<script lang="ts">
  import { api } from "./api";
  import type { Drives } from "./drives.svelte";
  import { formatCount } from "./format";

  let { drives, total }: { drives: Drives; total: number } = $props();

  const jobs = $derived(drives.jobs);
  const scans = $derived(Object.values(drives.progress));
  const working = $derived(jobs.volumes.length > 0);
  const online = $derived(drives.catalogued.filter((v) => v.online).length);
  /** Drives waiting for another on the same physical disk. */
  const waiting = $derived(jobs.volumes.filter((p) => p.state === "queued").length);

  function timeLeft(ms: number | null): string {
    if (ms === null) return "";
    const minutes = Math.round(ms / 60_000);
    if (minutes < 1) return "under a minute left";
    if (minutes < 90) return `about ${minutes} min left`;
    return `about ${Math.round(minutes / 60)} h left`;
  }
</script>

<footer>
  {#if working}
    <progress max={drives.jobsTotal} value={drives.jobsDone}></progress>
    <span class="muted">
      {jobs.paused ? "Paused" : "Making thumbnails"} · {formatCount(drives.jobsDone)} of {formatCount(drives.jobsTotal)}{#if !jobs.paused && drives.remainingMs !== null}
        {" "}· {timeLeft(drives.remainingMs)}{/if}{#if waiting}
        {" "}· {waiting} {waiting === 1 ? "drive" : "drives"} waiting for the same disk{/if}
    </span>
    <button onclick={() => drives.run(() => api.setJobsPaused(!jobs.paused))}>{jobs.paused ? "Resume" : "Pause"}</button>
  {:else if jobs.paused}
    <span class="muted">Background work is paused</span>
    <button onclick={() => drives.run(() => api.setJobsPaused(false))}>Resume</button>
  {:else if scans.length}
    <span class="muted">Scanning… {formatCount(scans.reduce((n, p) => n + p.stats.files_seen, 0))} files</span>
  {:else}
    <span class="muted">All thumbnails are up to date</span>
  {/if}
  <span class="spring"></span>
  <span class="muted">{formatCount(total)} files · {drives.catalogued.length} drives · {online} online</span>
</footer>

<style>
  footer {
    display: flex;
    flex: none;
    gap: 12px;
    align-items: center;
    min-height: 32px;
    padding: 0 24px;
    border-top: 1px solid var(--border);
    background: var(--panel);
    font-size: 12px;
    white-space: nowrap;
  }
  progress {
    width: 140px;
    height: 4px;
    accent-color: var(--accent);
  }
  button {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-weight: 500;
  }
  .spring {
    flex: 1;
  }
</style>
