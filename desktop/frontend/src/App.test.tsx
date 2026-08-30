// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";

const commands: string[] = [];
let storedPending: unknown = null;
let configured = true;

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.1.0") }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string, args: Record<string, unknown>) => {
  commands.push(command);
  switch (command) {
    case "get_desktop_capabilities": return { configured,
      agent_definition_id: "definition-main", multi_turn: true, durable_navigation: true,
      activity: true, setup: !configured, workspaces: true, artifacts: true, updater: false };
    case "get_setup_catalogue": return { schema_version: 1, catalogue_revision: "catalogue-1",
      profiles: [], presets: [], limits: { max_profiles: 2, max_text_bytes: 256,
        max_endpoint_bytes: 2048, max_secret_bytes: 16384, max_plan_count: 16,
        plan_lifetime_seconds: 900 } };
    case "read_client_preferences": return null;
    case "read_pending_command": return storedPending;
    case "read_pending_update": return null;
    case "write_pending_command": storedPending = args.value; return undefined;
    case "write_client_preferences": return undefined;
    case "get_agent_definitions": return { api_version: "v1", definitions: [{ api_version: "v1",
      definition_id: "definition-main", definition_revision: "revision-1", capabilities: [] }] };
    case "get_product_sessions": return { api_version: "v1", sessions: [] };
    case "create_product_session": return { session_id: "session-1", agent_instance_id: "agent-1",
      committed_position: 1 };
    case "get_product_timeline": return { api_version: "v1", session_id: "session-1", items: [],
      scanned_through_position: 1, observed_max_position: 1, has_more: false };
    case "start_product_turn": return { session_id: "session-1", turn_id: "turn-1",
      execution_id: "execution-1", committed_position: 4 };
    case "get_session_events": return { events: [{ api_version: "v1", session_id: "session-1",
      position: 6, event: "turn.completed", turn_id: "turn-1", execution_id: "execution-1",
      text: "Durable product answer" }], scanned_through_position: 6, observed_max_position: 6 };
    case "list_artifacts": return { api_version: "v1", session_id: "session-1", items: [],
      scanned_through_position: 1, observed_max_position: 1, has_more: false };
    case "get_session_workspaces": return [];
    default: throw new Error(`unexpected command: ${command}`);
  }
}) }));

import { App } from "./App";

afterEach(cleanup);

describe("Desktop product experience", () => {
  beforeEach(() => {
    commands.length = 0; storedPending = null; configured = true;
    Object.defineProperty(globalThis, "crypto", { configurable: true, value: webcrypto });
    Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn(() => ({
      matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn(),
    })) });
  });

  it("creates, acknowledges, follows and completes a first durable Turn", async () => {
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    fireEvent.click(view.container.querySelector<HTMLButtonElement>(".suggestion-grid button")!);

    const composer = await screen.findByRole("textbox");
    await waitFor(() => expect((composer as HTMLTextAreaElement).value.length).toBeGreaterThan(0));
    await waitFor(() => expect(commands).toContain("get_product_timeline"));
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send work" }).disabled).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "Send work" }));

    await waitFor(() => expect(commands, JSON.stringify(commands)).toContain("start_product_turn"));
    expect(await screen.findByText("Durable product answer")).toBeTruthy();
    expect(commands).toContain("create_product_session");
    expect(commands).toContain("start_product_turn");
    expect(commands).toContain("get_session_events");
    expect(commands).not.toContain("run_agent_turn");
  });

  it("shows setup without issuing product reads when Runtime is not configured", async () => {
    configured = false;
    render(<App />);
    expect(await screen.findByText("Configure Garive")).toBeTruthy();
    expect(commands).not.toContain("get_agent_definitions");
    expect(commands).not.toContain("get_product_sessions");
  });

  it("shows exact version and an honest unavailable update channel", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Updates" })).toBeTruthy();
    expect(await screen.findByText("0.1.0")).toBeTruthy();
    expect(screen.getByText("This build has no configured update channel.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  });

  it("submits on Enter but not Shift+Enter or an active IME composition", async () => {
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    fireEvent.click(view.container.querySelector<HTMLButtonElement>(".suggestion-grid button")!);
    const composer = await screen.findByRole("textbox");
    await waitFor(() => expect(commands).toContain("get_product_timeline"));
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send work" }).disabled).toBe(false));
    fireEvent.keyDown(composer, { key: "Enter", shiftKey: true });
    fireEvent.keyDown(composer, { key: "Enter", isComposing: true });
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(commands).not.toContain("start_product_turn");
    fireEvent.keyDown(composer, { key: "Enter" });
    await waitFor(() => expect(commands).toContain("start_product_turn"));
  });
});
