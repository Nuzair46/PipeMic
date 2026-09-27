import test from "node:test";
import assert from "node:assert/strict";
import fixture from "./fixtures/ipc.json";
import type { AppConfig, AppSettings, ControlUpdate, RouteStatus } from "../src/lib/types";
import { controlsFromConfig, defaultConfig, settingsFromConfig, stoppedStatus } from "../src/lib/config";

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
