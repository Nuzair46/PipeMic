#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const rootDir = process.cwd();
const VERSION_FILES = {
  packageJson: path.join(rootDir, "package.json"),
  tauriConf: path.join(rootDir, "src-tauri", "tauri.conf.json"),
  cargoTauri: path.join(rootDir, "src-tauri", "Cargo.toml"),
};

const HELP = `Usage:
  yarn version:bump <patch|minor|major>
  yarn version:bump <x.y.z[-prerelease][+build]>
  yarn version:bump --check

Examples:
  yarn version:bump patch
  yarn version:bump minor
  yarn version:bump 0.2.0
`;

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function writeText(filePath, value) {
  fs.writeFileSync(filePath, value, "utf8");
}

function parseJson(filePath) {
  return JSON.parse(readText(filePath));
}

function parseCargoPackageVersion(filePath) {
  const content = readText(filePath);
  const packageSectionMatch = content.match(/\[package\][\s\S]*?(?=\n\[|$)/);
  if (!packageSectionMatch) {
    throw new Error(`Could not find [package] section in ${path.relative(rootDir, filePath)}`);
  }

  const versionMatch = packageSectionMatch[0].match(/^\s*version\s*=\s*"([^"]+)"\s*$/m);
  if (!versionMatch) {
    throw new Error(`Could not find package version in ${path.relative(rootDir, filePath)}`);
  }

  return versionMatch[1];
}

function replaceCargoPackageVersion(filePath, nextVersion) {
  const content = readText(filePath);
  const packageSectionMatch = content.match(/\[package\][\s\S]*?(?=\n\[|$)/);
  if (!packageSectionMatch || packageSectionMatch.index == null) {
    throw new Error(`Could not find [package] section in ${path.relative(rootDir, filePath)}`);
  }

  const section = packageSectionMatch[0];
  const nextSection = section.replace(
    /^(\s*version\s*=\s*")([^"]+)("\s*)$/m,
    `$1${nextVersion}$3`,
  );

  if (section === nextSection) {
    throw new Error(`Could not update package version in ${path.relative(rootDir, filePath)}`);
  }

  const start = packageSectionMatch.index;
  const end = start + section.length;
  writeText(filePath, `${content.slice(0, start)}${nextSection}${content.slice(end)}`);
}

function readVersions() {
  return {
    "package.json": String(parseJson(VERSION_FILES.packageJson).version ?? ""),
    "src-tauri/Cargo.toml": parseCargoPackageVersion(VERSION_FILES.cargoTauri),
    "src-tauri/tauri.conf.json": String(parseJson(VERSION_FILES.tauriConf).version ?? ""),
  };
}

function ensureVersionsSynced(versions) {
  const unique = [...new Set(Object.values(versions))];
  if (unique.length !== 1) {
    const lines = Object.entries(versions)
      .map(([file, version]) => `  ${file}: ${version}`)
      .join("\n");
    throw new Error(`Version mismatch across files:\n${lines}`);
  }

  return unique[0];
}

function parseSemver(version) {
  const numericIdentifier = "0|[1-9]\\d*";
  const nonNumericIdentifier = "\\d*[A-Za-z-][0-9A-Za-z-]*";
  const prereleaseIdentifier = `(?:${numericIdentifier}|${nonNumericIdentifier})`;
  const prerelease = `${prereleaseIdentifier}(?:\\.${prereleaseIdentifier})*`;
  const build = "[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*";
  const semverPattern = new RegExp(
    `^(${numericIdentifier})\\.(${numericIdentifier})\\.(${numericIdentifier})(?:-(${prerelease}))?(?:\\+(${build}))?$`,
  );
  const match = version.match(semverPattern);
  if (!match) {
    throw new Error(`Invalid semver version: ${version}`);
  }

  return {
    major: Number.parseInt(match[1], 10),
    minor: Number.parseInt(match[2], 10),
    patch: Number.parseInt(match[3], 10),
    prerelease: match[4] ?? null,
    build: match[5] ?? null,
  };
}

function bumpVersion(currentVersion, kind) {
  const parsed = parseSemver(currentVersion);
  if (kind === "major") {
    return `${parsed.major + 1}.0.0`;
  }
  if (kind === "minor") {
    return `${parsed.major}.${parsed.minor + 1}.0`;
  }
  if (kind === "patch") {
    return `${parsed.major}.${parsed.minor}.${parsed.patch + 1}`;
  }

  throw new Error(`Unsupported bump kind: ${kind}`);
}

function resolveNextVersion(currentVersion, arg) {
  if (["major", "minor", "patch"].includes(arg)) {
    return bumpVersion(currentVersion, arg);
  }

  parseSemver(arg);
  return arg;
}

function updateJsonVersion(filePath, nextVersion) {
  const content = parseJson(filePath);
  content.version = nextVersion;
  writeText(filePath, `${JSON.stringify(content, null, 2)}\n`);
}

function main() {
  const arg = process.argv[2];

  if (!arg || arg === "--help" || arg === "-h") {
    process.stdout.write(HELP);
    process.exit(arg ? 0 : 1);
  }

  const versions = readVersions();
  const currentVersion = ensureVersionsSynced(versions);
  parseSemver(currentVersion);

  if (arg === "--check") {
    console.log(`Versions are synced: ${currentVersion}`);
    return;
  }

  const nextVersion = resolveNextVersion(currentVersion, arg);
  if (nextVersion === currentVersion) {
    console.log(`Version already ${currentVersion}; no changes made.`);
    return;
  }

  updateJsonVersion(VERSION_FILES.packageJson, nextVersion);
  replaceCargoPackageVersion(VERSION_FILES.cargoTauri, nextVersion);
  updateJsonVersion(VERSION_FILES.tauriConf, nextVersion);

  console.log(`Bumped version: ${currentVersion} -> ${nextVersion}`);
  console.log("Updated:");
  for (const file of [
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
  ]) {
    console.log(`  - ${file}`);
  }
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
