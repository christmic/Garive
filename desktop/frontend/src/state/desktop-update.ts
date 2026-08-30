/** Closed, secret-free Desktop update lifecycle owned by the product UI. */

export type DesktopUpdateFailure =
  | "update_not_configured"
  | "update_invalid_version"
  | "update_not_newer"
  | "update_check_failed"
  | "update_download_failed"
  | "update_signature_invalid"
  | "update_install_failed"
  | "update_outcome_unknown"
  | "update_busy";

interface CurrentVersion {
  readonly currentVersion: string;
}

interface CandidateVersion extends CurrentVersion {
  readonly targetVersion: string;
}

export type DesktopUpdateState =
  | ({ readonly kind: "unavailable" | "idle" | "checking" | "current" } & CurrentVersion)
  | ({ readonly kind: "available" | "ready_to_install" | "installing" | "restart_required" } & CandidateVersion)
  | ({ readonly kind: "downloading"; readonly receivedBytes: number; readonly totalBytes?: number } & CandidateVersion)
  | ({ readonly kind: "refused" | "failed"; readonly reason: DesktopUpdateFailure } & CurrentVersion);

export type DesktopUpdateEvent =
  | { readonly type: "check" }
  | { readonly type: "check_result"; readonly version?: string }
  | { readonly type: "check_failed" }
  | { readonly type: "download" }
  | { readonly type: "download_metadata"; readonly totalBytes?: number }
  | { readonly type: "download_progress"; readonly chunkBytes: number }
  | { readonly type: "download_verified" }
  | { readonly type: "download_failed"; readonly signatureInvalid: boolean }
  | { readonly type: "install" }
  | { readonly type: "install_committed" }
  | { readonly type: "install_failed"; readonly outcomeUnknown: boolean };

/** Creates an unavailable, idle, or invalid-version initial update state. */
export function initialDesktopUpdateState(
  configured: boolean,
  currentVersion: string,
): DesktopUpdateState {
  if (!configured) return { kind: "unavailable", currentVersion };
  if (!stableVersion(currentVersion)) {
    return { kind: "refused", currentVersion, reason: "update_invalid_version" };
  }
  return { kind: "idle", currentVersion };
}

/** Applies one admitted update event; duplicate or out-of-phase events are inert. */
export function reduceDesktopUpdate(
  state: DesktopUpdateState,
  event: DesktopUpdateEvent,
): DesktopUpdateState {
  if (event.type === "check") {
    return ["idle", "current", "refused", "failed"].includes(state.kind)
      ? { kind: "checking", currentVersion: state.currentVersion }
      : state;
  }
  if (event.type === "check_result" && state.kind === "checking") {
    if (event.version === undefined) return { kind: "current", currentVersion: state.currentVersion };
    const candidate = semanticVersion(event.version);
    if (!candidate) return failed(state, "refused", "update_invalid_version");
    if (candidate.prerelease || compareVersions(event.version, state.currentVersion) <= 0) {
      return failed(state, "refused", "update_not_newer");
    }
    return { kind: "available", currentVersion: state.currentVersion, targetVersion: event.version };
  }
  if (event.type === "check_failed" && state.kind === "checking") {
    return failed(state, "failed", "update_check_failed");
  }
  if (event.type === "download" && state.kind === "available") {
    return { ...state, kind: "downloading", receivedBytes: 0 };
  }
  if (event.type === "download_metadata" && state.kind === "downloading") {
    if (event.totalBytes !== undefined && !positiveInteger(event.totalBytes)) {
      return failed(state, "failed", "update_download_failed");
    }
    return { ...state, totalBytes: event.totalBytes };
  }
  if (event.type === "download_progress" && state.kind === "downloading") {
    if (!nonNegativeInteger(event.chunkBytes)) return failed(state, "failed", "update_download_failed");
    const receivedBytes = state.receivedBytes + event.chunkBytes;
    if (!Number.isSafeInteger(receivedBytes)
      || (state.totalBytes !== undefined && receivedBytes > state.totalBytes)) {
      return failed(state, "failed", "update_download_failed");
    }
    return { ...state, receivedBytes };
  }
  if (event.type === "download_verified" && state.kind === "downloading") {
    if (state.totalBytes !== undefined && state.receivedBytes !== state.totalBytes) {
      return failed(state, "failed", "update_download_failed");
    }
    return candidateState(state, "ready_to_install");
  }
  if (event.type === "download_failed" && state.kind === "downloading") {
    return event.signatureInvalid
      ? failed(state, "refused", "update_signature_invalid")
      : failed(state, "failed", "update_download_failed");
  }
  if (event.type === "install" && state.kind === "ready_to_install") {
    return candidateState(state, "installing");
  }
  if (event.type === "install_committed" && state.kind === "installing") {
    return candidateState(state, "restart_required");
  }
  if (event.type === "install_failed" && state.kind === "installing") {
    return failed(state, "failed", event.outcomeUnknown
      ? "update_outcome_unknown" : "update_install_failed");
  }
  return state;
}

function candidateState(
  state: CandidateVersion,
  kind: "ready_to_install" | "installing" | "restart_required",
): DesktopUpdateState {
  return { kind, currentVersion: state.currentVersion, targetVersion: state.targetVersion };
}

function failed(
  state: CurrentVersion,
  kind: "refused" | "failed",
  reason: DesktopUpdateFailure,
): DesktopUpdateState {
  return { kind, currentVersion: state.currentVersion, reason };
}

function compareVersions(left: string, right: string): number {
  const leftVersion = semanticVersion(left);
  const rightVersion = semanticVersion(right);
  if (!leftVersion || !rightVersion) return 0;
  for (let index = 0; index < leftVersion.core.length; index += 1) {
    if (leftVersion.core[index]! > rightVersion.core[index]!) return 1;
    if (leftVersion.core[index]! < rightVersion.core[index]!) return -1;
  }
  return 0;
}

function stableVersion(value: string): boolean {
  const parsed = semanticVersion(value);
  return parsed !== undefined && !parsed.prerelease;
}

function semanticVersion(value: string): { readonly core: readonly bigint[]; readonly prerelease: boolean } | undefined {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.exec(value);
  if (!match) return undefined;
  return { core: [BigInt(match[1]!), BigInt(match[2]!), BigInt(match[3]!)], prerelease: match[4] !== undefined };
}

function positiveInteger(value: number): boolean {
  return Number.isSafeInteger(value) && value > 0;
}

function nonNegativeInteger(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0;
}
