// SealSuite's own updates. The Rust side (src-tauri/src/selfupdate.rs) does the network, the
// version choice and the install; this module only names its commands and types.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Stored values. `prerelease` is shown to people as Beta. */
export type UpdateChannel = "stable" | "prerelease" | "alpha";

export const CHANNEL_LABELS: Record<UpdateChannel, string> = {
  stable: "Stable",
  prerelease: "Beta",
  alpha: "Alpha",
};

/** What the button on a release row says, compared with the running version. */
export type UpdateAction = "update" | "downgrade" | "reinstall" | "install";

export interface BuildInfo {
  version: string;
  /** Set only on CI builds. */
  sha: string | null;
  branch: string | null;
  runId: number | null;
}

export interface SelfUpdateSettings {
  channel: UpdateChannel;
  autoCheck: boolean;
}

export interface ReleaseInfo {
  version: string;
  tagName: string;
  publishedAt: string | null;
  prerelease: boolean;
  releaseUrl: string;
  downloadUrl: string;
  assetName: string;
  sizeBytes: number;
  /** Markdown, cut before the installation notes. */
  notes: string;
  action: UpdateAction;
}

export interface AlphaBuild {
  runId: number;
  runNumber: number;
  branch: string;
  sha: string;
  createdAt: string | null;
  runUrl: string;
  downloadUrl: string;
  /** The running build came from this run's commit. */
  isCurrent: boolean;
}

export const PROGRESS_EVENT = "self-update-progress";

export function currentBuild(): Promise<BuildInfo> {
  return invoke("self_update_build_info");
}

export function getSettings(): Promise<SelfUpdateSettings> {
  return invoke("self_update_get_settings");
}

export async function getChannel(): Promise<UpdateChannel> {
  return (await getSettings()).channel;
}

/** Saves at once and drops any offer from the old channel. */
export function setChannel(channel: UpdateChannel): Promise<void> {
  return invoke("self_update_set_channel", { channel });
}

/** Controls only the checks the app starts by itself. */
export function setAutoCheck(enabled: boolean): Promise<void> {
  return invoke("self_update_set_auto_check", { enabled });
}

/** Asks GitHub for the newest release above the running one. Null when there is none, and
 *  always null on Alpha, whose builds are installed by hand. Rejects with a readable message. */
export function checkForUpdate(channel?: UpdateChannel): Promise<ReleaseInfo | null> {
  return invoke("self_update_check", { channel: channel ?? null });
}

/** The last check's offer for the saved channel, with no network request. */
export function availableUpdate(): Promise<ReleaseInfo | null> {
  return invoke("self_update_available");
}

/** Every installable release on Stable or Beta, newest first, older ones included. */
export function listReleases(channel?: UpdateChannel, refresh = false): Promise<ReleaseInfo[]> {
  return invoke("self_update_list_releases", { channel: channel ?? null, refresh });
}

/** The newest Build Test run of each live branch, at most eight. */
export function listAlphaBuilds(refresh = false): Promise<AlphaBuild[]> {
  return invoke("self_update_list_alpha_builds", { refresh });
}

/** Downloads and starts the installer. On success SealSuite exits and the installer relaunches it. */
export function installRelease(release: ReleaseInfo): Promise<void> {
  return invoke("self_update_install_release", { release });
}

export function installAlphaBuild(build: AlphaBuild): Promise<void> {
  return invoke("self_update_install_alpha", { build });
}

/** Download progress from 0 to 1 while an install runs. */
export function onInstallProgress(callback: (fraction: number) => void): Promise<UnlistenFn> {
  return listen<number>(PROGRESS_EVENT, (event) => callback(event.payload));
}
