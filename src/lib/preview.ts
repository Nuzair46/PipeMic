import type { AppConfig, AppSettings, AudioDevice, AudioSession, ControlUpdate, LevelMeters, RouteStatus } from "./types";
import { cloneAppConfig, neutralTone } from "./config";
import { micSourceId, appSourceId } from "./devices";
import { currentAppVersion, checkForUpdate, sourceUrl, safeReleaseUrl } from "./updates";

async function command<T>(_name: string, _args: unknown, fallback: () => T): Promise<T> {
  await new Promise(resolve => setTimeout(resolve, 90));
  return fallback();
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
  helloKittyMode: false,
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
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "session:game:9920",
    displayName: "Game Client",
    executable: "Game.exe",
    processId: 9920,
    state: "active",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "session:discord:1840",
    displayName: "Discord",
    executable: "Discord.exe",
    processId: 1840,
    state: "active",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "session:vrchat:2884",
    displayName: "VRChat",
    executable: "VRChat.exe",
    processId: 2884,
    state: "active",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "window:firefox:7332",
    displayName: "firefox",
    executable: "firefox.exe",
    processId: 7332,
    state: "inactive",
    isExcludedDefault: false,
    windowTitle: "YouTube - Mozilla Firefox",
    hasAudioSession: false,
    discoverySource: "window",
  },
  {
    id: "session:pipemic:1112",
    displayName: "PipeMic",
    executable: "pipemic.exe",
    processId: 1112,
    state: "active",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "session:system:0",
    displayName: "System Sounds",
    executable: "System Audio",
    processId: 0,
    state: "active",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
  },
  {
    id: "session:expired:88",
    displayName: "Closed Player",
    executable: "Closed.exe",
    processId: 88,
    state: "expired",
    isExcludedDefault: false,
    windowTitle: null,
    hasAudioSession: true,
    discoverySource: "audioSession",
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
      tone: { ...neutralTone },
    },
  ],
  appSources: [
    {
      id: appSourceId("Spotify.exe"),
      executable: "Spotify.exe",
      displayName: "Spotify",
      gain: 0.72,
      muted: false,
      tone: { ...neutralTone },
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

export const previewApi = {
  listCaptureDevices: () => command<AudioDevice[]>("list_capture_devices", {}, () => mockCaptureDevices),
  listRenderDevices: () => command<AudioDevice[]>("list_render_devices", {}, () => mockRenderDevices),
  listSessions: () => command<AudioSession[]>("list_sessions", {}, () => mockSessions),
  loadConfig: () => command<AppConfig>("load_config", {}, () => mockConfig),
  saveConfig: (config: AppConfig) =>
    command<AppConfig>("save_config", { config }, () => {
      mockConfig = cloneAppConfig(config);
      return mockConfig;
    }),
  startRouting: (config: AppConfig) =>
    command<RouteStatus>("start_routing", { config }, () => {
      mockConfig = cloneAppConfig(config);
      mockStartedAt = Date.now();
      return mockStatus();
    }),
  stopRouting: () =>
    command<RouteStatus>("stop_routing", {}, () => {
      mockStartedAt = 0;
      return mockStatus();
    }),
  getStatus: () => command<RouteStatus>("get_status", {}, mockStatus),
  getAppVersion: currentAppVersion,
  checkForUpdate,
  openSourceUrl: () =>
    command<void>("open_source_url", {}, () => {
      window.open(sourceUrl, "_blank", "noopener,noreferrer");
    }),
  openReleasesUrl: (url?: string | null) => {
    const targetUrl = safeReleaseUrl(url);
    return command<void>("open_releases_url", { url: targetUrl }, () => {
      window.open(targetUrl, "_blank", "noopener,noreferrer");
    });
  },
  applyAppSettings: (config: AppSettings) =>
    command<AppConfig>("apply_app_settings", { config }, () => {
      mockConfig = cloneAppConfig({ ...mockConfig, ...config });
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
