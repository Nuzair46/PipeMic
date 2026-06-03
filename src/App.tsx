import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Circle, Github, Mic, PanelRight, Play, Plus, Settings, SlidersHorizontal, Square, Volume2 } from "lucide-react";
import {
  api,
  appSourceId,
  isVbCableDevice,
  isRunning,
  micSourceId,
  outputDevicesForPicker,
  preferredOutputDevice,
  type AppConfig,
  type AppSourceConfig,
  type AudioDevice,
  type AudioSession,
  type ControlUpdate,
  type MicSourceConfig,
  type RouteStatus,
  type ShortcutConfig,
} from "@/lib/api";
import { registerHotkeys } from "@/lib/hotkeys";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { DeviceSelect } from "@/components/mixer/DeviceSelect";
import { LevelMeter } from "@/components/mixer/LevelMeter";
import { SourceStrip } from "@/components/mixer/SourceStrip";
import { ToastProvider, ToastStack, type AppToast, type ToastTone } from "@/components/ui/toast";
import { cn, formatGain } from "@/lib/utils";

const stoppedStatus: RouteStatus = {
  state: "stopped",
  message: "Routing stopped",
  meters: { micPeaks: {}, appPeaks: {}, outputPeak: 0 },
  warnings: [],
};

const defaultConfig: AppConfig = {
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

const SELF_EXECUTABLES = new Set(["pipemic.exe"]);

function controlsFromConfig(config: AppConfig): ControlUpdate {
  return {
    micSources: config.micSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    appSources: config.appSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    masterGain: config.masterGain,
    downmixToMono: config.downmixToMono,
  };
}

function isSelectableSession(session: AudioSession) {
  const executable = session.executable.trim();
  const normalized = executable.toLowerCase();
  return (
    session.state !== "expired" &&
    normalized.endsWith(".exe") &&
    !SELF_EXECUTABLES.has(normalized) &&
    !executable.includes("\\") &&
    !executable.includes("/")
  );
}

function sessionForExecutable(sessions: AudioSession[], executable: string) {
  const matches = sessions.filter((session) => session.executable.toLowerCase() === executable.toLowerCase());
  return matches.find((session) => session.state === "active") ?? matches[0] ?? null;
}

function uniqueSessionsByExecutable(sessions: AudioSession[]) {
  const seen = new Set<string>();
  return sessions.filter((session) => {
    const key = session.executable.toLowerCase();
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function applyPreferredOutput(config: AppConfig, renderDevices: AudioDevice[]) {
  const selectableRenderDevices = outputDevicesForPicker(renderDevices);
  const selectedOutputExists = Boolean(config.outputDeviceId && selectableRenderDevices.some((device) => device.id === config.outputDeviceId));
  if (selectedOutputExists || !selectableRenderDevices.length) {
    return config;
  }

  const preferred = preferredOutputDevice(selectableRenderDevices);
  return preferred ? { ...config, outputDeviceId: preferred.id } : config;
}

export default function App() {
  const [captureDevices, setCaptureDevices] = useState<AudioDevice[]>([]);
  const [renderDevices, setRenderDevices] = useState<AudioDevice[]>([]);
  const [sessions, setSessions] = useState<AudioSession[]>([]);
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [status, setStatus] = useState<RouteStatus>(stoppedStatus);
  const [booting, setBooting] = useState(true);
  const [toasts, setToasts] = useState<AppToast[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [draftConfig, setDraftConfig] = useState<AppConfig>(defaultConfig);

  const configRef = useRef(config);
  const statusRef = useRef(status);
  const lastStatusToastRef = useRef("");
  const toastIdRef = useRef(1);

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    statusRef.current = status;
  }, [status]);

  const pushToast = useCallback((title: string, description?: string, tone: ToastTone = "info") => {
    const id = toastIdRef.current++;
    setToasts((current) => [...current.slice(-3), { id, title, description, tone }]);
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const running = isRunning(status);
  const outputPickerDevices = useMemo(() => outputDevicesForPicker(renderDevices), [renderDevices]);
  const selectedOutput = renderDevices.find((device) => device.id === config.outputDeviceId) ?? null;
  const outputGuidanceWarns = !selectedOutput || !isVbCableDevice(selectedOutput);
  const outputGuidance = outputGuidanceWarns
    ? "PipeMic works best with VB-CABLE. Select CABLE Input here, then select CABLE Output as the microphone/input in your app."
    : "Select CABLE Output as the microphone/input in your app.";
  const selectableSessions = useMemo(() => uniqueSessionsByExecutable(sessions.filter(isSelectableSession)), [sessions]);
  const availableMicDevices = useMemo(
    () => captureDevices.filter((device) => !config.micSources.some((source) => source.deviceId === device.id)),
    [captureDevices, config.micSources],
  );
  const availableAppSessions = useMemo(
    () => selectableSessions.filter((session) => !config.appSources.some((source) => source.executable.toLowerCase() === session.executable.toLowerCase())),
    [config.appSources, selectableSessions],
  );

  useEffect(() => {
    if (status.state === "stopped") {
      lastStatusToastRef.current = "";
      return;
    }

    if (status.state !== "captureFailed" && status.state !== "deviceMissing") {
      return;
    }

    const warning = status.warnings[0] ?? "";
    const key = `${status.state}:${status.message}:${warning}`;
    if (key === lastStatusToastRef.current) {
      return;
    }
    lastStatusToastRef.current = key;
    pushToast(status.message, warning || undefined, "fail");
  }, [pushToast, status]);

  const saveConfig = useCallback(
    async (next: AppConfig) => {
      try {
        await api.saveConfig(next);
        return true;
      } catch (error) {
        pushToast("Could not save settings", error instanceof Error ? error.message : String(error), "fail");
        return false;
      }
    },
    [pushToast],
  );

  const openSettings = useCallback(() => {
    setDraftConfig(cloneAppConfig(configRef.current));
    setSettingsOpen(true);
  }, []);

  const saveSettings = useCallback(async () => {
    const error = settingsValidationError(draftConfig.shortcuts);
    if (error) {
      pushToast("Could not save settings", error, "fail");
      return;
    }

    try {
      const next = await api.applyAppSettings(draftConfig);
      configRef.current = next;
      setConfig(next);
      setDraftConfig(cloneAppConfig(next));
      setSettingsOpen(false);
      pushToast("Settings saved", undefined, "success");
    } catch (error) {
      pushToast("Could not save settings", error instanceof Error ? error.message : String(error), "fail");
    }
  }, [draftConfig, pushToast]);

  const applyControlsConfig = useCallback(
    (patch: Partial<AppConfig>) => {
      const next = { ...configRef.current, ...patch };
      configRef.current = next;
      setConfig(next);
      void saveConfig(next);
      void api
        .updateControls(controlsFromConfig(next))
        .then(setStatus)
        .catch((error) => {
          const message = error instanceof Error ? error.message : String(error);
          setStatus({
            state: "captureFailed",
            message,
            meters: statusRef.current.meters,
            warnings: [],
          });
          pushToast("Could not update controls", message, "fail");
        });
    },
    [pushToast, saveConfig],
  );

  const applyTopologyConfig = useCallback(
    (patch: Partial<AppConfig>) => {
      const wasRunning = isRunning(statusRef.current);
      const next = { ...configRef.current, ...patch };
      configRef.current = next;
      setConfig(next);
      void saveConfig(next)
        .then((saved) => {
          if (!saved) {
            return undefined;
          }
          if (!wasRunning) {
            return undefined;
          }
          return api.startRouting(next).then(setStatus);
        })
        .catch((error) => {
          pushToast("Could not update routing", error instanceof Error ? error.message : String(error), "fail");
        });
    },
    [pushToast, saveConfig],
  );

  const refreshDevices = useCallback(async () => {
    try {
      const [capture, render, appSessions, currentStatus] = await Promise.all([
        api.listCaptureDevices(),
        api.listRenderDevices(),
        api.listSessions(),
        api.getStatus(),
      ]);

      setCaptureDevices(capture);
      setRenderDevices(render);
      setSessions(appSessions);
      setStatus(currentStatus);

      setConfig((current) => {
        const next = applyPreferredOutput(current, render);
        const changed = next !== current;
        configRef.current = next;
        if (changed) {
          void saveConfig(next);
        }
        return changed ? next : current;
      });
    } catch (error) {
      pushToast("Could not refresh devices", error instanceof Error ? error.message : String(error), "fail");
    }
  }, [pushToast, saveConfig]);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const [savedConfig, capture, render, appSessions, currentStatus] = await Promise.all([
          api.loadConfig(),
          api.listCaptureDevices(),
          api.listRenderDevices(),
          api.listSessions(),
          api.getStatus(),
        ]);
        if (!alive) {
          return;
        }
        const nextConfig = applyPreferredOutput(savedConfig, render);
        configRef.current = nextConfig;
        setConfig(nextConfig);
        setCaptureDevices(capture);
        setRenderDevices(render);
        setSessions(appSessions);
        setStatus(currentStatus);
        if (nextConfig !== savedConfig) {
          void saveConfig(nextConfig);
        }
      } catch (error) {
        if (alive) {
          pushToast("PipeMic could not load devices", error instanceof Error ? error.message : String(error), "fail");
        }
      } finally {
        if (alive) {
          setBooting(false);
        }
      }
    })();

    return () => {
      alive = false;
    };
  }, [pushToast]);

  useEffect(() => {
    if (!booting) {
      void refreshDevices();
    }
  }, [booting, refreshDevices]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void api.getStatus().then(setStatus).catch((error) => {
        pushToast("Could not read routing status", error instanceof Error ? error.message : String(error), "fail");
      });
      void api.listSessions().then(setSessions).catch(() => undefined);
    }, isRunning(status) ? 220 : 1200);

    return () => window.clearInterval(timer);
  }, [pushToast, status]);

  const start = useCallback(async () => {
    try {
      const next = await api.startRouting(configRef.current);
      setStatus(next);
    } catch (error) {
      pushToast("Could not start routing", error instanceof Error ? error.message : String(error), "fail");
    }
  }, [pushToast]);

  const stop = useCallback(async () => {
    try {
      const next = await api.stopRouting();
      setStatus(next);
    } catch (error) {
      pushToast("Could not stop routing", error instanceof Error ? error.message : String(error), "fail");
    }
  }, [pushToast]);

  const toggleRouting = useCallback(() => {
    if (isRunning(statusRef.current)) {
      void stop();
    } else {
      void start();
    }
  }, [start, stop]);

  const updateMicSource = useCallback(
    (sourceId: string, patch: Partial<Pick<MicSourceConfig, "gain" | "muted">>) => {
      applyControlsConfig(
        {
          micSources: configRef.current.micSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
        }
      );
    },
    [applyControlsConfig],
  );

  const updateAppSource = useCallback(
    (sourceId: string, patch: Partial<Pick<AppSourceConfig, "gain" | "muted">>) => {
      applyControlsConfig(
        {
          appSources: configRef.current.appSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
        }
      );
    },
    [applyControlsConfig],
  );

  const toggleMicMute = useCallback(() => {
    const sources = configRef.current.micSources;
    if (!sources.length) {
      return;
    }
    const muted = sources.some((source) => !source.muted);
    applyControlsConfig({ micSources: sources.map((source) => ({ ...source, muted })) });
  }, [applyControlsConfig]);

  const toggleAppMute = useCallback(() => {
    const sources = configRef.current.appSources;
    if (!sources.length) {
      return;
    }
    const muted = sources.some((source) => !source.muted);
    applyControlsConfig({ appSources: sources.map((source) => ({ ...source, muted })) });
  }, [applyControlsConfig]);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void registerHotkeys(config.shortcuts, { toggleMicMute, toggleAppMute, toggleRouting }).then((dispose) => {
      if (disposed) {
        dispose();
        return;
      }
      cleanup = dispose;
    });

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [config.shortcuts, toggleAppMute, toggleMicMute, toggleRouting]);

  const addMicSource = useCallback(
    (deviceId: string) => {
      if (configRef.current.micSources.some((source) => source.deviceId === deviceId)) {
        return;
      }
      applyTopologyConfig({
        micSources: [
          ...configRef.current.micSources,
          {
            id: micSourceId(deviceId),
            deviceId,
            gain: 1,
            muted: false,
          },
        ],
      });
    },
    [applyTopologyConfig],
  );

  const addAppSource = useCallback(
    (sessionId: string) => {
      const session = sessions.find((item) => item.id === sessionId);
      if (!session || configRef.current.appSources.some((source) => source.executable.toLowerCase() === session.executable.toLowerCase())) {
        return;
      }
      applyTopologyConfig({
        appSources: [
          ...configRef.current.appSources,
          {
            id: appSourceId(session.executable),
            executable: session.executable,
            displayName: session.displayName,
            gain: 1,
            muted: false,
          },
        ],
      });
    },
    [applyTopologyConfig, sessions],
  );

  const removeMicSource = useCallback(
    (sourceId: string) => {
      applyTopologyConfig({ micSources: configRef.current.micSources.filter((source) => source.id !== sourceId) });
    },
    [applyTopologyConfig],
  );

  const removeAppSource = useCallback(
    (sourceId: string) => {
      applyTopologyConfig({ appSources: configRef.current.appSources.filter((source) => source.id !== sourceId) });
    },
    [applyTopologyConfig],
  );

  const canStart = Boolean(config.outputDeviceId && (config.micSources.length || config.appSources.length));
  const openSource = useCallback(() => {
    void api.openSourceUrl().catch((error) => {
      pushToast("Could not open GitHub", error instanceof Error ? error.message : String(error), "fail");
    });
  }, [pushToast]);

  return (
    <ToastProvider swipeDirection="right">
      <TooltipProvider delayDuration={150}>
        <div className="h-screen overflow-hidden p-5">
          <main className="mx-auto grid h-[calc(100vh-40px)] min-h-[560px] max-w-[1240px] grid-rows-[64px_minmax(0,1fr)] overflow-hidden rounded-lg border border-border bg-[#0e1415] shadow-[0_24px_90px_rgba(0,0,0,0.38)]">
            <header className="flex h-16 items-center justify-between border-b border-border bg-[#151b1f] px-5">
              <div>
                <h1 className="text-lg font-semibold text-foreground">PipeMic</h1>
                <p className="text-xs text-muted-foreground">Multi-source mixer</p>
              </div>
              <div className="flex items-center gap-3">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Circle className={cn("h-2.5 w-2.5 fill-current", running ? "text-primary" : "text-muted-foreground")} />
                  <span>{running ? "Live" : "Idle"}</span>
                </div>
                <Button
                  type="button"
                  variant={running ? "danger" : "default"}
                  disabled={!running && !canStart}
                  onClick={running ? stop : start}
                >
                  {running ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                  {running ? "Stop" : "Start"}
                </Button>
                <Button type="button" variant="ghost" size="icon" onClick={openSettings}>
                  <Settings className="h-4 w-4" />
                  <span className="sr-only">Settings</span>
                </Button>
              </div>
            </header>

            <div className="grid min-h-0 grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
              <section className="grid min-h-0 min-w-0 grid-rows-[44px_minmax(0,1fr)_minmax(0,1fr)] border-r border-border">
                <div className="panel-heading">
                  <div className="flex items-center gap-2">
                    <SlidersHorizontal className="h-4 w-4 text-primary" />
                    <h2 className="text-sm font-semibold uppercase text-muted-foreground">Sources</h2>
                  </div>
                </div>

                <div className="grid min-h-0 grid-rows-[48px_minmax(0,1fr)] border-b border-border">
                  <SourceSectionHeader
                    icon={<Mic className="h-4 w-4 text-primary" />}
                    title="Physical Microphones"
                    addDisabled={!availableMicDevices.length}
                    addLabel="Microphone"
                    onAdd={addMicSource}
                    options={availableMicDevices.map((device) => ({
                      value: device.id,
                      label: device.name,
                      detail: device.isDefault ? "Default" : undefined,
                    }))}
                  />
                  <div className="min-h-0 overflow-y-auto overscroll-contain">
                    {config.micSources.length ? (
                      config.micSources.map((source) => {
                        const device = captureDevices.find((item) => item.id === source.deviceId) ?? null;
                        return (
                          <SourceStrip
                            key={source.id}
                            kind="mic"
                            title={device?.name ?? source.deviceId}
                            detail={device ? undefined : "Device unavailable"}
                            gain={source.gain}
                            muted={source.muted}
                            level={status.meters.micPeaks[source.id] ?? 0}
                            inactive={!device}
                            onGainChange={(gain) => updateMicSource(source.id, { gain })}
                            onMutedChange={(muted) => updateMicSource(source.id, { muted })}
                            onRemove={() => removeMicSource(source.id)}
                          />
                        );
                      })
                    ) : (
                      <EmptySourceRow label={booting ? "Loading microphones" : "No microphones added"} />
                    )}
                  </div>
                </div>

                <div className="grid min-h-0 grid-rows-[48px_minmax(0,1fr)]">
                  <SourceSectionHeader
                    icon={<Volume2 className="h-4 w-4 text-primary" />}
                    title="Application Sources"
                    addDisabled={!availableAppSessions.length}
                    addLabel="Application"
                    onAdd={addAppSource}
                    options={availableAppSessions.map((session) => ({
                      value: session.id,
                      label: session.displayName || session.executable.replace(/\.exe$/i, ""),
                      detail: session.executable,
                    }))}
                  />
                  <div className="min-h-0 overflow-y-auto overscroll-contain">
                    {config.appSources.length ? (
                      config.appSources.map((source) => {
                        const session = sessionForExecutable(selectableSessions, source.executable);
                        const active = session?.state === "active";
                        return (
                          <SourceStrip
                            key={source.id}
                            kind="app"
                            title={source.displayName ?? session?.displayName ?? source.executable.replace(/\.exe$/i, "")}
                            detail={undefined}
                            gain={source.gain}
                            muted={source.muted}
                            level={status.meters.appPeaks[source.id] ?? 0}
                            inactive={!active}
                            onGainChange={(gain) => updateAppSource(source.id, { gain })}
                            onMutedChange={(muted) => updateAppSource(source.id, { muted })}
                            onRemove={() => removeAppSource(source.id)}
                          />
                        );
                      })
                    ) : (
                      <EmptySourceRow label={booting ? "Loading applications" : "No applications added"} />
                    )}
                  </div>
                </div>
              </section>

              <aside className="grid min-h-0 min-w-0 grid-rows-[44px_minmax(0,1fr)] bg-[#0d1214]">
                <div className="panel-heading">
                  <div className="flex items-center gap-2">
                    <PanelRight className="h-4 w-4 text-primary" />
                    <h2 className="text-sm font-semibold uppercase text-muted-foreground">Output</h2>
                  </div>
                </div>

                <div className="grid min-h-0 min-w-0 content-start gap-5 overflow-y-auto p-4">
                  <DeviceSelect
                    label="Virtual Mic"
                    placeholder={booting ? "Loading devices" : "Select virtual mic"}
                    devices={outputPickerDevices}
                    value={config.outputDeviceId}
                    onChange={(outputDeviceId) => applyTopologyConfig({ outputDeviceId })}
                  />
                  <p
                    className={cn(
                      "rounded-md border px-3 py-2 text-xs leading-5 text-muted-foreground",
                      outputGuidanceWarns ? "border-accent/35 bg-accent/10" : "border-border bg-[#0d1214]",
                    )}
                  >
                    {outputGuidance}
                  </p>

                  <div className="grid gap-3 border-y border-border py-4">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-semibold uppercase text-muted-foreground">Master</span>
                      <span className="font-mono text-xs text-foreground">{formatGain(config.masterGain)}</span>
                    </div>
                    <Slider
                      min={0}
                      max={1.5}
                      step={0.01}
                      value={[config.masterGain]}
                      onDoubleClick={() => applyControlsConfig({ masterGain: 1 })}
                      onValueChange={(value) => applyControlsConfig({ masterGain: value[0] ?? config.masterGain })}
                    />
                    <LevelMeter value={status.meters.outputPeak} muted={!running} />
                  </div>
                </div>
              </aside>
            </div>

          </main>
        </div>
        <SettingsDialog
          open={settingsOpen}
          config={draftConfig}
          onOpenChange={setSettingsOpen}
          onConfigChange={setDraftConfig}
          onSave={saveSettings}
          onOpenSource={openSource}
        />
        <ToastStack toasts={toasts} onDismiss={dismissToast} />
      </TooltipProvider>
    </ToastProvider>
  );
}

