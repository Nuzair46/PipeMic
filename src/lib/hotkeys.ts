import { register, unregister, unregisterAll, type ShortcutEvent } from "@tauri-apps/plugin-global-shortcut";
import type { ShortcutConfig } from "@/lib/api";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type HotkeyHandlers = {
  toggleMicMute: () => void;
  toggleAppMute: () => void;
  toggleRouting: () => void;
};

type HotkeyAction = "mic" | "app" | "routing";

type HotkeyDefinition = {
  accelerator: string;
  key: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
};

const tauriPresent = () => typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

export async function registerHotkeys(shortcuts: ShortcutConfig, handlers: HotkeyHandlers) {
  const hotkeys = hotkeyDefinitions(shortcuts);
  const registerLocalFallback = (actions: HotkeyAction[]) => {
    const definitions = actions.map((action) => ({ action, definition: hotkeys[action] }));
    const onKeyDown = (event: KeyboardEvent) => {
      const match = definitions.find(({ definition }) => eventMatchesDefinition(event, definition));
      if (match) {
        event.preventDefault();
        runHotkeyAction(match.action, handlers);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  };

  const actions = Object.keys(hotkeys) as HotkeyAction[];

  if (tauriPresent()) {
    await unregisterAll().catch(() => undefined);

    const registered: string[] = [];
    const fallbackActions: HotkeyAction[] = [];
    for (const action of actions) {
      const { accelerator } = hotkeys[action];
      try {
        await register(accelerator, (event: ShortcutEvent) => {
          if (event.state === "Pressed") {
            runHotkeyAction(action, handlers);
          }
        });
        registered.push(accelerator);
      } catch {
        fallbackActions.push(action);
      }
    }

    const cleanupFallback = fallbackActions.length ? registerLocalFallback(fallbackActions) : undefined;
    return () => {
      cleanupFallback?.();
      if (registered.length) {
        void unregister(registered);
      }
    };
  }

  return registerLocalFallback(actions);
}

function hotkeyDefinitions(shortcuts: ShortcutConfig): Record<HotkeyAction, HotkeyDefinition> {
  return {
    mic: parseAccelerator(shortcuts.micMute),
    app: parseAccelerator(shortcuts.appMute),
    routing: parseAccelerator(shortcuts.routing),
  };
}

function parseAccelerator(accelerator: string): HotkeyDefinition {
  const parts = accelerator.split("+").map((part) => part.trim()).filter(Boolean);
  const key = (parts[parts.length - 1] ?? "").toLowerCase();
  const modifiers = new Set(parts.slice(0, -1).map((part) => part.toLowerCase()));
  return {
    accelerator,
    key,
    ctrl: modifiers.has("ctrl") || modifiers.has("control"),
    alt: modifiers.has("alt") || modifiers.has("option"),
    shift: modifiers.has("shift"),
  };
}

function eventMatchesDefinition(event: KeyboardEvent, definition: HotkeyDefinition) {
  return (
    event.key.toLowerCase() === definition.key &&
    event.ctrlKey === definition.ctrl &&
    event.altKey === definition.alt &&
    event.shiftKey === definition.shift
  );
}

function runHotkeyAction(action: HotkeyAction, handlers: HotkeyHandlers) {
  if (action === "mic") {
    handlers.toggleMicMute();
  } else if (action === "app") {
    handlers.toggleAppMute();
  } else {
    handlers.toggleRouting();
  }
}
