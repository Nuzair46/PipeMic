import { cn } from "@/lib/utils";

type LevelMeterProps = {
  value: number;
  muted?: boolean;
  compact?: boolean;
  className?: string;
};

const METER_FLOOR_DB = -50;

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

  return (
    <div
      className={cn(
        "meter-grid relative overflow-hidden rounded-sm border border-border bg-[#060a0c] shadow-meter",
        compact ? "h-3" : "h-7",
        className,
      )}
      aria-label="Level meter"
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(visualLevel * 100)}
    >
      <div
        className={cn(
          "absolute inset-y-0 left-0 transition-[width,background-color] duration-75 ease-out",
          muted && "bg-muted-foreground/20",
          !muted && !warn && "bg-primary",
          !muted && warn && !danger && "bg-accent",
          !muted && danger && "bg-destructive",
        )}
        style={{ width: `${visualLevel * 100}%` }}
      />
      <div className="absolute inset-y-0 right-[18%] w-px bg-accent/55" />
      <div className="absolute inset-y-0 right-[4%] w-px bg-destructive/65" />
    </div>
  );
}
