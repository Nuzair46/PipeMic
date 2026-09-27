import test from "node:test";
import assert from "node:assert/strict";
import fixture from "./fixtures/ipc.json";
import type { AppConfig, AppSettings, ControlUpdate, RouteStatus } from "../src/lib/types";
import { cloneAppConfig, controlsFromConfig, defaultConfig, neutralTone, normalizeTone, settingsFromConfig, stoppedStatus } from "../src/lib/config";

test("the Rust IPC fixture matches TypeScript config, controls, settings and status", () => {
  const config: AppConfig = fixture.config;
  const settings: AppSettings = fixture.settings;
  const controls: ControlUpdate = fixture.controls;
  const status: RouteStatus = { ...fixture.status, state: "stopped" };
  assert.deepEqual(controlsFromConfig(config), controls);
  assert.deepEqual(settingsFromConfig(config), settings);
  assert.deepEqual(stoppedStatus, status);
  assert.equal(defaultConfig.helloKittyMode, false);
});

test("old configs acquire neutral tone and cloned tone settings are independent", () => {
  const old = JSON.parse(JSON.stringify(fixture.config));
  delete old.micSources[0].tone;
  delete old.appSources[0].tone;
  const upgraded = cloneAppConfig(old);
  assert.deepEqual(upgraded.micSources[0].tone, neutralTone);
  assert.deepEqual(upgraded.appSources[0].tone, neutralTone);
  const copy = cloneAppConfig(upgraded);
  copy.micSources[0].tone.x = 1;
  assert.equal(upgraded.micSources[0].tone.x, 0);
  assert.deepEqual(normalizeTone({ x: Infinity, y: -9 }), { x: 0, y: -1, bypassed: false });
});
