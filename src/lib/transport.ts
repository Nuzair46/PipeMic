import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, AppSettings, AudioDevice, AudioSession, ControlUpdate, RouteStatus } from "./types";
import { errorMessage } from "./errors";

declare global {
  interface Window { __TAURI_INTERNALS__?: unknown; }
}

export const isTauri = () => typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

export interface Commands {
  list_capture_devices: [{}, AudioDevice[]];
  list_render_devices: [{}, AudioDevice[]];
  list_sessions: [{}, AudioSession[]];
  load_config: [{}, AppConfig];
  save_config: [{ config: AppConfig }, AppConfig];
  apply_app_settings: [{ config: AppSettings }, AppConfig];
  start_routing: [{ config: AppConfig }, RouteStatus];
  stop_routing: [{}, RouteStatus];
  get_status: [{}, RouteStatus];
  update_controls: [{ controls: ControlUpdate }, RouteStatus];
  open_source_url: [{}, void];
  open_releases_url: [{ url: string }, void];
}

export async function invokeCommand<K extends keyof Commands>(name: K, args: Commands[K][0]): Promise<Commands[K][1]> {
  try {
    return await invoke<Commands[K][1]>(name, args);
  } catch (error) {
    throw new Error(errorMessage(error));
  }
}
