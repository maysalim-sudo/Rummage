// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MediaKind = "image" | "video" | "raw";
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

export interface FileQuery {
  text?: string;
  volumeId?: number | null;
  kind?: MediaKind | null;
  includeMissing?: boolean;
  sort?: Sort;
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
  absPath: string | null;
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
  removeVolume: (id: number) => invoke<void>("remove_volume", { id }),
  startScan: (id: number) => invoke<void>("start_scan", { id }),
  cancelScan: (id: number) => invoke<void>("cancel_scan", { id }),
  queryFiles: (query: FileQuery, offset: number, limit: number) =>
    invoke<Page>("query_files", { query, offset, limit }),
  openFile: (id: number) => invoke<void>("open_file", { id }),
  revealFile: (id: number) => invoke<void>("reveal_file", { id }),
};

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan-progress", (e) => cb(e.payload));

export const onScanFinished = (cb: (p: ScanFinished) => void): Promise<UnlistenFn> =>
  listen<ScanFinished>("scan-finished", (e) => cb(e.payload));
