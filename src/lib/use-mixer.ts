import { useEffect, useState, useSyncExternalStore } from "react";
import { api } from "./api";
import { MixerController } from "./controller";

export function useMixer(onError: (title: string, message: string) => void) {
  const [controller] = useState(() => new MixerController(api, onError));
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);

  useEffect(() => {
    let disposed = false;
    let statusTimer: ReturnType<typeof setTimeout>;
    let discoveryTimer: ReturnType<typeof setTimeout>;
    const status = async () => {
      await controller.pollStatus();
      if (!disposed) statusTimer = setTimeout(status, controller.getSnapshot().status.state === "running" ? 80 : 1000);
    };
    const discovery = async () => {
      await controller.refresh();
      if (!disposed) discoveryTimer = setTimeout(discovery, 1200);
    };
    const focus = () => { void controller.refresh(); };
    void status();
    void discovery();
    window.addEventListener("focus", focus);
    return () => {
      disposed = true;
      clearTimeout(statusTimer);
      clearTimeout(discoveryTimer);
      window.removeEventListener("focus", focus);
    };
  }, [controller]);

  return { ...state, controller };
}
