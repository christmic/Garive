// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";

const commands: string[] = [];
let storedPending: unknown = null;

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string, args: Record<string, unknown>) => {
  commands.push(command);
  switch (command) {
    case "get_desktop_capabilities": return { configured: true,
      agent_definition_id: "definition-main", multi_turn: true, durable_navigation: true,
      activity: true, setup: true, workspaces: true, artifacts: true };
    case "read_client_preferences": return null;
    case "read_pending_command": return storedPending;
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

describe("Desktop product experience", () => {
  beforeEach(() => {
    commands.length = 0; storedPending = null;
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
    const send = screen.getByRole<HTMLButtonElement>("button", { name: "Send work" });
    await waitFor(() => expect(send.disabled).toBe(false));
    fireEvent.click(send);

    await waitFor(() => expect(commands).toContain("start_product_turn"));
    expect(await screen.findByText("Durable product answer")).toBeTruthy();
    expect(commands).toContain("create_product_session");
    expect(commands).toContain("start_product_turn");
    expect(commands).toContain("get_session_events");
    expect(commands).not.toContain("run_agent_turn");
  });
});
