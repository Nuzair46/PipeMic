import { useEffect, useState } from "react";
import { Github } from "lucide-react";
import type { AppConfig, ShortcutConfig } from "@/lib/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";

type SettingsDialogProps = {
  open: boolean;
  config: AppConfig;
  onOpenChange: (open: boolean) => void;
  onConfigChange: (config: AppConfig) => void;
  onSave: () => void;
  onOpenSource: () => void;
};

type ShortcutField = keyof ShortcutConfig;

const shortcutRows: Array<{ key: ShortcutField; label: string }> = [
  { key: "micMute", label: "Mic mute" },
  { key: "appMute", label: "App mute" },
  { key: "routing", label: "Start / Stop" },
];

export function SettingsDialog({ open, config, onOpenChange, onConfigChange, onSave, onOpenSource }: SettingsDialogProps) {
  const [recording, setRecording] = useState<ShortcutField | null>(null);
  const validation = settingsValidationError(config.shortcuts);

  useEffect(() => {
    if (!open) {
      setRecording(null);
    }
  }, [open]);

  useEffect(() => {
    if (!recording) {
      return undefined;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecording(null);
        return;
      }

      const shortcut = shortcutFromEvent(event);
      if (!shortcut) {
        return;
      }

      onConfigChange({
        ...config,
        shortcuts: {
          ...config.shortcuts,
          [recording]: shortcut,
        },
      });
      setRecording(null);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [config, onConfigChange, recording]);

  const setChecked = (key: "startWithWindows" | "minimizeToTray", checked: boolean) => {
    onConfigChange({ ...config, [key]: checked });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Configure shortcuts and startup behavior.</DialogDescription>
        </DialogHeader>

        <div className="grid max-h-[calc(100vh-220px)] gap-5 overflow-y-auto px-5 py-5">
          <section className="grid gap-3">
            <h3 className="text-xs font-medium text-muted-foreground">Shortcuts</h3>
            <div className="grid gap-2">
              {shortcutRows.map((row) => (
                <div
                  key={row.key}
                  className="grid grid-cols-[140px_minmax(0,1fr)_96px] items-center gap-3 rounded-md border border-border bg-muted/20 px-3 py-2"
                >
                  <span className="text-sm text-foreground">{row.label}</span>
                  <kbd className="min-w-0 truncate rounded border border-border bg-background px-2 py-1.5 font-mono text-xs text-foreground">
                    {recording === row.key ? "Recording..." : config.shortcuts[row.key]}
                  </kbd>
                  <Button type="button" variant="ghost" size="sm" onClick={() => setRecording(row.key)}>
                    Record
                  </Button>
                </div>
              ))}
            </div>
            {validation ? <p className="text-xs text-destructive">{validation}</p> : null}
          </section>

          <section className="grid gap-2">
            <h3 className="text-xs font-medium text-muted-foreground">Startup</h3>
            <SwitchRow
              label="Start with Windows"
              checked={config.startWithWindows}
              onCheckedChange={(checked) => setChecked("startWithWindows", checked)}
            />
            <SwitchRow
              label="Minimize to tray"
              checked={config.minimizeToTray}
              onCheckedChange={(checked) => setChecked("minimizeToTray", checked)}
            />
          </section>
        </div>

        <DialogFooter className="justify-between">
          <Button type="button" variant="quiet" size="sm" onClick={onOpenSource}>
            <Github className="h-4 w-4" />
            GitHub
          </Button>
          <div className="flex items-center gap-2">
            <Button type="button" variant="quiet" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="button" disabled={Boolean(validation)} onClick={onSave}>
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SwitchRow({ label, checked, onCheckedChange }: { label: string; checked: boolean; onCheckedChange: (checked: boolean) => void }) {
  return (
    <label className="flex h-11 cursor-pointer items-center justify-between gap-3 rounded-md border border-border bg-muted/20 px-3 text-sm text-foreground">
      <span>{label}</span>
      <Switch checked={checked} onCheckedChange={onCheckedChange} aria-label={label} />
    </label>
  );
}

export function cloneAppConfig(config: AppConfig): AppConfig {
  return {
    ...config,
    micSources: config.micSources.map((source) => ({ ...source })),
    appSources: config.appSources.map((source) => ({ ...source })),
    shortcuts: { ...config.shortcuts },
  };
}

export function settingsValidationError(shortcuts: ShortcutConfig) {
  const values = [shortcuts.micMute, shortcuts.appMute, shortcuts.routing].map((shortcut) => shortcut.trim());
  if (values.some((shortcut) => !shortcut)) {
    return "Shortcut fields cannot be empty.";
  }

  const normalized = values.map((shortcut) => shortcut.toLowerCase());
  if (new Set(normalized).size !== normalized.length) {
    return "Each shortcut must be unique.";
  }

  return "";
}

function shortcutFromEvent(event: KeyboardEvent) {
  const key = shortcutKeyName(event.key);
  if (!key || key === "Ctrl" || key === "Alt" || key === "Shift") {
    return "";
  }

  const parts = [];
  if (event.ctrlKey) {
    parts.push("Ctrl");
  }
  if (event.altKey) {
    parts.push("Alt");
  }
  if (event.shiftKey) {
    parts.push("Shift");
  }
  if (!parts.length) {
    return "";
  }
  parts.push(key);
  return parts.join("+");
}

function shortcutKeyName(key: string) {
  if (key.length === 1) {
    return key.toUpperCase();
  }

  const normalized = key.trim();
  if (!normalized) {
    return "";
  }
  if (normalized === "Control") {
    return "Ctrl";
  }
  if (normalized === " ") {
    return "Space";
  }
  return normalized;
}
