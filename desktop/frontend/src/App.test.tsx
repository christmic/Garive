// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";

const commands: string[] = [];
let storedPending: unknown = null;
let configured = true;
let artifactItems: unknown[] = [];
let completedText = "Durable product answer";
let clipboardWrite = vi.fn(async (_text: string) => undefined);

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
      definition_id: "definition-main", definition_revision: "revision-1",
      capabilities: ["local-text"] }, { api_version: "v1", definition_id: "definition-workspace",
      definition_revision: "revision-2", capabilities: ["read-file", "write-file"] }] };
    case "get_product_sessions": return { api_version: "v1", sessions: [] };
    case "create_product_session": return { session_id: "session-1", agent_instance_id: "agent-1",
      committed_position: 1 };
    case "get_product_timeline": return { api_version: "v1", session_id: "session-1", items: [],
      scanned_through_position: 1, observed_max_position: 1, has_more: false };
    case "start_product_turn": return { session_id: "session-1", turn_id: "turn-1",
      execution_id: "execution-1", committed_position: 4 };
    case "get_session_events": return { events: [{ api_version: "v1", session_id: "session-1",
      position: 6, event: "turn.completed", turn_id: "turn-1", execution_id: "execution-1",
      text: completedText }], scanned_through_position: 6, observed_max_position: 6 };
    case "list_artifacts": return { api_version: "v1", session_id: "session-1", items: artifactItems,
      scanned_through_position: 6, observed_max_position: 6, has_more: false };
    case "get_artifact_preview": return { schema_version: 1, artifact_id: "artifact-1",
      revision: 1, kind: "text", content_utf8: "# Verified memo\n\nImmutable source.",
      truncated: false };
    case "get_session_workspaces": return [];
    default: throw new Error(`unexpected command: ${command}`);
  }
}) }));

import { App, CommittedActivity, TurnActivityDisclosure, TurnProgress, UserMessage,
  WorkspaceLoading } from "./App";
import { createTranslator } from "./i18n";
import type { WorkState } from "./state/workspace";

afterEach(cleanup);

