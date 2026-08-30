/** Typed Tauri updater composition with bounded state and reconciliation. */

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { check as checkForUpdate, type CheckOptions, type DownloadEvent } from "@tauri-apps/plugin-updater";

import {
  initialDesktopUpdateState, reduceDesktopUpdate, type DesktopUpdateEvent,
  type DesktopUpdateState,
} from "../state/desktop-update";

const MAX_PENDING_UPDATE_BYTES = 256;

export interface NativeDesktopUpdate {
  readonly currentVersion: string;
  readonly version: string;
  download(listener: (event: DownloadEvent) => void): Promise<void>;
  install(): Promise<void>;
  close(): Promise<void>;
}

export interface PendingDesktopUpdate {
  readonly schema_version: 1;
  readonly current_version: string;
  readonly target_version: string;
  readonly phase: "installing";
}

export interface UpdateBridge {
  currentVersion(): Promise<string>;
  check(options: CheckOptions): Promise<NativeDesktopUpdate | null>;
  readPending(): Promise<PendingDesktopUpdate | undefined>;
  writePending(value: PendingDesktopUpdate | undefined): Promise<void>;
  restart(): Promise<void>;
}

/** Owns one non-overlapping Desktop update effect and its secret-free UI state. */
export class DesktopUpdateClient {
  private state: DesktopUpdateState = { kind: "unavailable", currentVersion: "0.0.0" };
  private candidate?: NativeDesktopUpdate;
  private active?: Promise<void>;
  private readonly listeners = new Set<(state: DesktopUpdateState) => void>();

  public constructor(
    private readonly configured: boolean,
    private readonly bridge: UpdateBridge = TAURI_UPDATE_BRIDGE,
  ) {}

  public snapshot(): DesktopUpdateState { return this.state; }

