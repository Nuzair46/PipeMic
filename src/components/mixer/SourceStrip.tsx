import { Mic, Trash2, Volume2, VolumeX } from "lucide-react";
import { Button } from "@/components/ui/button";
import { LevelMeter } from "@/components/mixer/LevelMeter";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatGain } from "@/lib/utils";

type SourceStripProps = {
  kind: "mic" | "app";
  title: string;
  detail?: string;
  gain: number;
  muted: boolean;
  level: number;
  inactive?: boolean;
  onGainChange: (gain: number) => void;
  onMutedChange: (muted: boolean) => void;
  onRemove?: () => void;
};

export function SourceStrip({
  kind,
  title,
  detail,
  gain,
  muted,
  level,
  inactive = false,
  onGainChange,
  onMutedChange,
  onRemove,
}: SourceStripProps) {
  const Icon = kind === "mic" ? Mic : Volume2;

  return (
    <div className="grid h-[96px] grid-cols-[minmax(0,1fr)_210px_84px] items-center gap-4 border-b border-border px-4 last:border-b-0">
      <div className="grid min-w-0 grid-cols-[36px_minmax(0,1fr)] gap-3">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={muted ? "danger" : "ghost"}
              size="icon"
              type="button"
              className="h-9 w-9"
              onClick={() => onMutedChange(!muted)}
            >
              {muted ? <VolumeX className="h-4 w-4" /> : <Icon className="h-5 w-5 text-primary" />}
              <span className="sr-only">{muted ? "Unmute" : "Mute"}</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>{muted ? "Unmute" : "Mute"}</TooltipContent>
        </Tooltip>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-foreground">{title}</h3>
          </div>
          {detail ? <p className="mt-1 truncate text-xs text-muted-foreground">{detail}</p> : null}
          <LevelMeter className={detail ? "mt-3" : "mt-5"} value={level} muted={muted || inactive} />
        </div>
      </div>

      <div className="grid gap-2">
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

      <div className="flex items-center justify-end gap-2">
        {onRemove ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="quiet" size="icon" type="button" onClick={onRemove}>
                <Trash2 className="h-4 w-4" />
                <span className="sr-only">Remove source</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>Remove source</TooltipContent>
          </Tooltip>
        ) : null}
      </div>
    </div>
  );
}
