// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MediaKind = "image" | "video" | "raw" | "design";
export type Sort = "newest" | "oldest" | "name" | "largest";

export interface SetupInfo {
  catalogDir: string | null;
  defaultDir: string;
  defaultFreeBytes: number | null;
}

export interface Volume {
  id: number | null;
  label: string;
  mount: string | null;
  online: boolean;
  fsType: string;
  totalBytes: number;
  freeBytes: number;
  rotational: boolean;
  fileCount: number;
  lastScanAt: number | null;
  scanning: boolean;
  roots: string[];
}

export interface ScanStats {
  files_seen: number;
  added: number;
  changed: number;
  removed: number;
  skipped: number;
}

export interface ScanProgress {
  volumeId: number;
  stats: ScanStats;
  currentDir: string;
}

export interface ScanFinished {
  volumeId: number;
  status: "done" | "cancelled" | null;
  stats: ScanStats | null;
  error: string | null;
}

export type Orientation = "landscape" | "portrait" | "square";

export interface FileQuery {
  text?: string;
  volumeId?: number | null;
  kind?: MediaKind | null;
  includeMissing?: boolean;
  sort?: Sort;
  minSide?: number | null;
  orientation?: Orientation | null;
  dateFrom?: number | null;
  dateTo?: number | null;
  camera?: string | null;
  minDurationMs?: number | null;
  maxDurationMs?: number | null;
  /** Volume-relative folder ('' = drive root); needs volumeId. */
  folder?: string | null;
  /** Only files directly in `folder`, not its subfolders. */
  directOnly?: boolean;
}

export interface Folder {
  name: string;
  path: string;
  /** Media files in the folder and all its subfolders. */
  count: number;
  hasChildren: boolean;
}

export interface FileRow {
  id: number;
  volumeId: number;
  relPath: string;
  dir: string;
  name: string;
  ext: string;
  kind: MediaKind | null;
  size: number;
  mtime: number;
  missing: boolean;
  width: number | null;
  height: number | null;
  /** Date taken, or modified when unknown (unix seconds). */
  date: number;
  durationMs: number | null;
  camera: string | null;
  /** Thumbnail version; null when there is no thumbnail (yet). */
  thumb: number | null;
  absPath: string | null;
}

export interface FileDetails extends FileRow {
  takenAt: number | null;
  lens: string | null;
  iso: number | null;
  fNumber: number | null;
  exposure: string | null;
  focalMm: number | null;
  gpsLat: number | null;
  gpsLon: number | null;
  codec: string | null;
  fps: number | null;
  processed: boolean;
  processError: string | null;
}

export interface CameraCount {
  camera: string;
  count: number;
}

/** Overall progress of one drive: `done` counts every processed file, not just this session's. */
export interface JobProgress {
  volumeId: number;
  done: number;
  total: number;
  /** "queued": waiting for another drive on the same physical disk. */
  state: "running" | "queued";
}

export interface JobsStatus {
  paused: boolean;
  volumes: JobProgress[];
}

export interface Page {
  total: number;
  rows: FileRow[];
}

export const api = {
  getSetup: () => invoke<SetupInfo>("get_setup"),
  freeSpace: (path: string) => invoke<number | null>("free_space", { path }),
  setCatalogDir: (path: string) => invoke<void>("set_catalog_dir", { path }),
  listVolumes: () => invoke<Volume[]>("list_volumes"),
  addVolume: (mount: string) => invoke<number>("add_volume", { mount }),
  renameVolume: (id: number, label: string) => invoke<void>("rename_volume", { id, label }),
  removeVolume: (id: number) => invoke<void>("remove_volume", { id }),
  startScan: (id: number) => invoke<void>("start_scan", { id }),
  cancelScan: (id: number) => invoke<void>("cancel_scan", { id }),
  jobsStatus: () => invoke<JobsStatus>("jobs_status"),
  setJobsPaused: (paused: boolean) => invoke<void>("set_jobs_paused", { paused }),
  queryFiles: (query: FileQuery, offset: number, limit: number) =>
    invoke<Page>("query_files", { query, offset, limit }),
  fileDetails: (id: number) => invoke<FileDetails>("file_details", { id }),
  listFolders: (volumeId: number, parent: string, includeMissing: boolean) =>
    invoke<Folder[]>("list_folders", { volumeId, parent, includeMissing }),
  listCameras: () => invoke<CameraCount[]>("list_cameras"),
  openFile: (id: number) => invoke<void>("open_file", { id }),
  revealFile: (id: number) => invoke<void>("reveal_file", { id }),
  openInVlc: (id: number) => invoke<void>("open_in_vlc", { id }),
};

interface MediaServer {
  port: number;
  token: string;
}

let mediaServer: Promise<MediaServer | null> | null = null;

/** URL that streams a video (with seeking) from the local video server, or null if it isn't running. */
export async function videoUrl(id: number): Promise<string | null> {
  mediaServer ??= invoke<MediaServer | null>("media_server");
  const server = await mediaServer;
  return server ? `http://127.0.0.1:${server.port}/v/${id}?t=${server.token}` : null;
}

/** Thumbnail URL (served from the catalog by the Rust `media://` protocol), or null. */
export function thumbUrl(row: Pick<FileRow, "id" | "thumb">): string | null {
  return row.thumb === null ? null : `${convertFileSrc(`t${row.id}`, "media")}?v=${row.thumb}`;
}

/** Formats the browser can show directly as a full-size preview. */
const PREVIEWABLE = new Set(["jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "avif", "svg"]);

/** Formats the backend renders into a large preview (cached, so it also works offline later). */
const RENDERED_PREVIEW = new Set([
  "heic", "heif", "psd", "psb", "tga", "exr", "hdr", "jxl", "tif", "tiff", "ai", "eps", "pdf", "fig",
]);

/** URL of a large preview for the preview panel, or null if there is none. */
export function previewUrl(row: Pick<FileRow, "id" | "ext" | "absPath">): string | null {
  if (row.absPath && PREVIEWABLE.has(row.ext)) return convertFileSrc(`o${row.id}`, "media");
  if (RENDERED_PREVIEW.has(row.ext)) return convertFileSrc(`p${row.id}`, "media");
  return null;
}

export const onJobsProgress = (cb: (s: JobsStatus) => void): Promise<UnlistenFn> =>
  listen<JobsStatus>("jobs-progress", (e) => cb(e.payload));

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan-progress", (e) => cb(e.payload));

export const onScanFinished = (cb: (p: ScanFinished) => void): Promise<UnlistenFn> =>
  listen<ScanFinished>("scan-finished", (e) => cb(e.payload));
