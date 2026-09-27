import type { AppConfig, AppSettings, AudioDevice, AudioSession, ControlUpdate, RouteStatus } from "./types";
import { applyPreferredOutput, cloneAppConfig, controlsFromConfig, defaultConfig, settingsFromConfig, stoppedStatus } from "./config";
import { errorMessage } from "./errors";

export interface MixerApi {
  loadConfig(): Promise<AppConfig>;
  saveConfig(config: AppConfig): Promise<AppConfig>;
  applyAppSettings(settings: AppSettings): Promise<AppConfig>;
  listCaptureDevices(): Promise<AudioDevice[]>;
  listRenderDevices(): Promise<AudioDevice[]>;
  listSessions(): Promise<AudioSession[]>;
  startRouting(config: AppConfig): Promise<RouteStatus>;
  stopRouting(): Promise<RouteStatus>;
  updateControls(controls: ControlUpdate): Promise<RouteStatus>;
  getStatus(): Promise<RouteStatus>;
}

interface MixerState {
  config: AppConfig;
  status: RouteStatus;
  captureDevices: AudioDevice[];
  renderDevices: AudioDevice[];
  sessions: AudioSession[];
  ready: boolean;
}

// The controller owns user intent. Only one mutation crosses IPC at a time;
// reads and earlier responses cannot overwrite a more recent intent.
export class MixerController {
  private state: MixerState = { config: cloneAppConfig(defaultConfig), status: stoppedStatus, captureDevices: [], renderDevices: [], sessions: [], ready: false };
  private listeners = new Set<() => void>();
  private queue: Promise<unknown> = Promise.resolve();
  private revision = 0;
  private routeRevision = 0;
  private runningRevision = 0;
  private desiredRunning = false;
  private pending = 0;
  private queuedControls?: Promise<void>;
  private controlsRevision = 0;
  private refreshing?: Promise<void>;
  private readingStatus?: Promise<void>;
  private errors = new Map<string, string>();
  private warned = new Set<string>();

  constructor(private readonly api: MixerApi, private readonly onError: (title: string, message: string) => void) {}

  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  private publish(patch: Partial<MixerState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach(listener => listener());
  }

  private report(title: string, error: unknown) {
    const message = errorMessage(error);
    if (this.errors.get(title) !== message) this.onError(title, message);
    this.errors.set(title, message);
  }

  private acceptStatus(status: RouteStatus) {
    this.publish({ status });
    if (status.state === "captureFailed" || status.state === "deviceMissing") {
      this.report(status.message, status.warnings[0] ?? status.message);
    }
    const warnings = new Set(status.warnings);
    for (const warning of warnings) {
      if (!this.warned.has(warning)) this.onError("Routing notice", warning);
    }
    this.warned = warnings;
  }

  refresh = (): Promise<void> => {
    if (this.refreshing) return this.refreshing;
    this.refreshing = this.discover().finally(() => { this.refreshing = undefined; });
    return this.refreshing;
  };

  private async discover() {
    const read = async <T>(title: string, fetch: () => Promise<T>, apply: (value: T) => void) => {
      try { apply(await fetch()); this.errors.delete(title); }
      catch (error) { this.report(title, error); }
    };
    await Promise.all([
      this.state.ready ? Promise.resolve() : read("Could not load settings", () => this.api.loadConfig(), config => this.publish({ config: cloneAppConfig(config), ready: true })),
      read("Could not refresh microphones", () => this.api.listCaptureDevices(), captureDevices => this.publish({ captureDevices })),
      read("Could not refresh outputs", () => this.api.listRenderDevices(), renderDevices => this.publish({ renderDevices })),
      read("Could not refresh applications", () => this.api.listSessions(), sessions => this.publish({ sessions })),
    ]);
    // Choose the normal initial default only after authoritative config loads.
    // Preserve an unplugged selection during routing so discovery cannot reroute audio.
    if (this.state.ready && !this.desiredRunning) {
      const next = applyPreferredOutput(this.state.config, this.state.renderDevices);
      if (next !== this.state.config) await this.changeTopology({ outputDeviceId: next.outputDeviceId });
    }
  }

