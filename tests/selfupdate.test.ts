import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const calls: { cmd: string; args: unknown }[] = [];
(globalThis as any).window = {
  __TAURI_INTERNALS__: {
    invoke: async (cmd: string, args: unknown) => {
      calls.push({ cmd, args });
      return cmd === "self_update_get_settings" ? { channel: "prerelease", autoCheck: true } : null;
    },
    transformCallback: () => 1,
  },
};

const selfupdate = await import("../src/selfupdate.ts");

test("each wrapper calls its Rust command with the argument names Rust expects", async () => {
  calls.length = 0;
  const release = { assetName: "SealSuite-Setup.exe" } as any;
  const build = { runId: 1 } as any;
  await selfupdate.currentBuild();
  assert.equal(await selfupdate.getChannel(), "prerelease");
  await selfupdate.setChannel("alpha");
  await selfupdate.setAutoCheck(false);
  await selfupdate.checkForUpdate();
  await selfupdate.checkForUpdate("stable");
  await selfupdate.availableUpdate();
  await selfupdate.listReleases("prerelease", true);
  await selfupdate.listAlphaBuilds();
  await selfupdate.installRelease(release);
  await selfupdate.installAlphaBuild(build);
  assert.deepEqual(calls, [
    { cmd: "self_update_build_info", args: {} },
    { cmd: "self_update_get_settings", args: {} },
    { cmd: "self_update_set_channel", args: { channel: "alpha" } },
    { cmd: "self_update_set_auto_check", args: { enabled: false } },
    { cmd: "self_update_check", args: { channel: null } },
    { cmd: "self_update_check", args: { channel: "stable" } },
    { cmd: "self_update_available", args: {} },
    { cmd: "self_update_list_releases", args: { channel: "prerelease", refresh: true } },
    { cmd: "self_update_list_alpha_builds", args: { refresh: false } },
    { cmd: "self_update_install_release", args: { release } },
    { cmd: "self_update_install_alpha", args: { build } },
  ]);
});

test("every command the TS module calls is registered in lib.rs", () => {
  const ts = readFileSync(new URL("../src/selfupdate.ts", import.meta.url), "utf8");
  const lib = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const commands = [...ts.matchAll(/invoke\("(\w+)"/g)].map((m) => m[1]);
  assert.ok(commands.length >= 10);
  for (const command of commands) assert.match(lib, new RegExp(`selfupdate::${command}\\b`), command);
});

test("channel labels show prerelease as Beta", () => {
  assert.deepEqual(selfupdate.CHANNEL_LABELS, { stable: "Stable", prerelease: "Beta", alpha: "Alpha" });
});
