import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatGain(value: number) {
  return `${Math.round(value * 100)}%`;
}

export function formatPid(pid?: number | null) {
  return pid ? `PID ${pid}` : "No PID";
}

