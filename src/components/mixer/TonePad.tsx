import { useEffect, useId, useRef, type KeyboardEvent, type PointerEvent } from "react";
import { Button } from "@/components/ui/button";
import type { ToneConfig } from "@/lib/types";

type TonePadProps = {
  sourceName: string;
  tone: ToneConfig;
  onChange: (tone: ToneConfig) => void;
};

function axisText(value: number, negative: string, positive: string) {
  return value === 0 ? "Neutral" : `${value < 0 ? negative : positive} ${Math.round(Math.abs(value) * 100)}%`;
}

export function TonePad({ sourceName, tone, onChange }: TonePadProps) {
  const id = useId();
  const drag = useRef<number | null>(null);
  const frame = useRef<number>();
  const pending = useRef<ToneConfig>();
  const latest = useRef({ tone, onChange });
  latest.current = { tone, onChange };

  const flush = () => {
    cancelAnimationFrame(frame.current ?? 0);
    frame.current = undefined;
    if (pending.current) {
      const next = pending.current;
      pending.current = undefined;
      latest.current.tone = next;
      latest.current.onChange(next);
    }
  };

  // Removal must not drop the last position waiting for a paint.
  useEffect(() => () => flush(), []);

  const setTone = (next: ToneConfig) => {
    pending.current = next;
    flush();
  };

  const coordinate = (value: number) => Math.round(Math.max(-1, Math.min(1, value)) * 100) / 100;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = { ...(pending.current ?? latest.current.tone) };
    const step = event.shiftKey ? 0.1 : 0.01;
    switch (event.key) {
      case "ArrowLeft": next.x = coordinate(next.x - step); break;
      case "ArrowRight": next.x = coordinate(next.x + step); break;
      case "ArrowDown": next.y = coordinate(next.y - step); break;
      case "ArrowUp": next.y = coordinate(next.y + step); break;
      case "Home": next.x = 0; next.y = 0; break;
      default: return;
    }
    event.preventDefault();
    setTone(next);
  };

  const move = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current !== event.pointerId) return;
    const rect = event.currentTarget.getBoundingClientRect();
    pending.current = {
      ...latest.current.tone,
      x: coordinate(2 * (event.clientX - rect.left) / rect.width - 1),
      y: coordinate(1 - 2 * (event.clientY - rect.top) / rect.height),
    };
    if (frame.current === undefined) frame.current = requestAnimationFrame(flush);
  };

  return (
    <div className="tone-controls grid gap-1">
      <p id={`${id}-help`} className="sr-only">
        Left and Right adjust Lo and Hi. Down and Up cut and boost Mid.
        Hold Shift for larger steps. Home resets to neutral.
      </p>
      <div className={`tone-pad-layout ${tone.bypassed ? "opacity-50" : ""}`}>
        <span className="col-start-2 text-center" aria-hidden="true">Mid +</span>
        <span className="row-start-2 self-center" aria-hidden="true">Lo</span>
        <div
          className="tone-pad relative col-start-2 row-start-2 aspect-square touch-none select-none border border-border bg-muted/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          role="slider"
          tabIndex={0}
          aria-label={`Tone for ${sourceName}`}
          aria-roledescription="two-dimensional tone pad"
          aria-describedby={`${id}-help`}
          aria-valuemin={-1}
          aria-valuemax={1}
          aria-valuenow={tone.x}
          aria-valuetext={`${axisText(tone.x, "Lo", "Hi")}, ${axisText(tone.y, "Mid cut", "Mid boost")}${tone.bypassed ? ", bypassed" : ""}`}
          onKeyDown={onKeyDown}
          onPointerDown={(event) => {
            if (!event.isPrimary || event.button !== 0) return;
            event.preventDefault();
            event.currentTarget.focus({ preventScroll: true });
            drag.current = event.pointerId;
            event.currentTarget.setPointerCapture(event.pointerId);
            move(event);
          }}
          onPointerMove={move}
          onPointerUp={(event) => {
            if (drag.current !== event.pointerId) return;
            move(event);
            flush();
            drag.current = null;
            event.currentTarget.releasePointerCapture(event.pointerId);
          }}
          onPointerCancel={() => { flush(); drag.current = null; }}
          onLostPointerCapture={() => { flush(); drag.current = null; }}
        >
          <span className="pointer-events-none absolute inset-x-0 top-1/2 border-t border-dashed border-border" />
          <span className="pointer-events-none absolute inset-y-0 left-1/2 border-l border-dashed border-border" />
          <span
            className="tone-pad-handle pointer-events-none absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-background bg-primary ring-1 ring-primary"
            style={{ left: `${(tone.x + 1) * 50}%`, top: `${(1 - tone.y) * 50}%` }}
          />
        </div>
        <span className="col-start-3 row-start-2 self-center text-right" aria-hidden="true">Hi</span>
        <span className="col-start-2 row-start-3 text-center" aria-hidden="true">Mid −</span>
      </div>
      <div className="flex items-center justify-center gap-1">
        <Button type="button" variant="quiet" className="h-6 px-2 text-xs" onClick={() => setTone({ ...latest.current.tone, x: 0, y: 0 })}>Reset</Button>
        <Button
          type="button" variant={tone.bypassed ? "secondary" : "quiet"} className="h-6 px-2 text-xs"
          aria-pressed={tone.bypassed} onClick={() => {
            const current = pending.current ?? latest.current.tone;
            setTone({ ...current, bypassed: !current.bypassed });
          }}
        >Bypass</Button>
      </div>
    </div>
  );
}
