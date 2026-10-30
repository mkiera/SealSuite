import { backend, type Backend, type Progress } from "./backend";
import blubberbound from "./icons/blubberbound.png";
import finfetcher from "./icons/finfetcher.png";
import flipperclipper from "./icons/flipperclipper.png";
import { type AppView, checkedText, metaOf, pendingUpdates, statusOf } from "./suite";

const ICONS: Record<string, string> = { finfetcher, flipperclipper, blubberbound };
const GITHUB_MARK =
  '<svg class="mark" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"/></svg>';
const UP_RIGHT = '<svg class="arrow" viewBox="0 0 12 12" aria-hidden="true"><path d="M3.5 8.5 8.5 3.5M4.5 3.5h4v4"/></svg>';

let api: Backend;
let apps: AppView[] = [];
let checkedAt: number | null = null;
let checking = false;
const busy = new Map<string, Progress>();
const errors = new Map<string, string>();
let selfUpdate: { version: string } | null = null;
let selfProgress: number | null = null;
let selfError: string | null = null;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const esc = (text: string) => text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const message = (error: unknown) => (typeof error === "string" ? error : error instanceof Error ? error.message : "Something went wrong.");

function renderStrip() {
  const strip = $("strip");
  const pending = pendingUpdates(apps);
  const names = [...pending.map((a) => `${a.name} ${a.offer!.version}`), ...(selfUpdate ? [`SealSuite ${selfUpdate.version}`] : [])];
  const checkErrors = apps.filter((a) => a.error);
  const failed = !names.length && checkedAt !== null && checkErrors.length > 0;
  strip.className = `strip${names.length ? "" : " quiet"}${failed ? " warn" : ""}`;
  const title = names.length
    ? `${names.length} update${names.length > 1 ? "s" : ""} ready`
    : checkedAt === null
      ? "Checking for updates…"
      : checkErrors.length === apps.length
        ? "Could not check for updates"
        : "All apps up to date";
  const sub = selfError ?? (names.length ? names.join(" · ") : checkErrors.length ? checkErrors[0].error! : checkedText(checkedAt, Date.now()));
  const anyBusy = busy.size > 0 || selfProgress !== null;
  const label = selfProgress !== null ? `Updating SealSuite ${Math.round(selfProgress * 100)}%` : names.length > 1 ? "Update all" : "Update";
  strip.innerHTML = `
    <div class="lead"><span class="pulse" aria-hidden="true"></span><div><strong>${title}</strong><div class="sub${selfError ? " bad" : ""}">${esc(sub)}</div></div></div>
    ${names.length ? `<div class="right"><span class="when">${checkedText(checkedAt, Date.now())}</span><button class="button primary" data-update-all ${anyBusy ? "disabled" : ""}>${label}</button></div>` : ""}`;
}

function actions(app: AppView): string {
  const progress = busy.get(app.id);
  if (progress) {
    const verb = progress.phase === "install" ? (app.installed ? "Updating…" : "Installing…") : `Downloading ${progress.percent}%`;
    return `<button class="button primary" disabled>${verb}</button>`;
  }
  if (!app.installed) return `<button class="button primary" data-install="${app.id}">Install</button>`;
  return `${app.offer ? `<button class="button primary" data-install="${app.id}">Update</button>` : ""}<button class="button" data-open="${app.id}">Open</button>`;
}

function renderGrid() {
  $("grid").innerHTML = apps
    .map((app) => {
      const status = statusOf(app, checkedAt !== null);
      const progress = busy.get(app.id);
      const error = errors.get(app.id);
      return `
      <article class="panel tile" aria-labelledby="name-${app.id}">
        <button class="gh" data-github="${app.id}" title="github.com/${esc(app.repo)}" aria-label="${esc(app.name)} on GitHub (opens in your browser)">${GITHUB_MARK}GitHub${UP_RIGHT}</button>
        <img class="icon" src="${ICONS[app.id]}" alt="">
        <h2 id="name-${app.id}">${esc(app.name)}</h2>
        <div class="role">${esc(app.role)}</div>
        <div class="meta">${esc(metaOf(app))}</div>
        <div class="status ${status.kind}">${status.text}</div>
        ${progress ? `<div class="bar" role="progressbar" aria-valuenow="${progress.percent}" aria-label="${esc(app.name)} progress"><i style="width:${progress.percent}%"></i></div>` : ""}
        ${error ? `<p class="tile-error" role="alert">${esc(error)}</p>` : ""}
        <div class="acts">${actions(app)}</div>
      </article>`;
    })
    .join("");
}

function render() {
  renderStrip();
  renderGrid();
  $<HTMLButtonElement>("check").disabled = checking;
  $("check").textContent = checking ? "Checking…" : "Check now";
}

function replace(updated: AppView) {
  apps = apps.map((a) => (a.id === updated.id ? updated : a));
}

async function check() {
  if (checking) return;
  checking = true;
  render();
  try {
    const [fresh, self] = await Promise.all([api.checkUpdates(), api.checkSelf().catch(() => null)]);
    selfUpdate = self;
    // An app mid-install keeps its tile; its own result replaces it when the installer finishes.
    apps = fresh.map((a) => (busy.has(a.id) ? apps.find((b) => b.id === a.id) ?? a : a));
    checkedAt = Date.now();
  } finally {
    checking = false;
    render();
  }
}

async function install(id: string) {
  if (busy.has(id)) return;
  errors.delete(id);
  busy.set(id, { id, phase: "download", percent: 0 });
  render();
  try {
    replace(await api.installApp(id));
  } catch (error) {
    errors.set(id, message(error));
  } finally {
    busy.delete(id);
    render();
  }
}

async function updateSelf() {
  selfError = null;
  selfProgress = 0;
  render();
  try {
    await api.installSelf((fraction) => {
      selfProgress = fraction;
      renderStrip();
    });
  } catch (error) {
    selfError = message(error);
  } finally {
    selfProgress = null;
    render();
  }
}

async function run(id: string, action: (id: string) => Promise<void>) {
  errors.delete(id);
  try {
    await action(id);
  } catch (error) {
    errors.set(id, message(error));
  }
  render();
}

document.addEventListener("click", (event) => {
  const target = (event.target as HTMLElement).closest<HTMLElement>("[data-install], [data-open], [data-github], [data-update-all]");
  if (!target) return;
  const data = target.dataset;
  if (data.install) void install(data.install);
  else if (data.open) void run(data.open, (id) => api.openApp(id));
  else if (data.github) void run(data.github, (id) => api.openRepo(id));
  else if ("updateAll" in data) {
    void (async () => {
      for (const app of pendingUpdates(apps)) await install(app.id);
      // SealSuite goes last: its installer closes this window.
      if (selfUpdate && !pendingUpdates(apps).length) await updateSelf();
    })();
  }
});

async function start() {
  api = await backend();
  api.onProgress((progress) => {
    if (!busy.has(progress.id)) return;
    busy.set(progress.id, progress);
    render();
  });
  $("check").addEventListener("click", () => void check());
  setInterval(renderStrip, 30_000);
  const [version, list] = await Promise.all([api.version(), api.listApps()]);
  $("version").textContent = `v${version} · Made by Kiera`;
  apps = list;
  render();
  await check();
}

void start();
