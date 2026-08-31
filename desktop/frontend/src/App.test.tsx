// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";

const commands: string[] = [];
let storedPending: unknown = null;
let configured = true;
let artifactItems: unknown[] = [];

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
    case "list_artifacts": return { api_version: "v1", session_id: "session-1", items: artifactItems,
      scanned_through_position: 6, observed_max_position: 6, has_more: false };
    case "get_artifact_preview": return { schema_version: 1, artifact_id: "artifact-1",
      revision: 1, kind: "text", content_utf8: "# Verified memo\n\nImmutable source.",
      truncated: false };
    case "get_session_workspaces": return [];
    default: throw new Error(`unexpected command: ${command}`);
  }
}) }));

import { App, TurnProgress } from "./App";
import { createTranslator } from "./i18n";

afterEach(cleanup);

describe("Desktop product experience", () => {
  beforeEach(() => {
    commands.length = 0; storedPending = null; configured = true; artifactItems = [];
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

  it("opens one truthful usage view without changing durable task state", async () => {
    render(<App usageBudget={{ source: "included_plan", state: "critical",
      scopeLabel: "Personal plan", periodLabel: "5-hour window", remainingPercent: 8,
      resetsAtLabel: "Resets in 42m", attribution: "reported",
      modelPostureLabel: "Efficient", activeTurnMayFinish: true }} />);
    const trigger = await screen.findByRole("button", { name: "Capacity: 8% · 5-hour window" });
    fireEvent.click(trigger);
    expect(await screen.findByRole("heading", { name: "Usage & capacity" })).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "8% remaining" })).toBeTruthy();
    expect(screen.getByText("Current work may finish if included capacity reaches its limit.")).toBeTruthy();
    expect(screen.getByText("Local Runtime")).toBeTruthy();
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

  it("opens a keyboard-first command center and routes actions without losing work", async () => {
    render(<App />);
    await screen.findByText("What should we accomplish?");
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const dialog = await screen.findByRole("dialog", { name: "Garive command center" });
    const commandSearch = screen.getByRole("textbox", { name: "Search commands and durable work" });
    expect(commandSearch).toBeTruthy();
    fireEvent.keyDown(commandSearch, { key: "ArrowDown" });
    expect(screen.getByRole("button", { name: "New work" })).toBe(document.activeElement);
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeTruthy();
    expect(dialog.isConnected).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Quick switcher" }));
    expect(await screen.findByRole("dialog", { name: "Garive command center" })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Garive command center" })).toBeNull();
  });

  it("keeps navigation reachable as a dismissible small-window sheet", async () => {
    Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn((query: string) => ({
      matches: query === "(max-width: 480px)", addEventListener: vi.fn(), removeEventListener: vi.fn(),
    })) });
    render(<App />);
    await screen.findByText("What should we accomplish?");
    const trigger = screen.getByRole("button", { name: "Open navigation" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelector("#primary-navigation")?.hasAttribute("inert")).toBe(true);
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("button", { name: "Close navigation" })).toBeTruthy();
    expect(document.querySelector("#primary-navigation")?.hasAttribute("inert")).toBe(false);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("button", { name: "Close navigation" })).toBeNull();
  });

  it("keeps the new-task canvas quiet and free of decorative product marks", async () => {
    const view = render(<App />);
    await screen.findByText("What should we accomplish?");
    expect(view.container.querySelector(".new-work-surface")).not.toBeNull();
    expect(view.container.querySelector(".brand-mark, .hero-mark, .message-mark")).toBeNull();
    const composer = screen.getByRole("textbox", { name: "Describe the outcome you want" });
    expect(composer.getAttribute("rows")).toBe("1");
    expect(composer.getAttribute("aria-describedby")).toBe("composer-commit-note");
    expect(screen.getByRole("button", { name: "Add context" }).textContent).toBe("");
    expect(document.querySelector("#composer-commit-note")?.classList.contains("sr-only")).toBe(true);
  });

  it("collapses and restores the native navigation without discarding work", async () => {
    const view = render(<App />);
    await screen.findByText("What should we accomplish?");
    fireEvent.click(screen.getByRole("button", { name: "Hide navigation" }));
    expect(view.container.querySelector(".app-shell")?.classList.contains("navigation-collapsed")).toBe(true);
    expect(view.container.querySelector("#primary-navigation")?.getAttribute("aria-hidden")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Open navigation" }));
    expect(view.container.querySelector(".app-shell")?.classList.contains("navigation-collapsed")).toBe(false);
  });

  it("protects an older reading position and offers an explicit return to the tail", async () => {
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    fireEvent.click(view.container.querySelector<HTMLButtonElement>(".suggestion-grid button")!);
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send work" }).disabled).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "Send work" }));
    await screen.findByText("Durable product answer");

    const conversation = view.container.querySelector<HTMLElement>(".conversation")!;
    Object.defineProperties(conversation, {
      scrollHeight: { configurable: true, value: 1_000 },
      clientHeight: { configurable: true, value: 400 },
      scrollTop: { configurable: true, writable: true, value: 120 },
    });
    fireEvent.scroll(conversation);
    const jump = await screen.findByRole("button", { name: "Jump to latest" });
    expect(conversation.scrollTop).toBe(120);
    fireEvent.click(jump);
    expect(conversation.scrollTop).toBe(1_000);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull());
  });

  it("opens a Turn deliverable in one tabbed rendered/source workbench", async () => {
    artifactItems = [{ api_version: "v1", artifact_id: "artifact-1", revision: 1,
      session_id: "session-1", turn_id: "turn-1", display_name: "memo.md",
      kind: "document", mime_type: "text/markdown", byte_size: 42,
      content_digest: "7".repeat(64), committed_position: 6, verification: "not_run",
      preview: "text", revealable: true, exportable: true }];
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    fireEvent.click(view.container.querySelector<HTMLButtonElement>(".suggestion-grid button")!);
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send work" }).disabled).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "Send work" }));

    fireEvent.click(await screen.findByRole("button", { name: "Open deliverables" }));
    expect(await screen.findByRole("heading", { name: "Deliverables" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    expect(await screen.findByRole("tab", { name: "memo.md" })).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: "View source" }));
    expect(screen.getByLabelText("Artifact source").textContent).toContain("Immutable source.");
    fireEvent.click(screen.getByRole("button", { name: "Rendered" }));
    expect(await screen.findByRole("heading", { name: "Verified memo" })).toBeTruthy();
  });

  it("renders progressive work from admitted Activity instead of invented stages", () => {
    const open = vi.fn();
    render(<TurnProgress t={createTranslator("en")} onOpen={open} activities={[{
      api_version: "v1", activity_id: "read-1", kind: "tool",
      label_key: "agent.activity.read_file", state: "completed", source_position: 4,
      terminal: true,
    }, { api_version: "v1", activity_id: "write-1", kind: "tool",
      label_key: "agent.activity.write_file", state: "running", source_position: 7,
      terminal: false,
    }]} />);
    expect(screen.getByText("Read scoped file")).toBeTruthy();
    expect(screen.getByText("Write scoped file")).toBeTruthy();
    expect(screen.getByText("Running")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open activity" }));
    expect(open).toHaveBeenCalledOnce();
  });
});
