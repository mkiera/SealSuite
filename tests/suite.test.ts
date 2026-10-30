import assert from "node:assert/strict";
import test from "node:test";
import { SUITE_APPS } from "../src/suite.ts";

test("each suite app has a unique name and a mkiera repository", () => {
  const names = SUITE_APPS.map((app) => app.name);
  assert.equal(new Set(names).size, names.length);
  for (const app of SUITE_APPS) {
    assert.equal(app.repository, `mkiera/${app.name}`);
  }
});