  public subscribe(listener: (state: DesktopUpdateState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  public async initialize(): Promise<void> {
    const currentVersion = await this.bridge.currentVersion();
    this.state = initialDesktopUpdateState(this.configured, currentVersion);
    let pending: PendingDesktopUpdate | undefined;
    try { pending = await this.bridge.readPending(); }
    catch {
      if (this.configured) {
        this.state = { kind: "failed", currentVersion, reason: "update_outcome_unknown" };
      }
      this.emit();
      return;
    }
    if (pending && pending.target_version === currentVersion) {
      await this.bridge.writePending(undefined);
      this.state = this.configured
        ? { kind: "current", currentVersion }
        : { kind: "unavailable", currentVersion };
    } else if (pending && this.configured) {
      this.state = { kind: "failed", currentVersion, reason: "update_outcome_unknown" };
    }
    this.emit();
  }

  public check(): Promise<void> {
    return this.runExclusive(async () => {
      const checking = reduceDesktopUpdate(this.state, { type: "check" });
      if (checking === this.state) return;
      this.set(checking);
      try {
        const candidate = await this.bridge.check({ allowDowngrades: false, timeout: 30_000 });
        if (!candidate) {
          this.transition({ type: "check_result" });
          return;
        }
        if (candidate.currentVersion !== this.state.currentVersion) {
          await candidate.close();
          this.transition({ type: "check_failed" });
          return;
        }
        this.transition({ type: "check_result", version: candidate.version });
        if (this.state.kind === "available") this.candidate = candidate;
        else await candidate.close();
      } catch {
        this.transition({ type: "check_failed" });
      }
    });
  }

  public download(): Promise<void> {
    return this.runExclusive(async () => {
      if (!this.candidate) return;
      const downloading = reduceDesktopUpdate(this.state, { type: "download" });
      if (downloading === this.state) return;
      this.set(downloading);
      try {
        await this.candidate.download((event) => {
          if (event.event === "Started") {
            this.transition({ type: "download_metadata", totalBytes: event.data.contentLength });
          } else if (event.event === "Progress") {
            this.transition({ type: "download_progress", chunkBytes: event.data.chunkLength });
          }
        });
        this.transition({ type: "download_verified" });
        if (this.state.kind !== "ready_to_install") await this.releaseCandidate();
      } catch (error) {
        this.transition({ type: "download_failed", signatureInvalid: signatureFailure(error) });
        await this.releaseCandidate();
      }
    });
  }

  public install(): Promise<void> {
    return this.runExclusive(async () => {
      if (!this.candidate) return;
      const installing = reduceDesktopUpdate(this.state, { type: "install" });
      if (installing === this.state || installing.kind !== "installing") return;
      this.set(installing);
      const pending: PendingDesktopUpdate = {
        schema_version: 1, current_version: installing.currentVersion,
        target_version: installing.targetVersion, phase: "installing",
      };
      let installInvoked = false;
      try {
        await this.bridge.writePending(pending);
        installInvoked = true;
        await this.candidate.install();
        this.candidate = undefined;
        this.transition({ type: "install_committed" });
      } catch {
        if (!installInvoked) {
          try { await this.bridge.writePending(undefined); } catch { /* install was never invoked */ }
        }
        this.transition({ type: "install_failed", outcomeUnknown: installInvoked });
        await this.releaseCandidate();
      }
    });
  }

  public async restart(): Promise<void> {
    if (this.state.kind === "restart_required") await this.bridge.restart();
  }

  private runExclusive(effect: () => Promise<void>): Promise<void> {
    if (this.active) return this.active;
    const active = effect().finally(() => {
      if (this.active === active) this.active = undefined;
    });
    this.active = active;
    return active;
  }

  private transition(event: DesktopUpdateEvent): void {
    this.set(reduceDesktopUpdate(this.state, event));
  }

  private set(state: DesktopUpdateState): void {
    if (state === this.state) return;
    this.state = state;
    this.emit();
  }

  private emit(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private async releaseCandidate(): Promise<void> {
    const candidate = this.candidate;
    this.candidate = undefined;
    try { await candidate?.close(); } catch { /* resource cleanup cannot alter update truth */ }
  }
}

const TAURI_UPDATE_BRIDGE: UpdateBridge = {
  currentVersion: getVersion,
  check: checkForUpdate,
  async readPending() {
    return decodePending(await invoke<unknown>("read_pending_update", {}));
  },
  async writePending(value) {
    await invoke("write_pending_update", {
      value: value === undefined ? null : Array.from(encodePending(value)),
    });
  },
  async restart() { await invoke("restart_desktop", {}); },
};

function decodePending(raw: unknown): PendingDesktopUpdate | undefined {
  if (raw === null || raw === undefined) return undefined;
  if (!Array.isArray(raw) || raw.length === 0 || raw.length > MAX_PENDING_UPDATE_BYTES
    || raw.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)) {
    throw new Error("invalid_pending_update");
  }
  let parsed: unknown;
  try { parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(raw))); }
  catch { throw new Error("invalid_pending_update"); }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("invalid_pending_update");
  const value = parsed as Record<string, unknown>;
  if (Object.keys(value).sort().join(",") !== "current_version,phase,schema_version,target_version"
    || value.schema_version !== 1 || value.phase !== "installing"
    || typeof value.current_version !== "string" || typeof value.target_version !== "string") {
    throw new Error("invalid_pending_update");
  }
  return value as unknown as PendingDesktopUpdate;
}

function encodePending(value: PendingDesktopUpdate): Uint8Array {
  const encoded = new TextEncoder().encode(JSON.stringify(value));
  if (encoded.length === 0 || encoded.length > MAX_PENDING_UPDATE_BYTES) {
    throw new Error("invalid_pending_update");
  }
  return encoded;
}

function signatureFailure(error: unknown): boolean {
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return ["signature verification failed", "different key than the one provided",
    "Unexpected signature algorithm", "signature algorithm is not supported",
    "Invalid encoding in minisign data", "signature must be the contents of the `.sig` file"]
    .some((fragment) => message.includes(fragment));
}
