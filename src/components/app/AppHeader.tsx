import { Circle, Play, Settings, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type AppHeaderProps = {
  running: boolean;
  canStart: boolean;
  onStart: () => void;
  onStop: () => void;
  onSettings: () => void;
};

export function AppHeader({ running, canStart, onStart, onStop, onSettings }: AppHeaderProps) {
  return (
    <header className="flex h-16 items-center justify-between border-b border-border bg-background px-5">
      <div className="min-w-0">
        <h1 className="truncate text-lg font-semibold text-foreground">PipeMic</h1>
        <p className="truncate text-xs text-muted-foreground">Multi-source mixer</p>
      </div>
      <div className="flex shrink-0 items-center gap-3">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Circle className={cn("h-2.5 w-2.5 fill-current", running ? "text-foreground" : "text-muted-foreground")} />
          <span>{running ? "Live" : "Idle"}</span>
        </div>
        <Button type="button" variant={running ? "danger" : "default"} disabled={!running && !canStart} onClick={running ? onStop : onStart}>
          {running ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          {running ? "Stop" : "Start"}
        </Button>
        <Button type="button" variant="ghost" size="icon" onClick={onSettings}>
          <Settings className="h-4 w-4" />
          <span className="sr-only">Settings</span>
        </Button>
      </div>
    </header>
  );
}
