import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const iss = read("installer.iss");
const profile = JSON.parse(read("app_profile.json"));
const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));

test("the installer keys on app_profile.json's package_id", () => {
  // Inno doubles the opening brace of a literal GUID.
  assert.match(iss, new RegExp(`^AppId=\\{${profile.package_id.replace(/[{}]/g, (c: string) => `\\${c}`)}$`, "m"));
});

test("the installer is per-user and names its files from the profile", () => {
  assert.match(iss, /^PrivilegesRequired=lowest$/m);
  assert.match(iss, /^DefaultDirName=\{localappdata\}\\Programs\\SealSuite$/m);
  assert.match(iss, new RegExp(`^OutputBaseFilename=${profile.installer_asset.replace(/\.exe$/, "")}$`, "m"));
  assert.match(iss, new RegExp(`^#define AppExeName "${profile.executable}"$`, "m"));
});

test("uninstalling deletes only SealSuite's own data folders", () => {
  const section = iss.split("[UninstallDelete]")[1] ?? "";
  const names = [...section.matchAll(/Name: "([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(names, [`{localappdata}\\${tauri.identifier}`, `{userappdata}\\${tauri.identifier}`]);
  assert.doesNotMatch(iss, /\[UninstallRun\]|\[Registry\]/);
});

test("both workflows publish the installer under the profile's names", () => {
  for (const file of ["build-test.yml", "build-release.yml"]) {
    const workflow = read(`.github/workflows/${file}`);
    assert.match(workflow, /runs-on: windows-latest/);
    assert.ok(workflow.includes(`dist_installer/${profile.installer_asset}`), file);
  }
  assert.match(read(".github/workflows/build-test.yml"), new RegExp(`name: ${profile.alpha_artifact}$`, "m"));
});
