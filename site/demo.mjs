// A working copy of Rummage's library for the landing page: search, drives, a justified
// grid and the full-screen viewer. All artwork is drawn here, from made-up files.
// `mountRummageDemo(element)` fills the element and returns a function that removes it.

const DRIVES = [
  { id: "studio", name: "Studio SSD", count: "72k", used: 0.62, online: true },
  { id: "work", name: "Work", count: "31k", used: 0.71, online: true },
  { id: "archive", name: "Archive", count: "42k", used: 0.46, online: true },
  { id: "backup", name: "Backup HDD", count: "194k", used: 0.93, online: true },
  { id: "travel", name: "Travel SSD", count: "2k", used: 0.34, online: false },
];

// [name, drive, month, ratio, kind, camera, palette, scene, extra]
const FILES = [
  ["Harbor_sunrise.jpg", "work", "October 2026", 1.5, "photo", "Canon EOS R5", 0, "hills"],
  ["Portrait_0412.psd", "studio", "October 2026", 0.67, "design", null, 0, "waiting", "PSD"],
  ["Old_town_rooftops.jpg", "work", "October 2026", 1.78, "photo", "Canon EOS R5", 2, "city"],
  ["Drone_coast_4K.mp4", "archive", "October 2026", 1.33, "video", "DJI Mini 4", 3, "hills", "0:48"],
  ["Ridge_at_dusk.jpg", "work", "October 2026", 1.5, "photo", "Sony A7 IV", 4, "hills"],
  ["Blue_hour_street.jpg", "backup", "October 2026", 1, "photo", "Sony A7 IV", 5, "city"],
  ["Mountain_lake.cr3", "archive", "October 2026", 1.78, "raw", "Canon EOS R5", 6, "hills", "RAW"],
  ["Logo_final_v3.ai", "studio", "October 2026", 0.75, "design", null, 7, "hills", "AI"],
  ["Market_day.jpg", "travel", "October 2026", 1.5, "photo", "Fujifilm X-T5", 0, "city"],
  ["Tide_pools.jpg", "backup", "October 2026", 1.33, "photo", "Fujifilm X-T5", 1, "hills"],
  ["Festival_night.mov", "archive", "September 2026", 1.5, "video", "Sony A7 IV", 2, "hills", "1:12"],
  ["Pine_forest.jpg", "backup", "September 2026", 1.78, "photo", "Canon EOS R5", 3, "city"],
  ["Canyon_morning.heic", "travel", "September 2026", 0.8, "photo", "iPhone 16 Pro", 4, "hills"],
  ["Coastal_road.jpg", "work", "September 2026", 1.5, "photo", "Canon EOS R5", 5, "hills"],
  ["Client_brief.pdf", "studio", "September 2026", 1.33, "design", null, 0, "waiting", "PDF"],
  ["Lighthouse.jpg", "archive", "September 2026", 1.78, "photo", "Sony A7 IV", 6, "hills"],
  ["Night_trains.dng", "backup", "September 2026", 1, "raw", "Sony A7 IV", 7, "city", "RAW"],
  ["Dunes_at_noon.jpg", "archive", "September 2026", 1.5, "photo", "Fujifilm X-T5", 0, "hills"],
];

const PALETTES = [
  ["#f7b267", "#f25c54", "#3d2c5e"], ["#8ecae6", "#219ebc", "#023047"], ["#ffd6a5", "#e5989b", "#6d6875"],
  ["#c7e9b0", "#7aa874", "#2c3e2d"], ["#fcbf49", "#d62828", "#1d1a2f"], ["#cdb4db", "#6a4c93", "#1b1b3a"],
  ["#e0fbfc", "#98c1d9", "#293241"], ["#f4a261", "#2a9d8f", "#132a2f"],
];

const GAP = 6;
const escape = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

function random(seed) {
  let value = seed;
  return () => (value = (value * 16807) % 2147483647) / 2147483647;
}

