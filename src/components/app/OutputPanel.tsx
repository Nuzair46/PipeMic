import { PanelRight } from "lucide-react";
import type { AudioDevice } from "@/lib/api";
import { DeviceSelect } from "@/components/mixer/DeviceSelect";
import { LevelMeter } from "@/components/mixer/LevelMeter";
import { Slider } from "@/components/ui/slider";
import { cn, formatGain } from "@/lib/utils";

type OutputPanelProps = {
  booting: boolean;
  running: boolean;
  devices: AudioDevice[];
  outputDeviceId: string | null;
  outputGuidance: string;
  outputGuidanceWarns: boolean;
  masterGain: number;
  outputPeak: number;
  onOutputDeviceChange: (outputDeviceId: string) => void;
  onMasterGainChange: (masterGain: number) => void;
};

export function OutputPanel({
  booting,
  running,
  devices,
  outputDeviceId,
  outputGuidance,
  outputGuidanceWarns,
  masterGain,
  outputPeak,
  onOutputDeviceChange,
  onMasterGainChange,
}: OutputPanelProps) {
  return (
    <aside className="grid min-h-0 min-w-0 grid-rows-[44px_minmax(0,1fr)] bg-muted/10">
      <div className="panel-heading">
        <div className="flex min-w-0 items-center gap-2">
          <PanelRight className="h-4 w-4 text-muted-foreground" />
          <h2 className="truncate text-sm font-medium text-foreground">Output</h2>
        </div>
      </div>

      <div className="grid min-h-0 min-w-0 content-start gap-5 overflow-y-auto p-4">
        <DeviceSelect
          label="Virtual Mic"
          placeholder={booting ? "Loading devices" : "Select virtual mic"}
          devices={devices}
          value={outputDeviceId}
          onChange={onOutputDeviceChange}
        />
        <p
          className={cn(
            "rounded-md border px-3 py-2 text-xs leading-5 text-muted-foreground",
            outputGuidanceWarns ? "border-border bg-muted/35 text-foreground" : "border-border bg-background",
          )}
        >
          {outputGuidance}
        </p>

        <div className="grid gap-3 border-y border-border py-4">
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs font-medium text-muted-foreground">Master</span>
            <span className="font-mono text-xs text-foreground">{formatGain(masterGain)}</span>
          </div>
          <Slider
            min={0}
            max={1.5}
            step={0.01}
            value={[masterGain]}
            onDoubleClick={() => onMasterGainChange(1)}
            onValueChange={(value) => onMasterGainChange(value[0] ?? masterGain)}
          />
          <LevelMeter value={outputPeak} muted={!running} />
        </div>
      </div>
    </aside>
  );
}
