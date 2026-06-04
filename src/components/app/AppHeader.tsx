import { Circle, ExternalLink, Play, Settings, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { UpdateCheckResult } from "@/lib/api";
import { cn } from "@/lib/utils";

type AppHeaderProps = {
  running: boolean;
  canStart: boolean;
  update: UpdateCheckResult;
  onStart: () => void;
  onStop: () => void;
  onSettings: () => void;
  onOpenUpdate: () => void;
};

export function AppHeader({ running, canStart, update, onStart, onStop, onSettings, onOpenUpdate }: AppHeaderProps) {
  return (
    <header className="flex h-16 items-center justify-between border-b border-border bg-background px-5">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="truncate text-lg font-semibold text-foreground">PipeMic</h1>
          <span className="shrink-0 rounded-sm border border-border bg-muted/25 px-1.5 py-0.5 font-mono text-[11px] leading-4 text-muted-foreground">
            v{update.currentVersion}
          </span>
          {update.status === "updateAvailable" ? (
            <button
              type="button"
              className="inline-flex h-6 shrink-0 items-center gap-1 rounded-sm border border-border bg-secondary px-2 text-xs font-medium text-foreground transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onClick={onOpenUpdate}
              title={update.latestVersion ? `Open PipeMic ${update.latestVersion} release` : "Open PipeMic releases"}
            >
              <span>Update available</span>
              <ExternalLink className="h-3 w-3" />
            </button>
          ) : null}
        </div>
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