/** A small generated picture, as an image URL. */
function scene(file, seed) {
  const w = Math.round(600 * file.ratio);
  const h = 600;
  const rnd = random(seed * 7919 + 13);
  const [top, middle, dark] = PALETTES[file.palette];
  let art = "";
  if (file.scene === "city") {
    for (let x = -10; x < w; ) {
      const bw = 40 + rnd() * w * 0.11;
      const bh = h * (0.25 + rnd() * 0.5);
      art += `<rect x="${x | 0}" y="${(h - bh) | 0}" width="${bw | 0}" height="${bh | 0}" fill="${dark}"/>`;
      x += bw + 5;
    }
  } else {
    art = `<circle cx="${w * (0.2 + rnd() * 0.6) | 0}" cy="${h * (0.2 + rnd() * 0.2) | 0}" r="${h * 0.1 | 0}" fill="#fff4d6" opacity=".92"/>`;
    for (let layer = 0; layer < 2; layer++) {
      const base = h * (0.55 + layer * 0.2);
      let points = `0,${h}`;
      for (let k = 0; k <= 6; k++) points += ` ${(w * k) / 6 | 0},${(base + (rnd() - 0.5) * h * 0.22) | 0}`;
      art += `<polygon points="${points} ${w},${h}" fill="${dark}" opacity="${layer ? 1 : 0.65}"/>`;
    }
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}"><defs><linearGradient id="s" x2="0" y2="1"><stop stop-color="${top}"/><stop offset="1" stop-color="${middle}"/></linearGradient></defs><rect width="${w}" height="${h}" fill="url(#s)"/>${art}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** Rows of real picture shapes, each row filling the width. */
function rows(files, width, target) {
  const out = [];
  let row = [];
  let sum = 0;
  for (const file of files) {
    row.push(file);
    sum += file.ratio;
    const height = (width - GAP * (row.length - 1)) / sum;
    if (height <= target) {
      out.push({ files: row, height });
      row = [];
      sum = 0;
    }
  }
  if (row.length) out.push({ files: row, height: target });
  return out;
}

export function mountRummageDemo(root) {
  const files = FILES.map(([name, drive, month, ratio, kind, camera, palette, scene, extra], i) => ({
    id: i, name, drive, month, ratio, kind, camera, palette, scene, extra,
  }));
  for (const file of files) file.url = file.scene === "waiting" ? null : scene(file, file.id + 1);

  const state = { text: "", kind: null, drive: null, viewer: null };
  const driveName = (id) => DRIVES.find((d) => d.id === id)?.name ?? "";
  const shown = () =>
    files.filter(
      (f) =>
        (!state.drive || f.drive === state.drive) &&
        (!state.kind || f.kind === state.kind) &&
        f.name.toLowerCase().includes(state.text.toLowerCase()),
    );

  root.classList.add("rd");
  root.innerHTML = `
    <div class="rd-window">
      <nav class="rd-rail" aria-hidden="true">
        <span class="rd-logo">R</span>
        <span class="rd-nav on"><i></i>Library</span>
        <span class="rd-nav"><i></i>Folders</span>
        <span class="rd-nav"><i></i>Duplicates</span>
      </nav>
      <div class="rd-main">
        <div class="rd-top">
          <label class="rd-search">
            <span class="rd-tags"></span>
            <input type="search" placeholder="Try harbor, .psd or night" aria-label="Search the demo library">
          </label>
          <div class="rd-kinds" role="group" aria-label="Filter by type">
            ${[["photo", "Photos"], ["video", "Videos"], ["design", "Design"]].map(([k, l]) => `<button data-kind="${k}">${l}</button>`).join("")}
          </div>
        </div>
        <div class="rd-strip" role="group" aria-label="Drives"></div>
        <div class="rd-grid" tabindex="0" aria-label="Demo library"></div>
        <div class="rd-status">
          <span class="rd-bar"><span></span></span>
          <span>Making thumbnails · 147,541 of 341,615 · about 9 h left</span>
          <span class="rd-end">341,615 files · 5 drives · 4 online</span>
        </div>
      </div>
      <div class="rd-viewer" hidden role="dialog" aria-label="Viewer"></div>
    </div>`;

  const grid = root.querySelector(".rd-grid");
  const strip = root.querySelector(".rd-strip");
  const tags = root.querySelector(".rd-tags");
  const input = root.querySelector("input");
  const viewer = root.querySelector(".rd-viewer");

  function drawStrip() {
    const all = `<button class="rd-drive${state.drive ? "" : " on"}" data-drive=""><span class="rd-line"><i class="rd-dot"></i><b>All drives</b><em>341,615</em></span><span class="rd-use"><span style="width:100%" class="all"></span></span></button>`;
    strip.innerHTML =
      all +
      DRIVES.map(
        (d) => `<button class="rd-drive${state.drive === d.id ? " on" : ""}${d.online ? "" : " off"}" data-drive="${d.id}">
          <span class="rd-line"><i class="rd-dot${d.online ? "" : " none"}"></i><b>${escape(d.name)}</b><em>${d.online ? d.count : "Offline"}</em></span>
          <span class="rd-use"><span style="width:${d.used * 100}%" class="${d.used > 0.9 ? "full" : ""}"></span></span>
        </button>`,
      ).join("");
  }

  function drawGrid() {
    const width = Math.max(240, grid.clientWidth - 32);
    const target = width < 560 ? 110 : 158;
    const list = shown();
    if (!list.length) {
      grid.innerHTML = `<p class="rd-empty">No files match. Rummage would tell you exactly which filter hid them.</p>`;
      return;
    }
    let html = "";
    for (const month of [...new Set(list.map((f) => f.month))]) {
      const group = list.filter((f) => f.month === month);
      html += `<h4>${month} <span>${group.length} ${group.length === 1 ? "file" : "files"}</span></h4>`;
      for (const row of rows(group, width, target)) {
        html += `<div class="rd-row" style="height:${row.height}px">`;
        for (const f of row.files) {
          html += `<button class="rd-tile" data-file="${f.id}" style="width:${f.ratio * row.height}px" aria-label="Open ${escape(f.name)}">
            ${f.url ? `<img src="${f.url}" alt="" draggable="false">` : `<span class="rd-wait"><i></i>${f.ratio * row.height >= 130 ? "Making thumbnail" : ""}</span>`}
            ${f.extra ? `<span class="rd-chip">${f.extra}</span>` : ""}
            <span class="rd-cap"><b>${escape(f.name)}</b><span>${escape(f.camera ?? driveName(f.drive))}</span></span>
          </button>`;
        }
        html += `</div>`;
      }
    }
    grid.innerHTML = html;
  }

  function drawTags() {
    const list = [];
    if (state.kind) list.push(root.querySelector(`[data-kind="${state.kind}"]`).textContent);
    tags.innerHTML = list.map((t) => `<button data-clear>${t} ×</button>`).join("");
    for (const b of root.querySelectorAll("[data-kind]")) b.classList.toggle("on", b.dataset.kind === state.kind);
  }

  function drawViewer() {
    const list = shown();
    const file = list[state.viewer];
    viewer.hidden = !file;
    if (!file) return;
    const nearby = list.slice(Math.max(0, state.viewer - 4), state.viewer + 5);
    viewer.innerHTML = `
      <div class="rd-stage-col">
        <div class="rd-vhead"><button data-close>← Library</button><b>${escape(file.name)}</b><span>${state.viewer + 1} of ${list.length}</span></div>
        <div class="rd-stage">
          ${file.url ? `<img src="${file.url}" alt="">` : `<span class="rd-wait"><i></i>Preview on its way</span>`}
          <button class="rd-go prev" data-go="-1" aria-label="Previous" ${state.viewer ? "" : "disabled"}>‹</button>
          <button class="rd-go next" data-go="1" aria-label="Next" ${state.viewer < list.length - 1 ? "" : "disabled"}>›</button>
        </div>
        <div class="rd-film">${nearby.map((f) => `<button data-pick="${list.indexOf(f)}" class="${f === file ? "on" : ""}" aria-label="${escape(f.name)}">${f.url ? `<img src="${f.url}" alt="">` : ""}</button>`).join("")}</div>
      </div>
      <aside class="rd-details">
        <button class="rd-x" data-close aria-label="Close viewer">✕</button>
        <strong>${escape(file.camera ?? "Design file")}</strong>
        ${file.camera ? `<div class="rd-exp"><span>f/2.8</span><span>1/250</span><span>ISO 200</span></div>` : ""}
        <dl><dt>Taken</dt><dd>${file.month}</dd><dt>Where</dt><dd>${escape(driveName(file.drive))}</dd></dl>
        ${file.id % 3 === 0 ? `<p class="rd-copies"><b>2 copies of this file</b>Also on Backup HDD</p>` : ""}
        <span class="rd-open">Open in default app</span>
      </aside>`;
  }

  function onClick(e) {
    const t = e.target.closest("button");
    if (!t) return;
    if (t.dataset.drive !== undefined) {
      state.drive = t.dataset.drive || null;
      drawStrip();
      drawGrid();
    } else if (t.dataset.kind) {
      state.kind = state.kind === t.dataset.kind ? null : t.dataset.kind;
      drawTags();
      drawGrid();
    } else if ("clear" in t.dataset) {
      state.kind = null;
      drawTags();
      drawGrid();
    } else if (t.dataset.file) {
      state.viewer = shown().findIndex((f) => f.id === Number(t.dataset.file));
      drawViewer();
    } else if ("close" in t.dataset) {
      state.viewer = null;
      viewer.hidden = true;
    } else if (t.dataset.go) {
      state.viewer += Number(t.dataset.go);
      drawViewer();
    } else if (t.dataset.pick) {
      state.viewer = Number(t.dataset.pick);
      drawViewer();
    }
  }

  function onKey(e) {
    if (state.viewer === null) return;
    const count = shown().length;
    if (e.key === "Escape") {
      state.viewer = null;
      viewer.hidden = true;
    } else if (e.key === "ArrowLeft" && state.viewer > 0) {
      state.viewer--;
      drawViewer();
    } else if (e.key === "ArrowRight" && state.viewer < count - 1) {
      state.viewer++;
      drawViewer();
    }
  }

  const onInput = () => {
    state.text = input.value.trim().replace(/^\./, "");
    drawGrid();
  };

  root.addEventListener("click", onClick);
  input.addEventListener("input", onInput);
  window.addEventListener("keydown", onKey);
  const resize = new ResizeObserver(drawGrid);
  resize.observe(grid);
  drawStrip();
  drawTags();
  drawGrid();

  return () => {
    resize.disconnect();
    window.removeEventListener("keydown", onKey);
    root.removeEventListener("click", onClick);
    root.innerHTML = "";
    root.classList.remove("rd");
  };
}
