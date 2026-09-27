import type { AppConfig, AppSettings, AudioDevice, ControlUpdate, RouteStatus } from "./types";
import { outputDevicesForPicker, preferredOutputDevice } from "./devices";

export const stoppedStatus: RouteStatus = {
  state: "stopped",
  message: "Routing stopped",
  meters: { micPeaks: {}, appPeaks: {}, outputPeak: 0 },
  warnings: [],
};

export const defaultConfig: AppConfig = {
  micSources: [],
  appSources: [],
  outputDeviceId: null,
  masterGain: 1,
  bufferFrames: 960,
  downmixToMono: true,
  shortcuts: {
    micMute: "Ctrl+Alt+M",
    appMute: "Ctrl+Alt+A",
    routing: "Ctrl+Alt+S",
  },
  startWithWindows: true,
  minimizeToTray: true,
};

export function controlsFromConfig(config: AppConfig): ControlUpdate {
  return {
    micSources: config.micSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    appSources: config.appSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    masterGain: config.masterGain,
    downmixToMono: config.downmixToMono,
  };
}

export function applyPreferredOutput(config: AppConfig, renderDevices: AudioDevice[]) {
  const selectableRenderDevices = outputDevicesForPicker(renderDevices);
  const selectedOutputExists = Boolean(
    config.outputDeviceId && selectableRenderDevices.some((device) => device.id === config.outputDeviceId),
  );
  if (selectedOutputExists || !selectableRenderDevices.length) {
    return config;
  }

  const preferred = preferredOutputDevice(selectableRenderDevices);
  return preferred ? { ...config, outputDeviceId: preferred.id } : config;
}


export function settingsFromConfig(config: AppSettings): AppSettings {
  return { shortcuts: { ...config.shortcuts }, startWithWindows: config.startWithWindows, minimizeToTray: config.minimizeToTray };
}

export function cloneAppConfig(config: AppConfig): AppConfig {
  return { ...config, ...settingsFromConfig(config), micSources: config.micSources.map(source => ({ ...source })), appSources: config.appSources.map(source => ({ ...source })) };
}
