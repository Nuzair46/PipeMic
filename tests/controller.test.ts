import test from "node:test";
import assert from "node:assert/strict";
import { MixerController, type MixerApi } from "../src/lib/controller";
import { cloneAppConfig, defaultConfig, neutralTone, settingsFromConfig, stoppedStatus } from "../src/lib/config";
import { errorMessage } from "../src/lib/errors";
import type { ControlUpdate, RouteStatus } from "../src/lib/types";

const running: RouteStatus = { ...stoppedStatus, state: "running" };
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

function setup(overrides: Partial<MixerApi> = {}) {
  const config = { ...cloneAppConfig(defaultConfig), outputDeviceId: "out", micSources: [{ id: "mic:one", deviceId: "one", gain: 0.7, muted: false, tone: { ...neutralTone } }] };
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

test("tone drags coalesce behind IPC and send the final position with concurrent gain and mute", async () => {
  const delayed = deferred<RouteStatus>();
  const sent: ControlUpdate[] = [];
  const { controller } = setup({ updateControls: async controls => {
    sent.push(controls);
    return sent.length === 1 ? delayed.promise : running;
  } });
  await controller.refresh();
  const changeSource = (patch: Partial<ReturnType<typeof controller.getSnapshot>["config"]["micSources"][number]>) =>
    controller.changeControls({ micSources: controller.getSnapshot().config.micSources.map(source => ({ ...source, ...patch })) });
  const first = changeSource({ tone: { x: 0.1, y: 0, bypassed: false } });
  await tick();
  const edits = [];
  for (let i = 2; i <= 100; i++) edits.push(changeSource({ tone: { x: i / 100, y: -0.6, bypassed: false } }));
  edits.push(changeSource({ gain: 1.4, muted: true }));
  assert.equal(sent.length, 1);
  delayed.resolve(running);
  await Promise.all([first, ...edits]);
  assert.equal(sent.length, 2);
  assert.deepEqual(sent[1].micSources[0], { id: "mic:one", gain: 1.4, muted: true, tone: { x: 1, y: -0.6, bypassed: false } });
});

test("removing a source during a pending tone update cannot restore it", async () => {
  const delayed = deferred<RouteStatus>();
  const sent: ControlUpdate[] = [];
  const { controller, saved } = setup({ updateControls: async controls => {
    sent.push(controls);
    return sent.length === 1 ? delayed.promise : stoppedStatus;
  } });
  await controller.refresh();
  const first = controller.changeControls({ masterGain: 0.5 });
  await tick();
  const tone = controller.changeControls({ micSources: controller.getSnapshot().config.micSources.map(source => ({ ...source, tone: { x: 1, y: 1, bypassed: false } })) });
  const remove = controller.changeTopology({ micSources: [] });
  delayed.resolve(stoppedStatus);
  await Promise.all([first, tone, remove]);
  assert.deepEqual(sent.at(-1)?.micSources, []);
  assert.deepEqual(saved.at(-1)?.micSources, []);
  assert.deepEqual(controller.getSnapshot().config.micSources, []);
});

test("a queued control response cannot replace a later Stop intent", async () => {
  const settingsResult = deferred<ReturnType<typeof cloneAppConfig>>();
  const stopResult = deferred<RouteStatus>();
  const { controller, config } = setup({
    applyAppSettings: () => settingsResult.promise,
    updateControls: async () => running,
    stopRouting: () => stopResult.promise,
  });
  await controller.refresh();
  const settings = controller.saveSettings(settingsFromConfig(config));
  await tick();
  const controls = controller.changeControls({ masterGain: 0.6 });
  const stop = controller.stop();
  settingsResult.resolve(config);
  await controls;
  assert.equal(controller.getSnapshot().status.state, "stopped");
  stopResult.resolve(stoppedStatus);
  await Promise.all([settings, stop]);
});

test("saved tone and bypass survive a new controller without capture restarts", async () => {
  const context = setup({
    startRouting: async () => { throw new Error("Tone must not restart capture"); },
    stopRouting: async () => { throw new Error("Tone must not stop capture"); },
  });
  let persisted = cloneAppConfig(context.config);
  context.api.loadConfig = async () => cloneAppConfig(persisted);
  context.api.updateControls = async controls => {
    persisted = { ...persisted, micSources: persisted.micSources.map(source => ({ ...source, ...controls.micSources.find(control => control.id === source.id) })) };
    return running;
  };
  await context.controller.refresh();
  const tone = { x: -0.75, y: 0.4, bypassed: true };
  await context.controller.changeControls({ micSources: persisted.micSources.map(source => ({ ...source, tone })) });
  const restarted = new MixerController(context.api, () => assert.fail("Unexpected error"));
  await restarted.refresh();
  assert.deepEqual(restarted.getSnapshot().config.micSources[0].tone, tone);
  assert.equal(context.controller.getSnapshot().status.state, "running");
  assert.deepEqual(context.errors, []);
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
