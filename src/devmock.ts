// Development only: backend.ts loads this when the page runs in a plain browser, never in a build.
import type { Backend, Progress } from "./backend";
import type { AppView } from "./suite";

const params = new URLSearchParams(location.search);
const apps: AppView[] = [
  { id: "finfetcher", name: "FinFetcher", role: "Downloading", repo: "mkiera/FinFetcher", installed: { version: "1.2.9", channel: "prerelease" }, offer: null, error: null },
  { id: "flipperclipper", name: "FlipperClipper", role: "Clipping", repo: "mkiera/FlipperClipper", installed: { version: "1.2.0", channel: "stable" }, offer: null, error: null },
  { id: "blubberbound", name: "BlubberBound", role: "Compressing", repo: "mkiera/BlubberBound", installed: null, offer: null, error: null },
];
const offers: Record<string, string> = { finfetcher: "1.3.0-beta.2", blubberbound: "1.1.1" };
let progress: (p: Progress) => void = () => {};
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const copy = () => apps.map((a) => ({ ...a }));

export const mock: Backend = {
  version: async () => "0.1.0",
  listApps: async () => copy(),
  async checkUpdates() {
    await wait(Number(params.get("delay") ?? 400));
    for (const app of apps) {
      app.offer = offers[app.id] ? { version: offers[app.id] } : null;
      app.error = params.has("offline") ? "Could not reach GitHub." : null;
      if (params.has("offline")) app.offer = null;
    }
    return copy();
  },
  async installApp(id) {
    const app = apps.find((a) => a.id === id)!;
    for (let p = 0; p < 100; p += 20) {
      progress({ id, phase: "download", percent: p });
      await wait(200);
    }
    progress({ id, phase: "install", percent: 100 });
    await wait(500);
    if (params.get("fail") === id) throw "The installer stopped with code 1.";
    app.installed = { version: app.offer!.version, channel: app.installed?.channel ?? "stable" };
    app.offer = null;
    delete offers[id];
    return { ...app };
  },
  async openApp(id) {
    console.log("[mock] open", id);
  },
  async openRepo(id) {
    console.log("[mock] github", id);
  },
  onProgress(handler) {
    progress = handler;
  },
};
