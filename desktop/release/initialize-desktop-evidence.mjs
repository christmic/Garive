#!/usr/bin/env node
import { execFile as execFileCallback } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, open, readFile, realpath, rename, rm } from "node:fs/promises";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFile = promisify(execFileCallback);
const releaseDir = dirname(fileURLToPath(import.meta.url));
const repository = resolve(releaseDir, "../..");
const positional = process.argv.slice(2).filter((argument) => argument !== "--dry-run");
const dryRun = process.argv.includes("--dry-run");
if (positional.length < 2 || positional.length > 3) {
  throw new Error("usage: initialize-desktop-evidence.mjs <target/*.dmg> <macOS-version> [manifest] [--dry-run]");
}
const [packageArgument, macosVersion, manifestArgument] = positional;
if (!/^\d+\.\d+(?:\.\d+)?$/.test(macosVersion)) throw new Error("invalid macOS version");

const targetRoot = await realpath(resolve(repository, "target"));
const unresolvedPackagePath = resolve(repository, packageArgument);
if ((await lstat(unresolvedPackagePath)).isSymbolicLink()) {
  throw new Error("candidate must not be a symbolic link");
}
const packagePath = await realpath(unresolvedPackagePath);
if (!packagePath.startsWith(`${targetRoot}${sep}`) || !packagePath.endsWith(".dmg")) {
  throw new Error("candidate must be a DMG under this repository's target directory");
}

const manifestPath = resolve(repository,
  manifestArgument ?? "docs/evidence/desktop-capture-manifest.json");
const evidenceRoot = resolve(repository, "docs/evidence");
if (!manifestPath.startsWith(`${evidenceRoot}${sep}`) || !manifestPath.endsWith(".json")) {
  throw new Error("manifest must be JSON under docs/evidence");
}
const manifestStat = await lstat(manifestPath);
if (manifestStat.isSymbolicLink()) throw new Error("manifest must not be a symbolic link");
const originalBytes = await readFile(manifestPath);
const manifest = JSON.parse(originalBytes.toString("utf8"));
if (manifest.schema_version !== 1 || !Array.isArray(manifest.captures)
    || manifest.captures.some((capture) => capture.status !== "pending")) {
  throw new Error("manifest is invalid or already contains admitted evidence");
}
if (manifest.candidate?.git_revision !== null
    || manifest.candidate?.package_sha256 !== null
    || manifest.candidate?.package_path !== null
    || (manifest.candidate?.tested_macos?.length ?? 0) !== 0) {
  throw new Error("manifest candidate is already initialized");
}

const { stdout: status } = await execFile("git", ["status", "--porcelain"], { cwd: repository });
if (status.trim()) throw new Error("candidate initialization requires a clean Git worktree");
const { stdout: revisionOutput } = await execFile("git", ["rev-parse", "HEAD"], { cwd: repository });
const gitRevision = revisionOutput.trim();
if (!/^[0-9a-f]{40}$/.test(gitRevision)) throw new Error("invalid Git revision");
const configuration = JSON.parse(await readFile(
  resolve(repository, "desktop/backend/tauri.conf.json"), "utf8"));
const packageSha256 = await sha256File(packagePath);
const packageRelativePath = relative(repository, packagePath).split(sep).join("/");
const candidate = {
  version: configuration.version,
  git_revision: gitRevision,
  package_path: packageRelativePath,
  package_sha256: packageSha256,
  tested_macos: [macosVersion],
};

if (dryRun) {
  console.log(JSON.stringify(candidate, null, 2));
  process.exit(0);
}

const updated = `${JSON.stringify({ ...manifest, candidate }, null, 2)}\n`;
const temporaryPath = `${manifestPath}.tmp-${process.pid}`;
let temporary;
try {
  temporary = await open(temporaryPath, "wx", manifestStat.mode & 0o777);
  await temporary.writeFile(updated, "utf8");
  await temporary.sync();
  await temporary.close();
  temporary = undefined;
  const currentBytes = await readFile(manifestPath);
  if (!currentBytes.equals(originalBytes)) throw new Error("manifest changed during initialization");
  await rename(temporaryPath, manifestPath);
} catch (error) {
  await temporary?.close();
  await rm(temporaryPath, { force: true });
  throw error;
}
console.log(`Initialized Desktop evidence for ${gitRevision} (${packageSha256})`);

async function sha256File(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}
