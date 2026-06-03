import { invoke } from "@tauri-apps/api/core";

export type DeviceFlow = "capture" | "render";
export type RouteState = "stopped" | "running" | "deviceMissing" | "appInactive" | "captureFailed";
export type SessionState = "active" | "inactive" | "expired";

export interface AudioDevice {
  id: string;
  name: string;
  flow: DeviceFlow;
  isDefault: boolean;
  isVirtualCableLike: boolean;
  channels?: number | null;
  sampleRate?: number | null;
}

export interface AudioSession {
  id: string;
  displayName: string;
  executable: string;
  processId: number;
  state: SessionState;
  isExcludedDefault: boolean;
}

export interface MicSourceConfig {
  id: string;
  deviceId: string;
  gain: number;
  muted: boolean;
}

export interface AppSourceConfig {
  id: string;
  executable: string;
  displayName?: string | null;
  gain: number;
  muted: boolean;
}

export interface SourceControlUpdate {
  id: string;
  gain: number;
  muted: boolean;
}

export interface LevelMeters {
  micPeaks: Record<string, number>;
  appPeaks: Record<string, number>;
  outputPeak: number;
}

export interface RouteStatus {
  state: RouteState;
  message: string;
  meters: LevelMeters;
  warnings: string[];
}

export interface AppConfig {
  micSources: MicSourceConfig[];
  appSources: AppSourceConfig[];
  outputDeviceId: string | null;
  masterGain: number;
  bufferFrames: number;
  downmixToMono: boolean;
  shortcuts: ShortcutConfig;
  startWithWindows: boolean;
  minimizeToTray: boolean;
}

export interface ShortcutConfig {
  micMute: string;
  appMute: string;
  routing: string;
}