  pollStatus = (): Promise<void> => {
    if (this.readingStatus || this.pending) return this.readingStatus ?? Promise.resolve();
    const revision = this.revision;
    this.readingStatus = this.api.getStatus().then(status => {
      if (revision !== this.revision || this.pending) return;
      this.desiredRunning = status.state === "running";
      this.acceptStatus(status);
      this.errors.delete("Could not read routing status");
    }).catch(error => this.report("Could not read routing status", error)).finally(() => { this.readingStatus = undefined; });
    return this.readingStatus;
  };

  private enqueue<T>(title: string | null, action: (revision: number) => Promise<T>): Promise<T> {
    const revision = ++this.revision;
    this.pending++;
    const operation = this.queue.then(() => action(revision));
    this.queue = operation.then(
      () => { if (title) this.errors.delete(title); },
      error => { if (title) this.report(title, error); },
    ).finally(() => { this.pending--; });
    return operation;
  }

  changeControls = (patch: Partial<AppConfig>) => {
    if (!this.state.ready) return Promise.resolve();
    this.publish({ config: { ...this.state.config, ...patch } });
    if (this.queuedControls) {
      this.controlsRevision = ++this.revision;
      return this.queuedControls;
    }
    this.queuedControls = this.enqueue("Could not update controls", async () => {
      // Allow one later update while IPC is in flight. Read the latest intent
      // here so drags, gain/mute edits and source removal share one snapshot.
      this.queuedControls = undefined;
      const revision = this.controlsRevision;
      const status = await this.api.updateControls(controlsFromConfig(this.state.config));
      if (revision === this.revision) this.acceptStatus(status);
    }).catch(() => undefined);
    this.controlsRevision = this.revision;
    return this.queuedControls;
  };

  changeTopology = (patch: Partial<AppConfig>) => {
    if (!this.state.ready) return Promise.resolve();
    this.publish({ config: { ...this.state.config, ...patch } });
    const routeRevision = ++this.routeRevision;
    return this.enqueue("Could not update routing", async revision => {
      if (routeRevision !== this.routeRevision) return;
      const config = cloneAppConfig(this.state.config);
      // Saving and starting are one command; there is no deferred restart after a save.
      if (this.desiredRunning) {
        const status = await this.api.startRouting(config);
        if (revision === this.revision) this.acceptStatus(status);
      } else {
        await this.api.saveConfig(config);
      }
    }).catch(() => undefined);
  };

  start = () => this.setRunning(true);
  stop = () => this.setRunning(false);
  toggleRouting = () => this.setRunning(!this.desiredRunning);

  private setRunning(running: boolean) {
    if (!this.state.ready && running) return Promise.resolve();
    this.desiredRunning = running;
    ++this.routeRevision;
    const runningRevision = ++this.runningRevision;
    return this.enqueue(running ? "Could not start routing" : "Could not stop routing", async revision => {
      // A later source selection must not cancel a queued Stop.
      if (runningRevision !== this.runningRevision) return;
      const status = running ? await this.api.startRouting(cloneAppConfig(this.state.config)) : await this.api.stopRouting();
      if (!running && this.state.ready) await this.api.saveConfig(cloneAppConfig(this.state.config));
      if (revision === this.revision) this.acceptStatus(status);
    }).catch(() => undefined);
  }

  saveSettings = (settings: AppSettings) => {
    if (!this.state.ready) return Promise.reject(new Error("Settings have not loaded yet."));
    const patch = settingsFromConfig(settings);
    return this.enqueue(null, async () => {
      await this.api.applyAppSettings(patch);
      // Apply only dialog-owned fields, preserving controls changed while saving.
      this.publish({ config: { ...this.state.config, ...patch } });
    });
  };
}