describe("Desktop product experience", () => {
  beforeEach(() => {
    commands.length = 0; storedPending = null; configured = true; artifactItems = [];
    completedText = "Durable product answer";
    clipboardWrite = vi.fn(async (_text: string) => undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true,
      value: { writeText: clipboardWrite } });
    Object.defineProperty(globalThis, "crypto", { configurable: true, value: webcrypto });
    Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn(() => ({
      matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn(),
    })) });
  });

  it("keeps routine workspace recovery quiet and logo-free", () => {
    const view = render(<WorkspaceLoading title="Opening your workspace"
      body="Recovering the local Runtime…" />);
    const status = screen.getByRole("status");
    expect(status.textContent).toContain("Recovering the local Runtime…");
    expect(status.textContent).toContain("Opening your workspace");
    expect(view.container.querySelector("svg, .orb, .setup-logo")).toBeNull();
    expect(view.container.querySelector(".workspace-loading-dot")?.getAttribute("aria-hidden"))
      .toBe("true");
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
    expect(view.container.querySelector(".composer")?.getAttribute("data-layout"))
      .toBe("multiline");
    const completedMeta = view.container.querySelector(".result-meta[data-terminal='completed']");
    expect(completedMeta?.classList.contains("attention")).toBe(false);
    expect(completedMeta?.querySelector(".result-terminal")?.classList.contains("sr-only")).toBe(true);
    expect(screen.getByRole("button", { name: "Export conversation as Markdown" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Work actions" }));
    expect(screen.getByRole("menu", { name: "Work actions" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /New work/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: /Open Environment/ }));
    expect(screen.getByRole("button", { name: "Toggle inspector" }).getAttribute("aria-expanded")).toBe("true");
    const environment = screen.getByRole("complementary", { name: "Work inspector" });
    expect(within(environment).getByRole<HTMLButtonElement>("button", { name: "Add context" }).disabled).toBe(false);
    expect(within(environment).queryByRole("button", { name: "Close inspector" })).toBeNull();
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
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Updates" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Updates" }));
    expect(await screen.findByRole("heading", { name: "Updates" })).toBeTruthy();
    expect(await screen.findByText("0.1.0")).toBeTruthy();
    expect(screen.getByText("This build has no configured update channel.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  });

  it("keeps desktop identity in the rail and execution state out of window chrome", async () => {
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    const host = view.container.querySelector(".host-identity");
    expect(host?.textContent).toContain("Local");
    expect(host?.textContent).toContain("Runtime ready");
    expect(host?.textContent).not.toContain("LocalLocal");
    expect(view.container.querySelector(".topbar .local-badge")).toBeNull();
    expect(screen.queryByRole("button", { name: "Account and app menu" })).toBeNull();
    const runtime = screen.getByRole("button", { name: "Local Runtime · Runtime ready" });
    fireEvent.click(runtime);
    expect(screen.getByRole("heading", { name: "Local Runtime" })).toBeTruthy();
  });

  it("turns the product switcher into a truthful keyboard Runtime menu", async () => {
    render(<App />);
    const trigger = await screen.findByRole("button", { name: "Garive Runtime menu" });
    expect(trigger.hasAttribute("disabled")).toBe(false);
    fireEvent.click(trigger);
    const menu = screen.getByRole("menu", { name: "Garive Runtime menu" });
    expect(within(menu).getByRole("status").textContent).toContain("Runtime ready");
    const runtime = within(menu).getByRole("menuitem", { name: "Local Runtime" });
    await waitFor(() => expect(document.activeElement).toBe(runtime));
    fireEvent.keyDown(menu, { key: "End" });
    expect(document.activeElement).toBe(within(menu).getByRole("menuitem", { name: /Settings/ }));
    fireEvent.keyDown(menu, { key: "Home" });
    expect(document.activeElement).toBe(runtime);
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Garive Runtime menu" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("presents durable search as a compact desktop finder", async () => {
    const view = render(<App />);
    fireEvent.click(await screen.findByTitle("Search durable work (⌘F)"));
    expect(screen.getByRole("heading", { name: "Find your work" })).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "Search durable work" })).toBeTruthy();
    expect(screen.getByRole("group", { name: "Filter durable work" })).toBeTruthy();
    expect(view.container.querySelector(".search-toolbar")).not.toBeNull();
    expect(view.container.querySelector(".search-result-heading")?.textContent).toContain("Recents");
    expect(view.container.querySelector(".search-empty")).not.toBeNull();
    expect(view.container.querySelector(".search-results")?.classList.contains("card")).toBe(false);
  });

  it("projects the real installed Agent catalogue into a progressive desktop workbench", async () => {
    const view = render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Agents" }));
    expect(await screen.findByRole("heading", { name: "Your Agents" })).toBeTruthy();
    expect(screen.queryByText("Installed locally")).toBeNull();
    expect(await screen.findByRole("navigation", { name: "Installed Agents" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "definition-main" })).toBeTruthy();
    expect(screen.getByText("revision-1")).toBeTruthy();
    expect(screen.getByLabelText("Default for new work")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /definition-workspace/ }));
    expect(screen.getByRole("heading", { name: "definition-workspace" })).toBeTruthy();
    expect(screen.getByText("revision-2")).toBeTruthy();
    fireEvent.click(screen.getByText("Capabilities"));
    expect(screen.getByText("write-file")).toBeTruthy();
    expect(view.container.querySelectorAll(".agent-card")).toHaveLength(0);
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
    expect(screen.queryByRole("heading", { name: "Local Runtime" })).toBeNull();
    expect(screen.getByRole("button", { name: "Local Runtime" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Local Runtime" }));
    expect(await screen.findByRole("heading", { name: "Local Runtime" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Usage & capacity" })).toBeNull();
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
    const composer = screen.getByRole("textbox", { name: "Describe the outcome you want" });
    composer.focus();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const dialog = await screen.findByRole("dialog", { name: "Garive command center" });
    const commandSearch = screen.getByRole("textbox", { name: "Search commands and durable work" });
    expect(commandSearch).toBeTruthy();
    fireEvent.keyDown(commandSearch, { key: "ArrowDown" });
    const newWork = within(dialog).getByRole("button", { name: "New work" });
    const searchAll = within(dialog).getByRole("button", { name: "Open full work search" });
    expect(newWork).toBe(document.activeElement);
    fireEvent.keyDown(newWork, { key: "ArrowDown" });
    expect(searchAll).toBe(document.activeElement);
    fireEvent.keyDown(searchAll, { key: "ArrowUp" });
    expect(newWork).toBe(document.activeElement);
    fireEvent.keyDown(newWork, { key: "End" });
    expect(within(dialog).getAllByRole("button").at(-1)).toBe(document.activeElement);
    fireEvent.keyDown(document.activeElement!, { key: "Home" });
    expect(newWork).toBe(document.activeElement);
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(composer).toBe(document.activeElement));
    expect(screen.queryByRole("dialog", { name: "Garive command center" })).toBeNull();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const routedDialog = await screen.findByRole("dialog", { name: "Garive command center" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeTruthy();
    expect(screen.queryByText("Desktop", { exact: true })).toBeNull();
    expect(routedDialog.isConnected).toBe(false);

    fireEvent.keyDown(window, { key: "k", metaKey: true });
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
    expect(view.container.querySelectorAll(".suggestion-grid .suggestion-icon")).toHaveLength(3);
    expect(view.container.querySelector(".suggestion-grid .suggestion-copy")?.textContent)
      .toBe("Turn notes into a clear decision memo");
    expect(view.container.querySelector(".suggestion-grid button svg[aria-hidden='true']")).not.toBeNull();
    const composer = screen.getByRole("textbox", { name: "Describe the outcome you want" });
    expect(composer.getAttribute("rows")).toBe("1");
    expect(composer.closest(".composer")?.getAttribute("data-layout")).toBe("single-line");
    expect(composer.getAttribute("aria-describedby")).toBe("composer-commit-note");
    expect(screen.getByRole("button", { name: "Add context" }).textContent).toBe("");
    expect(view.container.querySelector(".access-pill-label")?.textContent).toBe("Local · text only");
    expect(document.querySelector("#composer-commit-note")?.classList.contains("sr-only")).toBe(true);
    expect(view.container.querySelectorAll(".nav-item.selected, .recent-item.selected")).toHaveLength(1);
    expect(view.container.querySelector(".nav-item.selected")?.textContent).toContain("Work");
    expect(view.container.querySelector(".nav-stack")?.textContent).toContain("Agents");
    expect(view.container.querySelector(".nav-stack")?.textContent).toContain("Memory");
    expect(view.container.querySelector(".sidebar-section.library")).toBeNull();
    expect(screen.queryByRole("button", { name: "Toggle inspector" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Synthesize/ }));
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid")).toBeNull());
    await waitFor(() => expect(composer).toBe(document.activeElement));
  });

  it("collapses and restores the native navigation without discarding work", async () => {
    const view = render(<App />);
    await screen.findByText("What should we accomplish?");
    const separator = screen.getByRole("separator", { name: "Resize navigation" });
    expect(separator.getAttribute("aria-valuenow")).toBe("275");
    fireEvent.keyDown(separator, { key: "End" });
    expect(separator.getAttribute("aria-valuenow")).toBe("520");
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
    const topFade = view.container.querySelector<HTMLElement>(".conversation-top-fade")!;
    expect(topFade.dataset.visible).toBe("false");
    Object.defineProperties(conversation, {
      scrollHeight: { configurable: true, value: 1_000 },
      clientHeight: { configurable: true, value: 400 },
      scrollTop: { configurable: true, writable: true, value: 120 },
    });
    fireEvent.scroll(conversation);
    expect(topFade.dataset.visible).toBe("true");
    expect(screen.queryByRole("button", { name: "Jump to latest" })).toBeNull();
    fireEvent.wheel(conversation, { deltaY: -120 });
    conversation.scrollTop = 100;
    fireEvent.scroll(conversation);
    const jump = await screen.findByRole("button", { name: "Jump to latest" });
    expect(jump.closest(".composer-wrap")).toBe(view.container.querySelector(".composer-wrap"));
    expect(conversation.scrollTop).toBe(100);
    conversation.scrollTop = 0;
    fireEvent.scroll(conversation);
    expect(topFade.dataset.visible).toBe("false");
    conversation.scrollTop = 120;
    fireEvent.scroll(conversation);
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
    const inspectorToggle = screen.getByRole("button", { name: "Toggle inspector" });
    expect(inspectorToggle.getAttribute("aria-expanded")).toBe("true");
    expect(inspectorToggle.getAttribute("aria-controls")).toBe("work-inspector");
    expect(inspectorToggle.classList.contains("active")).toBe(false);
    expect(view.container.querySelector("#work-inspector")).not.toBeNull();
    inspectorToggle.focus();
    fireEvent.pointerUp(inspectorToggle);
    expect(inspectorToggle).not.toBe(document.activeElement);
    expect(view.container.querySelectorAll(".nav-item.selected, .recent-item.selected")).toHaveLength(1);
    expect(view.container.querySelector(".recent-item.selected")).not.toBeNull();
    expect(await screen.findByRole("tab", { name: "memo.md" })).toBeTruthy();
    expect(commands).toContain("get_artifact_preview");
    expect(screen.queryByRole("heading", { name: "Deliverables" })).toBeNull();
    expect(await screen.findByRole("tab", { name: "memo.md" })).toBeTruthy();
    expect(view.container.querySelector(".artifact-workbench-actions")?.textContent).toContain("Export copy…");
    const separator = screen.getByRole("separator", { name: "Resize workbench" });
    expect(separator.getAttribute("aria-valuenow")).toBe("352");
    fireEvent.keyDown(separator, { key: "ArrowRight" });
    expect(separator.getAttribute("aria-valuenow")).toBe("368");
    expect(view.container.querySelector<HTMLElement>(".app-shell")?.style
      .getPropertyValue("--conversation-split")).toBe("368px");
    fireEvent.mouseDown(separator);
    expect(view.container.querySelector(".app-shell")?.classList.contains("panel-dragging")).toBe(true);
    fireEvent.mouseUp(window);
    expect(view.container.querySelector(".app-shell")?.classList.contains("panel-dragging")).toBe(false);
    fireEvent.keyDown(separator, { key: "Home" });
    expect(separator.getAttribute("aria-valuenow")).toBe("320");
    fireEvent.doubleClick(separator);
    expect(separator.getAttribute("aria-valuenow")).toBe("352");
    fireEvent.click(await screen.findByRole("button", { name: "View source" }));
    expect(screen.getByLabelText("Artifact source").textContent).toContain("Immutable source.");
    fireEvent.click(screen.getByRole("button", { name: "Rendered" }));
    expect(await screen.findByRole("heading", { name: "Verified memo" })).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Close Artifact preview" })).toHaveLength(1);
    expect(view.container.querySelector(".workspace-panel > header .icon-button")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close Artifact preview" }));
    expect(await screen.findByRole("heading", { name: "Deliverables" })).toBeTruthy();
    expect(view.container.querySelector(".artifact-row")).not.toBeNull();
    expect(view.container.querySelector(".artifact-card")).toBeNull();
    expect(screen.getByRole("button", { name: "Close inspector" })).toBeTruthy();
    expect(view.container.querySelector(".workspace-panel > header .icon-button")).not.toBeNull();
  });

  it("renders fenced output as a labeled copyable workbench block", async () => {
    completedText = "```rust\nfn main() { println!(\"verified\"); }\n```";
    const view = render(<App />);
    await waitFor(() => expect(view.container.querySelector(".suggestion-grid button")).not.toBeNull());
    fireEvent.click(view.container.querySelector<HTMLButtonElement>(".suggestion-grid button")!);
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send work" }).disabled).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "Send work" }));

    const block = await screen.findByRole("region", { name: "Code block" });
    expect(block.textContent).toContain("rust");
    expect(block.textContent).toContain("println!");
    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    await waitFor(() => expect(clipboardWrite).toHaveBeenCalledWith("fn main() { println!(\"verified\"); }"));
    expect(screen.getByRole("button", { name: "Code copied" })).toBeTruthy();
  });

  it("renders progressive work from admitted Activity instead of invented stages", () => {
    const open = vi.fn();
    const view = render(<TurnProgress t={createTranslator("en")} onOpen={open}
      goal="Prepare the launch decision memo" activities={[{
      api_version: "v1", activity_id: "read-1", kind: "tool",
      label_key: "agent.activity.read_file", state: "completed", source_position: 4,
      terminal: true,
    }, { api_version: "v1", activity_id: "write-1", kind: "tool",
      label_key: "agent.activity.write_file", state: "running", source_position: 7,
      terminal: false,
    }]} />);
    expect(screen.getByText("Pursuing goal")).toBeTruthy();
    expect(screen.getByText("Prepare the launch decision memo")).toBeTruthy();
    const rail = view.container.querySelector(".turn-progress");
    expect(rail?.getAttribute("data-composer-rail-item")).toBe("present");
    expect(rail?.getAttribute("data-composer-rail-placement")).toBe("above");
    expect(rail?.getAttribute("data-composer-rail-variant")).toBe("controls");
    expect(screen.getByText("Read scoped file")).toBeTruthy();
    expect(screen.getByText("Write scoped file")).toBeTruthy();
    expect(view.container.querySelector(".progress-state")).toBeNull();
    expect(view.container.querySelector(".sr-only")?.textContent).toContain("Running");
    fireEvent.click(screen.getByRole("button", { name: "Open activity" }));
    expect(open).toHaveBeenCalledOnce();
  });

  it("collapses completed per-Turn activity and discloses only admitted facts", () => {
    const view = render(<TurnActivityDisclosure t={createTranslator("en")} activities={[{
      api_version: "v1", activity_id: "read-1", kind: "tool",
      label_key: "agent.activity.read_file", state: "completed", source_position: 4,
      terminal: true,
    }, { api_version: "v1", activity_id: "write-1", kind: "tool",
      label_key: "agent.activity.write_file", state: "completed", source_position: 7,
      terminal: true,
    }]} />);
    const disclosure = view.container.querySelector<HTMLDetailsElement>(".turn-activity");
    expect(disclosure?.open).toBe(false);
    expect(disclosure?.getAttribute("data-activity-count")).toBe("2");
    expect(screen.getByText("Read scoped file · Write scoped file")).toBeTruthy();
    expect(view.container.querySelectorAll(".turn-activity-row")).toHaveLength(2);
    fireEvent.click(view.container.querySelector("summary")!);
    expect(disclosure?.open).toBe(true);
  });

  it("collapses a measured long request and keeps copy in the Turn action row", async () => {
    const descriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollHeight");
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true,
      get() { return this.classList.contains("user-message-content") ? 500 : 0; } });
    const onCopy = vi.fn(async () => undefined);
    try {
      const view = render(<UserMessage id="user-long" text={"A long request\n".repeat(24)}
        copied={false} onCopy={onCopy} t={createTranslator("en")} />);
      const content = view.container.querySelector(".user-message-content");
      expect(content?.getAttribute("data-collapsed-lines")).toBe("19");
      expect(screen.getByRole("button", { name: "Show more" }).getAttribute("aria-expanded")).toBe("false");
      fireEvent.click(screen.getByRole("button", { name: "Show more" }));
      expect(screen.getByRole("button", { name: "Show less" }).getAttribute("aria-expanded")).toBe("true");
      fireEvent.click(screen.getByRole("button", { name: "Copy request" }));
      await waitFor(() => expect(onCopy).toHaveBeenCalledWith("user-long", "A long request\n".repeat(24)));
    } finally {
      if (descriptor) Object.defineProperty(HTMLElement.prototype, "scrollHeight", descriptor);
      else Reflect.deleteProperty(HTMLElement.prototype, "scrollHeight");
    }
  });

  it("keeps the goal rail visible when durable work needs input", () => {
    const view = render(<TurnProgress t={createTranslator("en")} onOpen={() => undefined}
      goal="Prepare the launch decision memo" status="Needs input" activities={[]} />);
    expect(view.container.querySelector(".turn-progress.attention")).not.toBeNull();
    expect(view.container.querySelector(".progress-state")?.textContent).toBe("Needs input");
    expect(screen.getByText("Prepare the launch decision memo")).toBeTruthy();
  });

  it("groups admitted Runtime, Workspace and Activity facts in Environment", () => {
    const state: WorkState = { boot: "ready", phase: "idle", execution: "idle",
      capabilities: { configured: true, agent_definition_id: "definition-main", multi_turn: true,
        durable_navigation: true, activity: true, setup: false, workspaces: true,
        artifacts: true, updater: false }, sessionId: "session-1", messages: [], artifacts: [],
      activities: [{ api_version: "v1", activity_id: "write-1", kind: "tool",
        label_key: "agent.activity.write_file", state: "running", source_position: 7,
        terminal: false }], workspaces: [{ api_version: "v1", session_id: "session-1",
        workspace_id: "workspace-1", display_name: "Launch materials", grant_revision: 2,
        access: "read_write", attached_position: 4 }], draft: "", inspectorOpen: true,
      inspectorTab: "activity" };
    render(<CommittedActivity state={state} t={createTranslator("en")} />);
    expect(screen.getByRole("heading", { name: "Runtime" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Workspaces" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Activity" })).toBeTruthy();
    expect(screen.getByText("Runtime ready")).toBeTruthy();
    expect(screen.getByText("Launch materials")).toBeTruthy();
    expect(screen.getByText("Write scoped file")).toBeTruthy();
  });
});
