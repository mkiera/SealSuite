import assert from "node:assert/strict";
import test from "node:test";
import { type AppView, checkedText, metaOf, pendingUpdates, statusOf } from "../src/suite.ts";

const base = { role: "Downloading", repo: "mkiera/FinFetcher", error: null };
const beta: AppView = { ...base, id: "finfetcher", name: "FinFetcher", installed: { version: "1.2.9", channel: "prerelease" }, offer: { version: "1.3.0-beta.2" } };
const current: AppView = { ...base, id: "flipperclipper", name: "FlipperClipper", installed: { version: "1.2.0", channel: "stable" }, offer: null };
const missing: AppView = { ...base, id: "blubberbound", name: "BlubberBound", installed: null, offer: { version: "1.1.1" } };

test("only installed apps with an offer count as updates", () => {
  assert.deepEqual(pendingUpdates([beta, current, missing]).map((a) => a.id), ["finfetcher"]);
});

test("each tile states its status", () => {
  assert.deepEqual(statusOf(beta, true), { kind: "update", text: "Update to 1.3.0-beta.2" });
  assert.deepEqual(statusOf(current, true), { kind: "ok", text: "Up to date" });
  assert.deepEqual(statusOf(current, false), { kind: "unknown", text: "Checking…" });
  assert.deepEqual(statusOf({ ...current, error: "Could not reach GitHub." }, true), { kind: "unknown", text: "Could not check" });
  assert.deepEqual(statusOf(missing, true), { kind: "missing", text: "Not installed" });
});

test("the version line names the channel the way the apps do", () => {
  assert.equal(metaOf(beta), "v1.2.9 · Beta");
  assert.equal(metaOf(current), "v1.2.0 · Stable");
  assert.equal(metaOf(missing), "v1.1.1 available");
  assert.equal(metaOf({ ...missing, offer: null }), "");
});

test("the last check reads in minutes, then hours", () => {
  const now = 10 * 3_600_000;
  assert.equal(checkedText(null, now), "Checking for updates…");
  assert.equal(checkedText(now - 20_000, now), "Checked just now");
  assert.equal(checkedText(now - 5 * 60_000, now), "Checked 5 min ago");
  assert.equal(checkedText(now - 3 * 3_600_000, now), "Checked 3 h ago");
});
