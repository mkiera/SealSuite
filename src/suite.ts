export type Channel = "stable" | "prerelease" | "alpha";

export interface AppView {
  id: string;
  name: string;
  role: string;
  repo: string;
  installed: { version: string; channel: Channel } | null;
  /** For an installed app the update on its channel; for a missing app the latest stable release. */
  offer: { version: string } | null;
  error: string | null;
}

export type Status =
  | { kind: "update"; text: string }
  | { kind: "ok"; text: string }
  | { kind: "missing"; text: string }
  | { kind: "unknown"; text: string };

export const CHANNEL_NAMES: Record<Channel, string> = { stable: "Stable", prerelease: "Beta", alpha: "Alpha" };

export function hasUpdate(app: AppView): boolean {
  return app.installed !== null && app.offer !== null;
}

export function pendingUpdates(apps: readonly AppView[]): AppView[] {
  return apps.filter(hasUpdate);
}

/** `checked` is false until the first update check has answered. */
export function statusOf(app: AppView, checked: boolean): Status {
  if (!app.installed) return { kind: "missing", text: "Not installed" };
  if (app.offer) return { kind: "update", text: `Update to ${app.offer.version}` };
  if (app.error) return { kind: "unknown", text: "Could not check" };
  if (!checked) return { kind: "unknown", text: "Checking…" };
  if (app.installed.channel === "alpha") return { kind: "unknown", text: "Alpha build" };
  return { kind: "ok", text: "Up to date" };
}

export function metaOf(app: AppView): string {
  if (app.installed) return `v${app.installed.version} · ${CHANNEL_NAMES[app.installed.channel]}`;
  return app.offer ? `v${app.offer.version} available` : "";
}

export function checkedText(checkedAt: number | null, now: number): string {
  if (checkedAt === null) return "Checking for updates…";
  const minutes = Math.floor((now - checkedAt) / 60_000);
  if (minutes < 1) return "Checked just now";
  if (minutes < 60) return `Checked ${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return `Checked ${hours} h ago`;
}
