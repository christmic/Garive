import { describe, expect, it, vi } from "vitest";

import { DesktopUpdateClient, type NativeDesktopUpdate, type UpdateBridge } from "./desktop-update";

function candidate(overrides: Partial<NativeDesktopUpdate> = {}): NativeDesktopUpdate {
  return {
    currentVersion: "1.0.0", version: "2.0.0",
    download: vi.fn(async (listener) => {
      listener({ event: "Started", data: { contentLength: 5 } });
      listener({ event: "Progress", data: { chunkLength: 5 } });
      listener({ event: "Finished" });
    }),
    install: vi.fn(async () => undefined), close: vi.fn(async () => undefined),
    ...overrides,
  };
}

function bridge(update: NativeDesktopUpdate | null = candidate()): UpdateBridge & {
  readonly check: ReturnType<typeof vi.fn>; readonly writePending: ReturnType<typeof vi.fn>;
  readonly restart: ReturnType<typeof vi.fn>;
} {
  return {
    currentVersion: vi.fn(async () => "1.0.0"),
    check: vi.fn(async () => update),
    readPending: vi.fn(async () => undefined),
    writePending: vi.fn(async () => undefined),
    restart: vi.fn(async () => undefined),
  };
}

describe("Tauri Desktop update client", () => {
  it("does no update request when the installed capability is unavailable", async () => {
    const port = bridge(); const client = new DesktopUpdateClient(false, port);
    await client.initialize(); await client.check();
    expect(client.snapshot()).toEqual({ kind: "unavailable", currentVersion: "1.0.0" });
    expect(port.check).not.toHaveBeenCalled();
  });

  it("checks without downgrade, verifies before install, persists, and restarts explicitly", async () => {
    const native = candidate(); const port = bridge(native);
    const client = new DesktopUpdateClient(true, port);
    const observed: string[] = []; client.subscribe((state) => observed.push(state.kind));
    await client.initialize(); await client.check(); await client.download(); await client.install();
    expect(port.check).toHaveBeenCalledWith({ allowDowngrades: false, timeout: 30_000 });
    expect(native.download).toHaveBeenCalledTimes(1);
    expect(native.install).toHaveBeenCalledTimes(1);
    expect(port.writePending).toHaveBeenCalledBefore(native.install as ReturnType<typeof vi.fn>);
    expect(client.snapshot().kind).toBe("restart_required");
    expect(observed).toEqual(["idle", "checking", "available", "downloading", "downloading",
      "downloading", "ready_to_install", "installing", "restart_required"]);
    expect(port.restart).not.toHaveBeenCalled();
    await client.restart(); expect(port.restart).toHaveBeenCalledOnce();
  });

  it("collapses signature failures and never retains the raw plugin error", async () => {
    const native = candidate({ download: vi.fn(async () => {
      throw new Error("The signature verification failed /Users/private/latest.tar.gz");
    }) });
    const client = new DesktopUpdateClient(true, bridge(native));
    await client.initialize(); await client.check(); await client.download();
    expect(client.snapshot()).toEqual({
      kind: "refused", currentVersion: "1.0.0", reason: "update_signature_invalid",
    });
    expect(JSON.stringify(client.snapshot())).not.toContain("/Users/private");
  });

  it("retains reconciliation when an invoked install has no proved outcome", async () => {
    const native = candidate({ install: vi.fn(async () => { throw new Error("private installer path"); }) });
    const port = bridge(native); const client = new DesktopUpdateClient(true, port);
    await client.initialize(); await client.check(); await client.download(); await client.install();
    expect(client.snapshot()).toMatchObject({ kind: "failed", reason: "update_outcome_unknown" });
    expect(port.writePending).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(client.snapshot())).not.toContain("private installer path");
  });

  it("reconciles a committed target and flags an unobserved old-version install", async () => {
    const installed = bridge(null);
    installed.currentVersion = vi.fn(async () => "2.0.0");
    installed.readPending = vi.fn(async () => ({ schema_version: 1 as const, current_version: "1.0.0",
      target_version: "2.0.0", phase: "installing" as const }));
    const installedClient = new DesktopUpdateClient(true, installed);
    await installedClient.initialize();
    expect(installedClient.snapshot().kind).toBe("current");
    expect(installed.writePending).toHaveBeenCalledWith(undefined);

    const unknown = bridge(null);
    unknown.readPending = installed.readPending;
    const unknownClient = new DesktopUpdateClient(true, unknown);
    await unknownClient.initialize();
    expect(unknownClient.snapshot()).toMatchObject({ kind: "failed", reason: "update_outcome_unknown" });
  });

  it("coalesces duplicate checks while one native request is active", async () => {
    let finish: ((value: NativeDesktopUpdate | null) => void) | undefined;
    const port = bridge(null);
    port.check.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    const client = new DesktopUpdateClient(true, port); await client.initialize();
    const first = client.check(); const second = client.check();
    expect(port.check).toHaveBeenCalledTimes(1);
    finish?.(null); await Promise.all([first, second]);
    expect(client.snapshot().kind).toBe("current");
  });
});
