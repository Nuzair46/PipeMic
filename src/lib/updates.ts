import { getVersion } from "@tauri-apps/api/app";
import packageInfo from "../../package.json";
import type { UpdateCheckResult } from "./types";
import { isTauri } from "./transport";

export const sourceUrl = "https://github.com/nuzair46/pipemic";
export const releasesUrl = `${sourceUrl}/releases`;
const latestReleaseApiUrl = "https://api.github.com/repos/nuzair46/pipemic/releases/latest";
const fallbackAppVersion = String(packageInfo.version ?? "0.0.0");

export const initialUpdateCheck: UpdateCheckResult = {
  status: "checking",
  currentVersion: fallbackAppVersion,
};

type ParsedSemver = {
  major: number;
  minor: number;
  patch: number;
  prerelease: string | null;
};

type GithubLatestRelease = {
  tag_name?: unknown;
  html_url?: unknown;
  draft?: unknown;
  prerelease?: unknown;
};

function parseSemver(value: string): ParsedSemver | null {
  const match = value
    .trim()
    .replace(/^v/i, "")
    .match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/);
  if (!match) {
    return null;
  }

  return {
    major: Number.parseInt(match[1], 10),
    minor: Number.parseInt(match[2], 10),
    patch: Number.parseInt(match[3], 10),
    prerelease: match[4] ?? null,
  };
}

function normalizeVersion(value: unknown) {
  return typeof value === "string" && parseSemver(value) ? value.trim().replace(/^v/i, "") : "";
}

export function compareSemverVersions(left: string, right: string) {
  const parsedLeft = parseSemver(left);
  const parsedRight = parseSemver(right);
  if (!parsedLeft || !parsedRight) {
    return 0;
  }

  for (const key of ["major", "minor", "patch"] as const) {
    const diff = parsedLeft[key] - parsedRight[key];
    if (diff !== 0) {
      return diff;
    }
  }

  if (parsedLeft.prerelease === parsedRight.prerelease) {
    return 0;
  }
  if (!parsedLeft.prerelease) {
    return 1;
  }
  if (!parsedRight.prerelease) {
    return -1;
  }
  return parsedLeft.prerelease.localeCompare(parsedRight.prerelease);
}

export function safeReleaseUrl(value: unknown) {
  if (typeof value !== "string") {
    return releasesUrl;
  }

  return value.startsWith(releasesUrl) ? value : releasesUrl;
}

export async function currentAppVersion() {
  if (!isTauri()) {
    return fallbackAppVersion;
  }

  try {
    return await getVersion();
  } catch {
    return fallbackAppVersion;
  }
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  const currentVersion = await currentAppVersion();

  try {
    const response = await fetch(latestReleaseApiUrl, {
      headers: { Accept: "application/vnd.github+json" },
      cache: "no-store",
    });
    if (!response.ok) {
      return { status: "unavailable", currentVersion };
    }

    const release = (await response.json()) as GithubLatestRelease;
    if (release.draft || release.prerelease) {
      return { status: "current", currentVersion };
    }

    const latestVersion = normalizeVersion(release.tag_name);
    if (!latestVersion) {
      return { status: "unavailable", currentVersion };
    }

    const releaseUrl = safeReleaseUrl(release.html_url);
    return compareSemverVersions(currentVersion, latestVersion) < 0
      ? { status: "updateAvailable", currentVersion, latestVersion, releaseUrl }
      : { status: "current", currentVersion, latestVersion, releaseUrl };
  } catch {
    return { status: "unavailable", currentVersion };
  }
}
