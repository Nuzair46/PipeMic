import type { AppSourceConfig, AudioSession } from "@/lib/api";

const SELF_EXECUTABLES = new Set(["pipemic.exe"]);

export function isSelectableSession(session: AudioSession) {
  const executable = session.executable.trim();
  const normalized = executable.toLowerCase();

  return (
    session.state !== "expired" &&
    normalized.endsWith(".exe") &&
    !SELF_EXECUTABLES.has(normalized) &&
    !executable.includes("\\") &&
    !executable.includes("/")
  );
}

export function sessionForExecutable(sessions: AudioSession[], executable: string) {
  const matches = sessions.filter((session) => session.executable.toLowerCase() === executable.toLowerCase());
  return matches.find((session) => session.state === "active") ?? matches[0] ?? null;
}

export function executableLabel(executable: string) {
  return executable.replace(/\.exe$/i, "");
}

export function sessionDisplayLabel(session: AudioSession) {
  const title = session.windowTitle?.trim();
  if (title) {
    return title;
  }

  return session.displayName || executableLabel(session.executable);
}

export function sourceDisplayLabel(source: AppSourceConfig, session: AudioSession | null) {
  return session ? sessionDisplayLabel(session) : source.displayName ?? executableLabel(source.executable);
}

export function savedDisplayName(session: AudioSession) {
  return session.displayName || executableLabel(session.executable);
}

export function uniqueSessionsByExecutable(sessions: AudioSession[]) {
  const seen = new Set<string>();

  return sessions.filter((session) => {
    const key = session.executable.toLowerCase();
    if (seen.has(key)) {
      return false;
    }

    seen.add(key);
    return true;
  });
}
