import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  api,
  appSourceId,
  initialUpdateCheck,
  isRunning,
  isVbCableDevice,
  micSourceId,
  outputDevicesForPicker,
  type AppSourceConfig,
  type AppSettings,
  type MicSourceConfig,
  type UpdateCheckResult,
} from "@/lib/api";
import { AppHeader } from "@/components/app/AppHeader";
import { OutputPanel } from "@/components/app/OutputPanel";
import { SettingsDialog, settingsValidationError } from "@/components/app/SettingsDialog";
import { SourcesPanel } from "@/components/app/SourcesPanel";
import { isSelectableSession, savedDisplayName, uniqueSessionsByExecutable } from "@/components/app/source-labels";
import { ToastProvider, ToastStack, type AppToast, type ToastTone } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useMixer } from "@/lib/use-mixer";
import { defaultConfig, settingsFromConfig } from "@/lib/config";
import { registerHotkeys } from "@/lib/hotkeys";

export default function App() {
  const [toasts, setToasts] = useState<AppToast[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [draftConfig, setDraftConfig] = useState<AppSettings>(settingsFromConfig(defaultConfig));
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult>(initialUpdateCheck);
  const toastIdRef = useRef(1);

  const pushToast = useCallback((title: string, description?: string, tone: ToastTone = "info") => {
    const id = toastIdRef.current++;
    setToasts((current) => [...current.slice(-3), { id, title, description, tone }]);
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const onError = useCallback((title: string, message: string) => pushToast(title, message, "fail"), [pushToast]);
  const { config, status, captureDevices, renderDevices, sessions, ready, controller } = useMixer(onError);
  const booting = !ready;
  const helloKittyMode = settingsOpen ? draftConfig.helloKittyMode : config.helloKittyMode;

  useLayoutEffect(() => {
    // Apply at the document root so portaled dialogs, menus, and toasts match.
    if (helloKittyMode) {
      document.documentElement.dataset.theme = "hello-kitty";
    } else {
      delete document.documentElement.dataset.theme;
    }
    return () => {
      delete document.documentElement.dataset.theme;
    };
  }, [helloKittyMode]);

  const applyControlsConfig = controller.changeControls;
  const applyTopologyConfig = controller.changeTopology;
  const start = controller.start;
  const stop = controller.stop;
  const toggleRouting = controller.toggleRouting;

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

  const openSettings = useCallback(() => {
    setDraftConfig(settingsFromConfig(controller.getSnapshot().config));
    setSettingsOpen(true);
  }, [controller]);

  const saveSettings = useCallback(async () => {
    const error = settingsValidationError(draftConfig.shortcuts);
    if (error) {
      pushToast("Could not save settings", error, "fail");
      return;
    }

    try {
      await controller.saveSettings(draftConfig);
      setSettingsOpen(false);
      pushToast("Settings saved", undefined, "success");
    } catch (error) {
      pushToast("Could not save settings", error instanceof Error ? error.message : String(error), "fail");
    }
  }, [controller, draftConfig, pushToast]);

  const updateMicSource = useCallback(
    (sourceId: string, patch: Partial<Pick<MicSourceConfig, "gain" | "muted">>) => {
      applyControlsConfig({
        micSources: controller.getSnapshot().config.micSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
      });
    },
    [applyControlsConfig],
  );

  const updateAppSource = useCallback(
    (sourceId: string, patch: Partial<Pick<AppSourceConfig, "gain" | "muted">>) => {
      applyControlsConfig({
        appSources: controller.getSnapshot().config.appSources.map((source) => (source.id === sourceId ? { ...source, ...patch } : source)),
      });
    },
    [applyControlsConfig],
  );

  const toggleMicMute = useCallback(() => {
    const sources = controller.getSnapshot().config.micSources;
    if (!sources.length) {
      return;
    }
    const muted = sources.some((source) => !source.muted);
    applyControlsConfig({ micSources: sources.map((source) => ({ ...source, muted })) });
  }, [applyControlsConfig]);

  const toggleAppMute = useCallback(() => {
    const sources = controller.getSnapshot().config.appSources;
    if (!sources.length) {
      return;
    }
    const muted = sources.some((source) => !source.muted);
    applyControlsConfig({ appSources: sources.map((source) => ({ ...source, muted })) });
  }, [applyControlsConfig]);

  useEffect(() => {
    if (!ready) return;
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
  }, [ready, config.shortcuts, toggleAppMute, toggleMicMute, toggleRouting]);

  const addMicSource = useCallback(
    (deviceId: string) => {
      if (controller.getSnapshot().config.micSources.some((source) => source.deviceId === deviceId)) {
        return;
      }
      applyTopologyConfig({
        micSources: [
          ...controller.getSnapshot().config.micSources,
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
      if (!session || controller.getSnapshot().config.appSources.some((source) => source.executable.toLowerCase() === session.executable.toLowerCase())) {
        return;
      }
      applyTopologyConfig({
        appSources: [
          ...controller.getSnapshot().config.appSources,
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
      applyTopologyConfig({ micSources: controller.getSnapshot().config.micSources.filter((source) => source.id !== sourceId) });
    },
    [applyTopologyConfig],
  );

  const removeAppSource = useCallback(
    (sourceId: string) => {
      applyTopologyConfig({ appSources: controller.getSnapshot().config.appSources.filter((source) => source.id !== sourceId) });
    },
    [applyTopologyConfig],
  );

  const canStart = Boolean(ready && config.outputDeviceId && (config.micSources.length || config.appSources.length));
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
        <div className="app-canvas h-screen overflow-hidden bg-background p-5">
          <main className="mixer-shell mx-auto grid h-[calc(100vh-40px)] min-h-[560px] max-w-[1240px] grid-rows-[64px_minmax(0,1fr)] overflow-hidden rounded-lg border border-border bg-background shadow-[0_20px_64px_rgba(0,0,0,0.42)]">
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