export interface ControlUpdate {
  micSources: SourceControlUpdate[];
  appSources: SourceControlUpdate[];
  masterGain: number;
  downmixToMono: boolean;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const isTauri = () => typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
const sourceUrl = "https://github.com/nuzair46/pipemic";

async function command<T>(name: string, args: Record<string, unknown> = {}, fallback: () => T): Promise<T> {
  if (!isTauri()) {
    await new Promise((resolve) => window.setTimeout(resolve, 90));
    return fallback();
  }

  return invoke<T>(name, args);
}

export function micSourceId(deviceId: string) {
  return `mic:${deviceId}`;
}

export function appSourceId(executable: string) {
  return `app:${executable.toLowerCase()}`;
}

export function isVbCableDevice(device: Pick<AudioDevice, "name" | "isVirtualCableLike"> | null | undefined) {
  if (!device?.isVirtualCableLike) {
    return false;
  }

  const name = device.name.toLowerCase();
  return name.includes("vb-audio") || name.includes("vb cable") || name.includes("vb-cable") || name.includes("cable input");
}

function isCanonicalVbCableInput(device: AudioDevice) {
  const name = device.name.toLowerCase();
  return isVbCableDevice(device) && name.includes("cable input") && !isSixteenChannelCableVariant(device);
}

function isSixteenChannelCableVariant(device: AudioDevice) {
  const name = device.name.toLowerCase();
  return name.includes("cable") && name.includes("16ch");
}

export function outputDevicesForPicker(devices: AudioDevice[]) {
  const canonicalVbCable = devices.find(isCanonicalVbCableInput);
  if (!canonicalVbCable) {
    return devices;
  }

  return devices.filter((device) => device.id === canonicalVbCable.id || !isSixteenChannelCableVariant(device));
}

export function preferredOutputDevice(devices: AudioDevice[]) {
  const selectableDevices = outputDevicesForPicker(devices);
  return (
    selectableDevices.find(isCanonicalVbCableInput) ??
    selectableDevices.find(isVbCableDevice) ??
    selectableDevices.find((device) => device.isVirtualCableLike) ??
    selectableDevices.find((device) => device.isDefault) ??
    selectableDevices[0] ??
    null
  );
}

const defaultConfig: AppConfig = {
  micSources: [],
  appSources: [],
  outputDeviceId: "render:cable-input",
  masterGain: 0.9,
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

const mockCaptureDevices: AudioDevice[] = [
  {
    id: "capture:studio-mic",
    name: "Studio Mic / USB Interface",
    flow: "capture",
    isDefault: true,
    isVirtualCableLike: false,
    channels: 2,
    sampleRate: 48000,
  },
  {
    id: "capture:webcam-mic",
    name: "Webcam Microphone",
    flow: "capture",
    isDefault: false,
    isVirtualCableLike: false,
    channels: 1,
    sampleRate: 48000,
  },
  {
    id: "capture:line-in",
    name: "Line In / Interface",
    flow: "capture",
    isDefault: false,
    isVirtualCableLike: false,
    channels: 2,
    sampleRate: 48000,
  },
];

const mockRenderDevices: AudioDevice[] = [
  {
    id: "render:cable-input",
    name: "CABLE Input (VB-Audio Virtual Cable)",
    flow: "render",
    isDefault: false,
    isVirtualCableLike: true,
    channels: 2,
    sampleRate: 48000,
  },
  {
    id: "render:headphones",
    name: "Headphones",
    flow: "render",
    isDefault: true,
    isVirtualCableLike: false,
    channels: 2,
    sampleRate: 48000,
  },
];

const mockSessions: AudioSession[] = [
  {
    id: "session:spotify:4242",
    displayName: "Spotify",
    executable: "Spotify.exe",
    processId: 4242,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:game:9920",
    displayName: "Game Client",
    executable: "Game.exe",
    processId: 9920,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:discord:1840",
    displayName: "Discord",
    executable: "Discord.exe",
    processId: 1840,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:vrchat:2884",
    displayName: "VRChat",
    executable: "VRChat.exe",
    processId: 2884,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:pipemic:1112",
    displayName: "PipeMic",
    executable: "pipemic.exe",
    processId: 1112,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:system:0",
    displayName: "System Sounds",
    executable: "System Audio",
    processId: 0,
    state: "active",
    isExcludedDefault: false,
  },
  {
    id: "session:expired:88",
    displayName: "Closed Player",
    executable: "Closed.exe",
    processId: 88,
    state: "expired",
    isExcludedDefault: false,
  },
];

let mockConfig: AppConfig = {
  ...defaultConfig,
  micSources: [
    {
      id: micSourceId("capture:studio-mic"),
      deviceId: "capture:studio-mic",
      gain: 1,
      muted: false,
    },
  ],
  appSources: [
    {
      id: appSourceId("Spotify.exe"),
      executable: "Spotify.exe",
      displayName: "Spotify",
      gain: 0.72,
      muted: false,
    },
  ],
};
let mockStartedAt = 0;

function emptyMeters(config = mockConfig): LevelMeters {
  return {
    micPeaks: Object.fromEntries(config.micSources.map((source) => [source.id, 0])),
    appPeaks: Object.fromEntries(config.appSources.map((source) => [source.id, 0])),
    outputPeak: 0,
  };
}

function mockStatus(): RouteStatus {
  if (!mockStartedAt) {
    return {
      state: "stopped",
      message: "Routing stopped",
      meters: emptyMeters(),
      warnings: mockConfig.outputDeviceId ? [] : ["Select a virtual cable output before starting."],
    };
  }

  const phase = (Date.now() - mockStartedAt) / 1000;
  const micPeaks = Object.fromEntries(
    mockConfig.micSources.map((source, index) => {
      const level = source.muted ? 0 : Math.abs(Math.sin(phase * (2.2 + index * 0.25))) * 0.76 * source.gain;
      return [source.id, Math.min(1, level)];
    }),
  );
  const appPeaks = Object.fromEntries(
    mockConfig.appSources.map((source, index) => {
      const level = source.muted ? 0 : Math.abs(Math.cos(phase * (1.55 + index * 0.2))) * 0.62 * source.gain;
      return [source.id, Math.min(1, level)];
    }),
  );
  const outputPeak =
    Math.min(
      1,
      [...Object.values(micPeaks), ...Object.values(appPeaks)].reduce((total, level) => total + level, 0) * 0.72 * mockConfig.masterGain,
    ) || 0;

  return {
    state: "running",
    message: `Routing ${mockConfig.micSources.length + mockConfig.appSources.length} sources to selected output`,
    meters: { micPeaks, appPeaks, outputPeak },
    warnings: [],
  };
}

export const api = {
  listCaptureDevices: () => command<AudioDevice[]>("list_capture_devices", {}, () => mockCaptureDevices),
  listRenderDevices: () => command<AudioDevice[]>("list_render_devices", {}, () => mockRenderDevices),
  listSessions: () => command<AudioSession[]>("list_sessions", {}, () => mockSessions),
  loadConfig: () => command<AppConfig>("load_config", {}, () => mockConfig),
  saveConfig: (config: AppConfig) =>
    command<AppConfig>("save_config", { config }, () => {
      mockConfig = cloneConfig(config);
      return mockConfig;
    }),
  startRouting: (config: AppConfig) =>
    command<RouteStatus>("start_routing", { config }, () => {
      mockConfig = cloneConfig(config);
      mockStartedAt = Date.now();
      return mockStatus();
    }),
  stopRouting: () =>
    command<RouteStatus>("stop_routing", {}, () => {
      mockStartedAt = 0;
      return mockStatus();
    }),
  getStatus: () => command<RouteStatus>("get_status", {}, mockStatus),
  openSourceUrl: () =>
    command<void>("open_source_url", {}, () => {
      window.open(sourceUrl, "_blank", "noopener,noreferrer");
    }),
  applyAppSettings: (config: AppConfig) =>
    command<AppConfig>("apply_app_settings", { config }, () => {
      mockConfig = cloneConfig(config);
      return mockConfig;
    }),
  updateControls: (controls: ControlUpdate) =>
    command<RouteStatus>("update_controls", { controls }, () => {
      const micControls = new Map(controls.micSources.map((source) => [source.id, source]));
      const appControls = new Map(controls.appSources.map((source) => [source.id, source]));
      mockConfig = {
        ...mockConfig,
        micSources: mockConfig.micSources.map((source) => ({ ...source, ...micControls.get(source.id) })),
        appSources: mockConfig.appSources.map((source) => ({ ...source, ...appControls.get(source.id) })),
        masterGain: controls.masterGain,
        downmixToMono: controls.downmixToMono,
      };
      return mockStatus();
    }),
};

function cloneConfig(config: AppConfig): AppConfig {
  return {
    ...config,
    micSources: config.micSources.map((source) => ({ ...source })),
    appSources: config.appSources.map((source) => ({ ...source })),
    shortcuts: { ...config.shortcuts },
  };
}

export function isRunning(status: RouteStatus) {
  return status.state === "running";
}
