import test from "node:test";
import assert from "node:assert/strict";
import { MixerController, type MixerApi } from "../src/lib/controller";
import { cloneAppConfig, defaultConfig, settingsFromConfig, stoppedStatus } from "../src/lib/config";
import { errorMessage } from "../src/lib/errors";
import type { RouteStatus } from "../src/lib/types";

const running: RouteStatus = { ...stoppedStatus, state: "running" };
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

function setup(overrides: Partial<MixerApi> = {}) {
  const config = { ...cloneAppConfig(defaultConfig), outputDeviceId: "out", micSources: [{ id: "mic:one", deviceId: "one", gain: 0.7, muted: false }] };
  const saved: typeof config[] = [];
  const errors: string[] = [];
  const api: MixerApi = {
    loadConfig: async () => config,
    saveConfig: async next => { saved.push(next as typeof config); return next; },
    applyAppSettings: async settings => ({ ...config, ...settings }),
    listCaptureDevices: async () => [],
    listRenderDevices: async () => [{ id: "out", name: "Output", flow: "render", isDefault: true, isVirtualCableLike: true }],
    listSessions: async () => [], startRouting: async () => running, stopRouting: async () => stoppedStatus,
    updateControls: async () => running, getStatus: async () => stoppedStatus,
    ...overrides,
  };
  return { config, saved, errors, api, controller: new MixerController(api, (_title, message) => errors.push(message)) };
}

test("a failed discovery query never discards saved config", async () => {
  const context = setup({ listSessions: async () => { throw { message: "Enumeration failed" }; } });
  await context.controller.refresh();
  assert.equal(context.controller.getSnapshot().config.micSources.length, 1);
  assert.equal(context.controller.getSnapshot().ready, true);
  context.api.listSessions = async () => [];
  await context.controller.refresh();
  assert.deepEqual(context.saved, []);
  assert.deepEqual(context.errors, ["Enumeration failed"]);
});

test("a failed config load disables all writes, including automatic defaults", async () => {
  const { controller, saved } = setup({ loadConfig: async () => { throw new Error("Corrupt config"); } });
  await controller.refresh();
  await controller.changeTopology({ outputDeviceId: "another" });
  await controller.changeControls({ masterGain: 0 });
  assert.equal(controller.getSnapshot().ready, false);
  assert.deepEqual(saved, []);
});

test("Stop supersedes a pending topology restart and preserves the chosen topology", async () => {
  const delayed = deferred<RouteStatus>();
  const calls: string[] = [];
  const { controller, saved, api } = setup({ startRouting: async () => { calls.push("start"); return delayed.promise; }, stopRouting: async () => { calls.push("stop"); return stoppedStatus; } });
  await controller.refresh();
  const start = controller.start();
  await tick();
  const topology = controller.changeTopology({ outputDeviceId: "new" });
  const stop = controller.stop();
  delayed.resolve(running);
  await Promise.all([start, topology, stop]);
  assert.deepEqual(calls, ["start", "stop"]);
  assert.equal(controller.getSnapshot().status.state, "stopped");
  assert.equal(saved.at(-1)?.outputDeviceId, "new");
  api.getStatus = async () => stoppedStatus;
});

test("a settings response cannot overwrite a mute changed during the save", async () => {
  const saved = deferred<ReturnType<typeof cloneAppConfig>>();
  const { controller, config } = setup({ applyAppSettings: () => saved.promise });
  await controller.refresh();
  const settings = controller.saveSettings({ ...settingsFromConfig(config), startWithWindows: false });
  await tick();
  const mute = controller.changeControls({ micSources: config.micSources.map(source => ({ ...source, muted: true })) });
  saved.resolve(config);
  await Promise.all([settings, mute]);
  assert.equal(controller.getSnapshot().config.micSources[0].muted, true);
  assert.equal(controller.getSnapshot().config.startWithWindows, false);
});

test("a topology change after Stop cannot cancel the queued Stop", async () => {
  const delayed = deferred<RouteStatus>();
  const calls: string[] = [];
  const { controller, saved } = setup({
    startRouting: async () => { calls.push("start"); return delayed.promise; },
    stopRouting: async () => { calls.push("stop"); return stoppedStatus; },
  });
  await controller.refresh();
  const start = controller.start();
  await tick();
  const stop = controller.stop();
  const topology = controller.changeTopology({ outputDeviceId: "new" });
  delayed.resolve(running);
  await Promise.all([start, stop, topology]);
  assert.deepEqual(calls, ["start", "stop"]);
  assert.equal(saved.at(-1)?.outputDeviceId, "new");
});

test("a failed mutation does not block the next control or Stop", async () => {
  let controls = 0;
  const { controller, errors } = setup({ updateControls: async () => {
    if (++controls === 1) throw { message: "Temporary failure" };
    return running;
  } });
  await controller.refresh();
  await controller.changeControls({ masterGain: 0.4 });
  await controller.changeControls({ masterGain: 0.8 });
  await controller.stop();
  assert.equal(controls, 2);
  assert.equal(controller.getSnapshot().status.state, "stopped");
  assert.deepEqual(errors, ["Temporary failure"]);
});

test("polling is single-flight and a stale read cannot replace a newer Stop", async () => {
  const result = deferred<RouteStatus>();
  let reads = 0;
  const { controller } = setup({ getStatus: () => { reads++; return result.promise; } });
  await controller.refresh();
  const first = controller.pollStatus();
  const second = controller.pollStatus();
  await controller.stop();
  result.resolve(running);
  await Promise.all([first, second]);
  assert.equal(reads, 1);
  assert.equal(controller.getSnapshot().status.state, "stopped");
});

test("native structured errors retain their message", () => {
  assert.equal(errorMessage({ message: "Disk full" }), "Disk full");
  assert.equal(errorMessage(new Error("Denied")), "Denied");
});

test("saved theme choices survive a new controller without restarting audio", async () => {
  const context = setup({
    getStatus: async () => running,
    startRouting: async () => { throw new Error("Settings must not restart audio"); },
    stopRouting: async () => { throw new Error("Settings must not stop audio"); },
  });
  let persisted = cloneAppConfig(context.config);
  context.api.loadConfig = async () => cloneAppConfig(persisted);
  context.api.applyAppSettings = async settings => (persisted = { ...persisted, ...settings });
  await context.controller.refresh();
  await context.controller.pollStatus();
  for (const helloKittyMode of [true, false]) {
    await context.controller.saveSettings({ ...settingsFromConfig(persisted), helloKittyMode });
    assert.equal(context.controller.getSnapshot().status.state, "running");
    const rebooted = new MixerController(context.api, (_title, message) => context.errors.push(message));
    await rebooted.refresh();
    assert.equal(rebooted.getSnapshot().config.helloKittyMode, helloKittyMode);
    assert.deepEqual(rebooted.getSnapshot().config.micSources, context.config.micSources);
  }
  assert.deepEqual(context.errors, []);
});
