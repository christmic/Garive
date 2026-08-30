#!/usr/bin/env node
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const releaseDir = dirname(fileURLToPath(import.meta.url));
const repository = resolve(releaseDir, "../..");
const specPath = resolve(repository, "spec/design/desktop-visual-manual-evidence.md");
const manualPath = resolve(repository, "docs/manual/desktop-user-guide.md");
const manifestPath = resolve(repository,
  process.argv[2] ?? "docs/evidence/desktop-capture-manifest.json");
const assetRoot = resolve(repository, "docs/manual/assets/desktop");
const failures = [];

const spec = await readFile(specPath, "utf8");
const manual = await readFile(manualPath, "utf8");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const expectedIds = [...spec.matchAll(/\| `(M\d{2})` \|/g)].map((match) => match[1]);
const uniqueExpected = new Set(expectedIds);
if (expectedIds.length !== uniqueExpected.size) failures.push("Spec contains duplicate capture IDs");
if (manifest.schema_version !== 1) failures.push("manifest schema_version must be 1");
if (!Array.isArray(manifest.captures)) failures.push("captures must be an array");

const captures = Array.isArray(manifest.captures) ? manifest.captures : [];
const pendingManualIds = new Set(
  [...manual.matchAll(/SCREENSHOT (M\d{2}) PENDING/g)].map((match) => match[1]),
);
const manualImages = new Map(
  [...manual.matchAll(/!\[(M\d{2})[^\]]*\]\((assets\/desktop\/[^)]+\.png)\)/g)]
    .map((match) => [match[1], match[2]]),
);
const capturedIds = captures.map((capture) => capture.id);
if (capturedIds.length !== new Set(capturedIds).size) failures.push("manifest contains duplicate IDs");
for (const id of expectedIds) {
  if (!capturedIds.includes(id)) failures.push(`${id}: missing manifest row`);
}
for (const id of capturedIds) {
  if (!uniqueExpected.has(id)) failures.push(`${id}: not declared by the accepted Spec`);
}

const candidate = manifest.candidate ?? {};
const candidateReady = typeof candidate.version === "string"
  && /^[0-9a-f]{40}$/.test(candidate.git_revision ?? "")
  && typeof candidate.package_path === "string"
  && /^[0-9a-f]{64}$/.test(candidate.package_sha256 ?? "")
  && Array.isArray(candidate.tested_macos) && candidate.tested_macos.length > 0;
if (candidateReady) {
  try {
    const packagePath = resolve(repository, candidate.package_path);
    const packageDigest = await sha256File(packagePath);
    if (packageDigest !== candidate.package_sha256) failures.push("candidate package SHA-256 mismatch");
  } catch {
    failures.push("candidate package is missing or unreadable");
  }
}
const classes = new Set([
  "packaged-real", "packaged-recovery", "deterministic-visual", "system-surface",
]);
const locales = new Set(["en", "zh-Hans", "en-XA"]);

for (const capture of captures) {
  const id = capture.id ?? "unknown";
  if (capture.status === "pending") {
    if (!pendingManualIds.has(id)) failures.push(`${id}: pending manual marker is missing`);
    failures.push(`${id}: pending`);
    continue;
  }
  if (capture.status !== "passed") {
    failures.push(`${id}: status must be pending or passed`);
    continue;
  }
  if (!candidateReady) failures.push(`${id}: candidate metadata is incomplete`);
  if (!classes.has(capture.evidence_class)) failures.push(`${id}: invalid evidence_class`);
  if (!locales.has(capture.locale)) failures.push(`${id}: invalid resolved locale`);
  if (!Array.isArray(capture.assertions) || capture.assertions.length === 0
      || capture.assertions.some((assertion) => typeof assertion !== "string" || !assertion)) {
    failures.push(`${id}: assertions must be a non-empty string array`);
  }
  for (const field of ["environment", "appearance", "density", "window_size",
    "setup_recipe", "captured_at", "image_path", "image_sha256"]) {
    if (typeof capture[field] !== "string" || !capture[field]) failures.push(`${id}: missing ${field}`);
  }
  if (capture.git_revision !== candidate.git_revision
      || capture.package_sha256 !== candidate.package_sha256) {
    failures.push(`${id}: candidate identity mismatch`);
  }
  const manualImage = manualImages.get(id);
  const expectedManualImage = typeof capture.image_path === "string"
    ? capture.image_path.replace(/^docs\/manual\//, "") : undefined;
  if (!manualImage || manualImage !== expectedManualImage) {
    failures.push(`${id}: manual image reference is missing or mismatched`);
  }
  if (!Array.isArray(capture.redactions) || !Array.isArray(capture.edits)) {
    failures.push(`${id}: redactions and edits must be explicit arrays`);
  }
  if (typeof capture.image_path !== "string") continue;
  const imagePath = resolve(repository, capture.image_path);
  if (!imagePath.startsWith(`${assetRoot}${sep}`) || !imagePath.endsWith(".png")) {
    failures.push(`${id}: image must be a PNG under docs/manual/assets/desktop`);
    continue;
  }
  try {
    if ((await lstat(imagePath)).isSymbolicLink()) {
      failures.push(`${id}: image must not be a symbolic link`);
      continue;
    }
    const canonicalImage = await realpath(imagePath);
    if (!canonicalImage.startsWith(`${await realpath(assetRoot)}${sep}`)) {
      failures.push(`${id}: canonical image path escapes the Desktop asset root`);
      continue;
    }
    const imageBytes = await readFile(canonicalImage);
    if (imageBytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
      failures.push(`${id}: image does not have a PNG signature`);
      continue;
    }
    const digest = createHash("sha256").update(imageBytes).digest("hex");
    if (digest !== capture.image_sha256) failures.push(`${id}: image SHA-256 mismatch`);
  } catch {
    failures.push(`${id}: image is missing or unreadable`);
  }
}

if (captures.length > 0 && captures.every((capture) => capture.status === "passed")
    && (/PENDING|待录入|草案，不可发布/.test(manual))) {
  failures.push("manual still contains draft or pending markers");
}

if (failures.length > 0) {
  console.error(`Desktop evidence gate failed (${failures.length}):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`Desktop evidence gate passed: ${captures.length}/${expectedIds.length} captures`);

async function sha256File(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}