type SourceOption = {
  value: string;
  label: string;
  detail?: string;
};

type SourceSectionHeaderProps = {
  icon: React.ReactNode;
  title: string;
  options: SourceOption[];
  addDisabled: boolean;
  addLabel: string;
  onAdd: (value: string) => void;
};

function SourceSectionHeader({ icon, title, options, addDisabled, addLabel, onAdd }: SourceSectionHeaderProps) {
  const [value, setValue] = useState("");

  return (
    <div className="flex h-12 min-w-0 items-center justify-between gap-3 border-b border-border bg-[#101619] px-4">
      <div className="flex min-w-0 items-center gap-2">
        {icon}
        <h3 className="truncate text-xs font-semibold uppercase text-muted-foreground">{title}</h3>
      </div>
      <Select
        value={value}
        onValueChange={(next) => {
          onAdd(next);
          setValue("");
        }}
      >
        <SelectTrigger className="h-8 w-[150px] min-w-0 px-2 text-xs" disabled={addDisabled}>
          <Plus className="h-4 w-4 text-primary" />
          <SelectValue placeholder={addLabel} />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>{title}</SelectLabel>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                <span className="flex min-w-0 items-center gap-2">
                  <span className="min-w-0 truncate">{option.label}</span>
                  {option.detail ? <span className="shrink-0 text-xs text-muted-foreground">{option.detail}</span> : null}
                </span>
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  );
}

