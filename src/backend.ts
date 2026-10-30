import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppView } from "./suite";

export interface Progress {
  id: string;
  phase: "download" | "install";
  percent: number;
}

export interface Backend {
  version(): Promise<string>;
  listApps(): Promise<AppView[]>;
  checkUpdates(): Promise<AppView[]>;
  installApp(id: string): Promise<AppView>;
  openApp(id: string): Promise<void>;
  openRepo(id: string): Promise<void>;
  onProgress(handler: (progress: Progress) => void): void;
}

const tauri: Backend = {
  version: () => getVersion(),
  listApps: () => invoke("list_apps"),
  checkUpdates: () => invoke("check_updates"),
  installApp: (id) => invoke("install_app", { id }),
  openApp: (id) => invoke("open_app", { id }),
  openRepo: (id) => invoke("open_repo", { id }),
  onProgress: (handler) => void listen<Progress>("install-progress", (event) => handler(event.payload)),
};

export async function backend(): Promise<Backend> {
  // A plain browser (vite dev without Tauri) gets stub data so the window can be checked headless.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    return (await import("./devmock")).mock;
  }
  return tauri;
}
