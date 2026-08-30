import { describe, expect, it } from "vitest";

import { initialDesktopUpdateState, reduceDesktopUpdate } from "./desktop-update";

describe("Desktop update lifecycle", () => {
  it("keeps an unconfigured build unavailable without an update action", () => {
    expect(initialDesktopUpdateState(false, "0.1.0")).toEqual({
      kind: "unavailable", currentVersion: "0.1.0",
    });
    expect(initialDesktopUpdateState(true, "not-semver")).toEqual({
      kind: "refused", currentVersion: "not-semver", reason: "update_invalid_version",
    });
  });

  it("admits one strictly newer stable candidate through verified install", () => {
    let state = initialDesktopUpdateState(true, "1.2.3+build.4");
    state = reduceDesktopUpdate(state, { type: "check" });
    expect(state.kind).toBe("checking");
    state = reduceDesktopUpdate(state, { type: "check_result", version: "1.3.0" });
    expect(state).toEqual({ kind: "available", currentVersion: "1.2.3+build.4", targetVersion: "1.3.0" });
    state = reduceDesktopUpdate(state, { type: "download" });
    state = reduceDesktopUpdate(state, { type: "download_metadata", totalBytes: 9 });
    state = reduceDesktopUpdate(state, { type: "download_progress", chunkBytes: 4 });
    state = reduceDesktopUpdate(state, { type: "download_progress", chunkBytes: 5 });
    expect(state).toMatchObject({ kind: "downloading", receivedBytes: 9, totalBytes: 9 });
    state = reduceDesktopUpdate(state, { type: "download_verified" });
    expect(state.kind).toBe("ready_to_install");
    state = reduceDesktopUpdate(state, { type: "install" });
    state = reduceDesktopUpdate(state, { type: "install_committed" });
    expect(state).toEqual({ kind: "restart_required", currentVersion: "1.2.3+build.4", targetVersion: "1.3.0" });
  });

  it("refuses equal, downgrade, prerelease, and malformed candidates", () => {
    for (const [version, reason] of [
      ["1.2.3", "update_not_newer"],
      ["1.2.2", "update_not_newer"],
      ["1.3.0-rc.1", "update_not_newer"],
      ["01.3.0", "update_invalid_version"],
    ] as const) {
      const checking = reduceDesktopUpdate(initialDesktopUpdateState(true, "1.2.3"), { type: "check" });
      expect(reduceDesktopUpdate(checking, { type: "check_result", version })).toEqual({
        kind: "refused", currentVersion: "1.2.3", reason,
      });
    }
  });

  it("fails closed on malformed progress and preserves an active effect", () => {
    const checking = reduceDesktopUpdate(initialDesktopUpdateState(true, "1.0.0"), { type: "check" });
    expect(reduceDesktopUpdate(checking, { type: "check" })).toBe(checking);
    const available = reduceDesktopUpdate(checking, { type: "check_result", version: "2.0.0" });
    let downloading = reduceDesktopUpdate(available, { type: "download" });
    downloading = reduceDesktopUpdate(downloading, { type: "download_metadata", totalBytes: 4 });
    expect(reduceDesktopUpdate(downloading, { type: "download_progress", chunkBytes: 5 })).toEqual({
      kind: "failed", currentVersion: "1.0.0", reason: "update_download_failed",
    });
  });

  it("classifies check, signature, install, and unknown outcomes without raw errors", () => {
    const idle = initialDesktopUpdateState(true, "1.0.0");
    const checking = reduceDesktopUpdate(idle, { type: "check" });
    expect(reduceDesktopUpdate(checking, { type: "check_failed" })).toMatchObject({ reason: "update_check_failed" });
    const available = reduceDesktopUpdate(checking, { type: "check_result", version: "2.0.0" });
    const downloading = reduceDesktopUpdate(available, { type: "download" });
    expect(reduceDesktopUpdate(downloading, { type: "download_failed", signatureInvalid: true }))
      .toMatchObject({ reason: "update_signature_invalid" });
    expect(reduceDesktopUpdate(downloading, { type: "download_failed", signatureInvalid: false }))
      .toMatchObject({ kind: "failed", reason: "update_download_failed" });
    const ready = reduceDesktopUpdate(downloading, { type: "download_verified" });
    const installing = reduceDesktopUpdate(ready, { type: "install" });
    expect(reduceDesktopUpdate(installing, { type: "install_failed", outcomeUnknown: false }))
      .toMatchObject({ reason: "update_install_failed" });
    expect(reduceDesktopUpdate(installing, { type: "install_failed", outcomeUnknown: true }))
      .toMatchObject({ reason: "update_outcome_unknown" });
    const unknown = reduceDesktopUpdate(installing, { type: "install_failed", outcomeUnknown: true });
    expect(reduceDesktopUpdate(unknown, { type: "check" })).toBe(unknown);
  });
});
