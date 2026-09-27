export type DeviceFlow = "capture" | "render";
export type RouteState = "stopped" | "running" | "deviceMissing" | "appInactive" | "captureFailed";
export type SessionState = "active" | "inactive" | "expired";
export type AppDiscoverySource = "audioSession" | "window" | "merged";

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
  windowTitle?: string | null;
  hasAudioSession: boolean;
  discoverySource: AppDiscoverySource;
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
  helloKittyMode: boolean;
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

export type UpdateCheckStatus = "checking" | "current" | "updateAvailable" | "unavailable";

export interface UpdateCheckResult {
  status: UpdateCheckStatus;
  currentVersion: string;
  latestVersion?: string;
  releaseUrl?: string;
}


export type AppSettings = Pick<AppConfig, "shortcuts" | "startWithWindows" | "minimizeToTray" | "helloKittyMode">;
