<script lang="ts" module>
  export type Screen = "library" | "dupes" | "drives";
</script>

<script lang="ts">
  let {
    screen = $bindable(),
    folders = $bindable(),
  }: {
    screen: Screen;
    /** Whether the folder panel is open. */
    folders: boolean;
  } = $props();

  function toggleFolders() {
    folders = screen !== "library" || !folders;
    screen = "library";
  }
</script>

<nav aria-label="Main">
  <div class="logo" aria-hidden="true">R</div>
  <button class:on={screen === "library" && !folders} onclick={() => ((screen = "library"), (folders = false))}>
    <svg viewBox="0 0 24 24"><path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z" /></svg>
    Library
  </button>
  <button class:on={screen === "library" && folders} onclick={toggleFolders}>
    <svg viewBox="0 0 24 24"><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H10l2 2.5h7.5A1.5 1.5 0 0 1 21 9v9.500a1.500 1.500 0 0 1-1.500 1.500h-15A1.500 1.500 0 0 1 3 18.500z" /></svg>
    Folders
  </button>
  <button class:on={screen === "dupes"} onclick={() => (screen = "dupes")}>
    <svg viewBox="0 0 24 24"><rect x="3" y="3" width="13" height="13" rx="2" /><rect x="8" y="8" width="13" height="13" rx="2" /></svg>
    Duplicates
  </button>
  <div class="spring"></div>
  <button class:on={screen === "drives"} onclick={() => (screen = "drives")}>
    <svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><path d="M7 7.500h.01M7 16.500h.01" /></svg>
    Drives
  </button>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 76px;
    padding: 14px 8px 12px;
    background: var(--panel);
    border-right: 1px solid var(--border);
  }
  .logo {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    margin-bottom: 12px;
    border-radius: 9px;
    background: var(--accent);
    color: #06222f;
    font-size: 16px;
    font-weight: 800;
  }
  button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 60px;
    padding: 8px 0 7px;
    border: none;
    border-radius: 10px;
    background: none;
    color: var(--muted);
    font-size: 10px;
    font-weight: 500;
  }
  button:hover:not(.on) {
    color: var(--text);
  }
  button.on {
    background: var(--raised);
    color: var(--text);
  }
  svg {
    width: 22px;
    height: 22px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .on svg {
    stroke: var(--accent);
  }
  .spring {
    flex: 1;
  }
</style>
