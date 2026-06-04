import { useState, type ReactNode } from "react";
import { Mic, Plus, SlidersHorizontal, Volume2 } from "lucide-react";
import type { AppSourceConfig, AudioDevice, AudioSession, LevelMeters, MicSourceConfig } from "@/lib/api";
import { SourceStrip } from "@/components/mixer/SourceStrip";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { sessionDisplayLabel, sessionForExecutable, sourceDisplayLabel } from "@/components/app/source-labels";

type SourceOption = {
  value: string;
  label: string;
  detail?: string;
};

type SourcesPanelProps = {
  booting: boolean;
  captureDevices: AudioDevice[];
  selectableSessions: AudioSession[];
  availableMicDevices: AudioDevice[];
  availableAppSessions: AudioSession[];
  micSources: MicSourceConfig[];
  appSources: AppSourceConfig[];
  meters: LevelMeters;
  onAddMicSource: (deviceId: string) => void;
  onAddAppSource: (sessionId: string) => void;
  onMicSourceChange: (sourceId: string, patch: Partial<Pick<MicSourceConfig, "gain" | "muted">>) => void;
  onAppSourceChange: (sourceId: string, patch: Partial<Pick<AppSourceConfig, "gain" | "muted">>) => void;
  onRemoveMicSource: (sourceId: string) => void;
  onRemoveAppSource: (sourceId: string) => void;
};

export function SourcesPanel({
  booting,
  captureDevices,
  selectableSessions,
  availableMicDevices,
  availableAppSessions,
  micSources,
  appSources,
  meters,
  onAddMicSource,
  onAddAppSource,
  onMicSourceChange,
  onAppSourceChange,
  onRemoveMicSource,
  onRemoveAppSource,
}: SourcesPanelProps) {
  return (
    <section className="grid min-h-0 min-w-0 grid-rows-[44px_minmax(0,1fr)_minmax(0,1fr)] border-r border-border bg-background">
      <div className="panel-heading">
        <div className="flex min-w-0 items-center gap-2">
          <SlidersHorizontal className="h-4 w-4 text-muted-foreground" />
          <h2 className="truncate text-sm font-medium text-foreground">Sources</h2>
        </div>
      </div>

      <div className="grid min-h-0 grid-rows-[48px_minmax(0,1fr)] border-b border-border">
        <SourceSectionHeader
          icon={<Mic className="h-4 w-4 text-muted-foreground" />}
          title="Physical Microphones"
          addDisabled={!availableMicDevices.length}
          addLabel="Microphone"
          onAdd={onAddMicSource}
          options={availableMicDevices.map((device) => ({
            value: device.id,
            label: device.name,
            detail: device.isDefault ? "Default" : undefined,
          }))}
        />
        <div className="min-h-0 overflow-y-auto overscroll-contain">
          {micSources.length ? (
            micSources.map((source) => {
              const device = captureDevices.find((item) => item.id === source.deviceId) ?? null;

              return (
                <SourceStrip
                  key={source.id}
                  kind="mic"
                  title={device?.name ?? source.deviceId}
                  detail={device ? undefined : "Device unavailable"}
                  gain={source.gain}
                  muted={source.muted}
                  level={meters.micPeaks[source.id] ?? 0}
                  inactive={!device}
                  onGainChange={(gain) => onMicSourceChange(source.id, { gain })}
                  onMutedChange={(muted) => onMicSourceChange(source.id, { muted })}
                  onRemove={() => onRemoveMicSource(source.id)}
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
          icon={<Volume2 className="h-4 w-4 text-muted-foreground" />}
          title="Application Sources"
          addDisabled={!availableAppSessions.length}
          addLabel="Application"
          onAdd={onAddAppSource}
          options={availableAppSessions.map((session) => ({
            value: session.id,
            label: sessionDisplayLabel(session),
            detail: session.executable,
          }))}
        />
        <div className="min-h-0 overflow-y-auto overscroll-contain">
          {appSources.length ? (
            appSources.map((source) => {
              const session = sessionForExecutable(selectableSessions, source.executable);
              const active = session?.state === "active";

              return (
                <SourceStrip
                  key={source.id}
                  kind="app"
                  title={sourceDisplayLabel(source, session)}
                  detail={undefined}
                  gain={source.gain}
                  muted={source.muted}
                  level={meters.appPeaks[source.id] ?? 0}
                  inactive={!active}
                  onGainChange={(gain) => onAppSourceChange(source.id, { gain })}
                  onMutedChange={(muted) => onAppSourceChange(source.id, { muted })}
                  onRemove={() => onRemoveAppSource(source.id)}
                />
              );
            })
          ) : (
            <EmptySourceRow label={booting ? "Loading applications" : "No applications added"} />
          )}
        </div>
      </div>
    </section>
  );
}

type SourceSectionHeaderProps = {
  icon: ReactNode;
  title: string;
  options: SourceOption[];
  addDisabled: boolean;
  addLabel: string;
  onAdd: (value: string) => void;
};

function SourceSectionHeader({ icon, title, options, addDisabled, addLabel, onAdd }: SourceSectionHeaderProps) {
  const [value, setValue] = useState("");

  return (
    <div className="flex h-12 min-w-0 items-center justify-between gap-3 border-b border-border bg-muted/25 px-4">
      <div className="flex min-w-0 items-center gap-2">
        {icon}
        <h3 className="truncate text-xs font-medium text-muted-foreground">{title}</h3>
      </div>
      <Select
        value={value}
        onValueChange={(next) => {
          onAdd(next);
          setValue("");
        }}
      >
        <SelectTrigger className="h-8 w-[156px] min-w-0 px-2 text-xs" disabled={addDisabled}>
          <Plus className="h-4 w-4 text-muted-foreground" />
          <SelectValue placeholder={addLabel} />
        </SelectTrigger>
        <SelectContent className="w-[min(360px,calc(100vw-32px))]">
          <SelectGroup>
            <SelectLabel>{title}</SelectLabel>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                <span className="grid min-w-0 max-w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 overflow-hidden">
                  <span className="block min-w-0 truncate" title={option.label}>
                    {option.label}
                  </span>
                  {option.detail ? (
                    <span className="block max-w-[112px] truncate text-xs text-muted-foreground" title={option.detail}>
                      {option.detail}
                    </span>
                  ) : null}
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
      <span className="truncate">{label}</span>
    </div>
  );
}
