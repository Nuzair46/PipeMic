import type { AppConfig, AppSettings, AudioDevice, ControlUpdate, RouteStatus, ToneConfig } from "./types";
import { outputDevicesForPicker, preferredOutputDevice } from "./devices";

export const neutralTone: ToneConfig = { x: 0, y: 0, bypassed: false };

export function normalizeTone(tone?: Partial<ToneConfig>): ToneConfig {
  const axis = (value: number | undefined) => value !== undefined && Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
  return { x: axis(tone?.x), y: axis(tone?.y), bypassed: tone?.bypassed ?? false };
}

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
  helloKittyMode: false,
};

export function controlsFromConfig(config: AppConfig): ControlUpdate {
  return {
    micSources: config.micSources.map(({ id, gain, muted, tone }) => ({ id, gain, muted, tone: normalizeTone(tone) })),
    appSources: config.appSources.map(({ id, gain, muted, tone }) => ({ id, gain, muted, tone: normalizeTone(tone) })),
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
  return {
    shortcuts: { ...config.shortcuts },
    startWithWindows: config.startWithWindows,
    minimizeToTray: config.minimizeToTray,
    helloKittyMode: config.helloKittyMode ?? false,
  };
}

export function cloneAppConfig(config: AppConfig): AppConfig {
  return { ...config, ...settingsFromConfig(config), micSources: config.micSources.map(source => ({ ...source, tone: normalizeTone(source.tone) })), appSources: config.appSources.map(source => ({ ...source, tone: normalizeTone(source.tone) })) };
}
