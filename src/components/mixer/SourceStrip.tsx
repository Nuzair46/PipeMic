import { Mic, Trash2, Volume2, VolumeX } from "lucide-react";
import { Button } from "@/components/ui/button";
import { LevelMeter } from "@/components/mixer/LevelMeter";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatGain } from "@/lib/utils";
import { TonePad } from "./TonePad";
import type { ToneConfig } from "@/lib/types";

type SourceStripProps = {
  kind: "mic" | "app";
  title: string;
  detail?: string;
  gain: number;
  muted: boolean;
  tone: ToneConfig;
  level: number;
  inactive?: boolean;
  onGainChange: (gain: number) => void;
  onMutedChange: (muted: boolean) => void;
  onToneChange: (tone: ToneConfig) => void;
  onRemove?: () => void;
};

export function SourceStrip({
  kind,
  title,
  detail,
  gain,
  muted,
  tone,
  level,
  inactive = false,
  onGainChange,
  onMutedChange,
  onToneChange,
  onRemove,
}: SourceStripProps) {
  const Icon = kind === "mic" ? Mic : Volume2;

  return (
    <div className="source-strip grid min-w-0 grid-cols-[minmax(0,1fr)_148px] items-center gap-5 border-b border-border px-4 py-2 last:border-b-0">
      <div className="grid min-w-0 gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant={muted ? "danger" : "ghost"}
                size="icon"
                type="button"
                className="h-8 w-8 shrink-0"
                onClick={() => onMutedChange(!muted)}
              >
                {muted ? <VolumeX className="h-4 w-4" /> : <Icon className="h-5 w-5 text-primary" />}
                <span className="sr-only">{muted ? "Unmute" : "Mute"}</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>{muted ? "Unmute" : "Mute"}</TooltipContent>
          </Tooltip>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-sm font-semibold text-foreground" title={title}>
              {title}
            </h3>
            {detail ? (
              <p className="mt-1 truncate text-xs text-muted-foreground" title={detail}>
                {detail}
              </p>
            ) : null}
          </div>
          {onRemove ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="quiet" size="icon" type="button" className="h-8 w-8 shrink-0" onClick={onRemove}>
                  <Trash2 className="h-4 w-4" />
                  <span className="sr-only">Remove source</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent>Remove source</TooltipContent>
            </Tooltip>
          ) : null}
        </div>

        <LevelMeter value={level} muted={muted || inactive} />

        <div className="grid min-w-0 gap-1">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>Gain</span>
            <span className="font-mono text-foreground">{formatGain(gain)}</span>
          </div>
          <Slider
            min={0}
            max={2}
            step={0.01}
            value={[gain]}
            onDoubleClick={() => onGainChange(1)}
            onValueChange={(value) => onGainChange(value[0] ?? gain)}
          />
        </div>
      </div>
      <TonePad sourceName={title} tone={tone} onChange={onToneChange} />
    </div>
  );
}