function EmptySourceRow({ label }: { label: string }) {
  return (
    <div className="flex h-[72px] items-center border-b border-border px-4 text-sm text-muted-foreground">
      <span>{label}</span>
    </div>
  );
}

type SettingsDialogProps = {
  open: boolean;
  config: AppConfig;
  onOpenChange: (open: boolean) => void;
  onConfigChange: (config: AppConfig) => void;
  onSave: () => void;
  onOpenSource: () => void;
};

type ShortcutField = keyof ShortcutConfig;

const shortcutRows: Array<{ key: ShortcutField; label: string }> = [
  { key: "micMute", label: "Mic mute" },
  { key: "appMute", label: "App mute" },
  { key: "routing", label: "Start / Stop" },
];

function SettingsDialog({ open, config, onOpenChange, onConfigChange, onSave, onOpenSource }: SettingsDialogProps) {
  const [recording, setRecording] = useState<ShortcutField | null>(null);
  const validation = settingsValidationError(config.shortcuts);

  useEffect(() => {
    if (!open) {
      setRecording(null);
    }
  }, [open]);

  useEffect(() => {
    if (!recording) {
      return undefined;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecording(null);
        return;
      }

      const shortcut = shortcutFromEvent(event);
      if (!shortcut) {
        return;
      }

      onConfigChange({
        ...config,
        shortcuts: {
          ...config.shortcuts,
          [recording]: shortcut,
        },
      });
      setRecording(null);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [config, onConfigChange, recording]);

  const setChecked = (key: "startWithWindows" | "minimizeToTray", checked: boolean) => {
    onConfigChange({ ...config, [key]: checked });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Configure shortcuts and startup behavior.</DialogDescription>
        </DialogHeader>

        <div className="grid max-h-[calc(100vh-220px)] gap-5 overflow-y-auto px-5 py-5">
          <section className="grid gap-3">
            <div>
              <h3 className="text-xs font-semibold uppercase text-muted-foreground">Shortcuts</h3>
            </div>
            <div className="grid gap-2">
              {shortcutRows.map((row) => (
                <div key={row.key} className="grid grid-cols-[140px_minmax(0,1fr)_96px] items-center gap-3 rounded-md border border-border bg-[#0d1214] px-3 py-2">
                  <span className="text-sm text-foreground">{row.label}</span>
                  <kbd className="min-w-0 truncate rounded border border-border bg-[#070b0d] px-2 py-1.5 font-mono text-xs text-foreground">
                    {recording === row.key ? "Recording..." : config.shortcuts[row.key]}
                  </kbd>
                  <Button type="button" variant="ghost" size="sm" onClick={() => setRecording(row.key)}>
                    Record
                  </Button>
                </div>
              ))}
            </div>
            {validation ? <p className="text-xs text-destructive">{validation}</p> : null}
          </section>

          <section className="grid gap-2">
            <h3 className="text-xs font-semibold uppercase text-muted-foreground">Startup</h3>
            <CheckboxRow
              label="Start with Windows"
              checked={config.startWithWindows}
              onCheckedChange={(checked) => setChecked("startWithWindows", checked)}
            />
            <CheckboxRow
              label="Minimize to tray"
              checked={config.minimizeToTray}
              onCheckedChange={(checked) => setChecked("minimizeToTray", checked)}
            />
          </section>
        </div>

        <DialogFooter className="justify-between">
          <Button type="button" variant="quiet" size="sm" onClick={onOpenSource}>
            <Github className="h-4 w-4" />
            GitHub
          </Button>
          <div className="flex items-center gap-2">
            <Button type="button" variant="quiet" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="button" disabled={Boolean(validation)} onClick={onSave}>
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function CheckboxRow({ label, checked, onCheckedChange }: { label: string; checked: boolean; onCheckedChange: (checked: boolean) => void }) {
  return (
    <label className="flex h-11 cursor-pointer items-center justify-between gap-3 rounded-md border border-border bg-[#0d1214] px-3 text-sm text-foreground">
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onCheckedChange(event.currentTarget.checked)}
        className="h-4 w-4 accent-primary"
      />
    </label>
  );
}

function cloneAppConfig(config: AppConfig): AppConfig {
  return {
    ...config,
    micSources: config.micSources.map((source) => ({ ...source })),
    appSources: config.appSources.map((source) => ({ ...source })),
    shortcuts: { ...config.shortcuts },
  };
}

function settingsValidationError(shortcuts: ShortcutConfig) {
  const values = [shortcuts.micMute, shortcuts.appMute, shortcuts.routing].map((shortcut) => shortcut.trim());
  if (values.some((shortcut) => !shortcut)) {
    return "Shortcut fields cannot be empty.";
  }

  const normalized = values.map((shortcut) => shortcut.toLowerCase());
  if (new Set(normalized).size !== normalized.length) {
    return "Each shortcut must be unique.";
  }

  return "";
}

function shortcutFromEvent(event: KeyboardEvent) {
  const key = shortcutKeyName(event.key);
  if (!key || key === "Ctrl" || key === "Alt" || key === "Shift") {
    return "";
  }

  const parts = [];
  if (event.ctrlKey) {
    parts.push("Ctrl");
  }
  if (event.altKey) {
    parts.push("Alt");
  }
  if (event.shiftKey) {
    parts.push("Shift");
  }
  if (!parts.length) {
    return "";
  }
  parts.push(key);
  return parts.join("+");
}

function shortcutKeyName(key: string) {
  if (key.length === 1) {
    return key.toUpperCase();
  }

  const normalized = key.trim();
  if (!normalized) {
    return "";
  }
  if (normalized === "Control") {
    return "Ctrl";
  }
  if (normalized === " ") {
    return "Space";
  }
  return normalized;
}
