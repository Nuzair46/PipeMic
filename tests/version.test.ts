import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

test("release bumps include the lockfile and detect a stale locked package version", () => {
  const directory = mkdtempSync(join(tmpdir(), "pipemic-version-"));
  try {
    mkdirSync(join(directory, "src-tauri"));
    for (const file of ["package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json"]) copyFileSync(resolve(file), join(directory, file));
    const bump = (...args: string[]) => spawnSync(process.execPath, [resolve("tools/version-bump.mjs"), ...args], { cwd: directory, encoding: "utf8" });
    { const result = bump("--check"); assert.equal(result.status, 0, result.stderr + result.stdout); }
    const current = JSON.parse(readFileSync(join(directory, "package.json"), "utf8")).version;
    { const result = bump("patch"); assert.equal(result.status, 0, result.stderr + result.stdout); }
    { const result = bump("--check"); assert.equal(result.status, 0, result.stderr + result.stdout); }
    const lockPath = join(directory, "src-tauri/Cargo.lock");
    writeFileSync(lockPath, readFileSync(lockPath, "utf8").replace(/(name = "pipemic"\r?\nversion = ")[^"]+/, `$1${current}`));
    const result = bump("--check");
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Cargo\.lock/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
