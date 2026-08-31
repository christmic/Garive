/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createTranslator } from "../i18n";
import type { WorkspaceEntry, WorkspaceGrant } from "../ipc/host";
import { WorkspacePicker } from "./WorkspacePicker";

afterEach(cleanup);

describe("localized Workspace picker", () => {
  it("keeps durable display names unchanged while localizing controls", async () => {
    const confirmed = vi.fn<(entries: readonly WorkspaceEntry[]) => void>();
    render(<WorkspacePicker preview grant={{
      schema_version: 1, workspace_id: "workspace-preview", display_name: "Launch 材料",
      grant_revision: 1, access: "enumerate", state: "active",
      expires_at: "2030-01-01T00:00:00Z",
    } satisfies WorkspaceGrant} onCancel={vi.fn()} onConfirm={confirmed}
      t={createTranslator("zh-Hans")} />);
    await screen.findByRole("heading", { name: "从 Launch 材料中选择文件" });
    expect(document.querySelector(".workspace-sheet > header > h2")).toBeTruthy();
    expect(document.querySelector(".workspace-heading")).toBeNull();
    expect(screen.getByText("Launch brief.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Research notes/ }));
    expect(await screen.findByTitle("Research notes")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "返回上级文件夹" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择文件作为上下文" }));
    fireEvent.click(screen.getByRole("button", { name: "添加 1 个文件" }));
    expect(confirmed).toHaveBeenCalledOnce();
  });
});
