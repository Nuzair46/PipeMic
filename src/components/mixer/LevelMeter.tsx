import { cn } from "@/lib/utils";

type LevelMeterProps = {
  value: number;
  muted?: boolean;
  compact?: boolean;
  className?: string;
};

const METER_FLOOR_DB = -50;
const REGULAR_SEGMENTS = 28;
const COMPACT_SEGMENTS = 18;

function clampMeterValue(value: number) {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
}

export function visualMeterLevel(value: number) {
  const normalized = clampMeterValue(value);
  if (normalized <= 0) {
    return 0;
  }

  const decibels = 20 * Math.log10(normalized);
  return Math.max(0, Math.min(1, (decibels - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

export function LevelMeter({ value, muted = false, compact = false, className }: LevelMeterProps) {
  const normalized = clampMeterValue(value);
  const visualLevel = muted ? 0 : visualMeterLevel(normalized);
  const warn = normalized > 0.82;
  const danger = normalized > 0.96;
  const segmentCount = compact ? COMPACT_SEGMENTS : REGULAR_SEGMENTS;
  const activeSegments = Math.ceil(visualLevel * segmentCount);
  const clipStart = Math.max(0, segmentCount - 2);

  return (
    <div
      className={cn(
        "relative grid overflow-hidden rounded-sm border bg-background p-[3px] shadow-meter transition-colors duration-75",
        danger ? "border-destructive/70" : warn ? "border-foreground/35" : "border-border",
        compact ? "h-3" : "h-7",
        className,
      )}
      aria-label="Level meter"
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(visualLevel * 100)}
    >
      <div className="grid h-full min-w-0 grid-flow-col gap-[3px]">
        {Array.from({ length: segmentCount }, (_, index) => {
          const active = index < activeSegments;
          const clippingSegment = danger && index >= clipStart;

          return (
            <span
              key={index}
              className={cn(
                "min-w-0 rounded-[1px] transition-colors duration-75 ease-out",
                active ? "bg-foreground" : "bg-muted/65",
                active && warn && "bg-foreground/80",
                active && clippingSegment && "bg-destructive",
              )}
            />
          );
        })}
      </div>
    </div>
  );
}
