import { invokeCommand, isTauri } from "./transport";
import { previewApi } from "./preview";
import { currentAppVersion, checkForUpdate, safeReleaseUrl } from "./updates";
import type { AppConfig, AppSettings, ControlUpdate } from "./types";
export * from "./types";
export * from "./devices";
export { initialUpdateCheck, compareSemverVersions } from "./updates";
export const isRunning = (status: { state: string }) => status.state === "running";

const nativeApi = {
  listCaptureDevices: () => invokeCommand("list_capture_devices", {}),
  listRenderDevices: () => invokeCommand("list_render_devices", {}),
  listSessions: () => invokeCommand("list_sessions", {}),
  loadConfig: () => invokeCommand("load_config", {}),
  saveConfig: (config: AppConfig) => invokeCommand("save_config", { config }),
  applyAppSettings: (config: AppSettings) => invokeCommand("apply_app_settings", { config }),
  startRouting: (config: AppConfig) => invokeCommand("start_routing", { config }),
  stopRouting: () => invokeCommand("stop_routing", {}),
  updateControls: (controls: ControlUpdate) => invokeCommand("update_controls", { controls }),
  getStatus: () => invokeCommand("get_status", {}),
  getAppVersion: currentAppVersion,
  checkForUpdate,
  openSourceUrl: () => invokeCommand("open_source_url", {}),
  openReleasesUrl: (url?: string | null) => invokeCommand("open_releases_url", { url: safeReleaseUrl(url) }),
};

export const api = isTauri() ? nativeApi : previewApi;
