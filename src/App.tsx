import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  appSourceId,
  initialUpdateCheck,
  isRunning,
  isVbCableDevice,
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
  type UpdateCheckResult,
} from "@/lib/api";
import { AppHeader } from "@/components/app/AppHeader";
import { OutputPanel } from "@/components/app/OutputPanel";
import { SettingsDialog, cloneAppConfig, settingsValidationError } from "@/components/app/SettingsDialog";
import { SourcesPanel } from "@/components/app/SourcesPanel";
import { isSelectableSession, savedDisplayName, uniqueSessionsByExecutable } from "@/components/app/source-labels";
import { ToastProvider, ToastStack, type AppToast, type ToastTone } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { registerHotkeys } from "@/lib/hotkeys";

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

function controlsFromConfig(config: AppConfig): ControlUpdate {
  return {
    micSources: config.micSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    appSources: config.appSources.map(({ id, gain, muted }) => ({ id, gain, muted })),
    masterGain: config.masterGain,
    downmixToMono: config.downmixToMono,
  };
}

function applyPreferredOutput(config: AppConfig, renderDevices: AudioDevice[]) {
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
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult>(initialUpdateCheck);

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

  useEffect(() => {
    let alive = true;
    void api.checkForUpdate().then((next) => {
      if (alive) {
        setUpdateCheck(next);
      }
    });

    return () => {
      alive = false;
    };
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
    () =>
      selectableSessions.filter(
        (session) => !config.appSources.some((source) => source.executable.toLowerCase() === session.executable.toLowerCase()),
      ),
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
  }, [pushToast, saveConfig]);

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
    }, isRunning(status) ? 80 : 1000);

    return () => window.clearInterval(timer);
  }, [pushToast, status]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void api.listSessions().then(setSessions).catch(() => undefined);
    }, 1200);

    return () => window.clearInterval(timer);
  }, []);

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
      applyControlsConfig({
        micSources: configRef.current.micSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
      });
    },
    [applyControlsConfig],
  );

  const updateAppSource = useCallback(
    (sourceId: string, patch: Partial<Pick<AppSourceConfig, "gain" | "muted">>) => {
      applyControlsConfig({
        appSources: configRef.current.appSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
      });
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
            displayName: savedDisplayName(session),
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

  const openUpdate = useCallback(() => {
    void api.openReleasesUrl(updateCheck.releaseUrl).catch((error) => {
      pushToast("Could not open releases", error instanceof Error ? error.message : String(error), "fail");
    });
  }, [pushToast, updateCheck.releaseUrl]);

  return (
    <ToastProvider swipeDirection="right">
      <TooltipProvider delayDuration={150}>
        <div className="h-screen overflow-hidden bg-background p-5">
          <main className="mx-auto grid h-[calc(100vh-40px)] min-h-[560px] max-w-[1240px] grid-rows-[64px_minmax(0,1fr)] overflow-hidden rounded-lg border border-border bg-background shadow-[0_20px_64px_rgba(0,0,0,0.42)]">
            <AppHeader
              running={running}
              canStart={canStart}
              update={updateCheck}
              onStart={start}
              onStop={stop}
              onSettings={openSettings}
              onOpenUpdate={openUpdate}
            />

            <div className="grid min-h-0 grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
              <SourcesPanel
                booting={booting}
                captureDevices={captureDevices}
                selectableSessions={selectableSessions}
                availableMicDevices={availableMicDevices}
                availableAppSessions={availableAppSessions}
                micSources={config.micSources}
                appSources={config.appSources}
                meters={status.meters}
                onAddMicSource={addMicSource}
                onAddAppSource={addAppSource}
                onMicSourceChange={updateMicSource}
                onAppSourceChange={updateAppSource}
                onRemoveMicSource={removeMicSource}
                onRemoveAppSource={removeAppSource}
              />

              <OutputPanel
                booting={booting}
                running={running}
                devices={outputPickerDevices}
                outputDeviceId={config.outputDeviceId}
                outputGuidance={outputGuidance}
                outputGuidanceWarns={outputGuidanceWarns}
                masterGain={config.masterGain}
                outputPeak={status.meters.outputPeak}
                onOutputDeviceChange={(outputDeviceId) => applyTopologyConfig({ outputDeviceId })}
                onMasterGainChange={(masterGain) => applyControlsConfig({ masterGain })}
              />
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
