import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  attachWorkspaceToSession, authorizeWorkspaceWrites, chooseWorkspace,
  detachWorkspaceFromSession,
  getArtifactPreview, getDesktopCapabilities, getSessionWorkspaces, getWorkspaceRecoveryStatus, listAllArtifacts,
  listWorkspaceAuthorizations, reauthorizeWorkspace,
  revokeWorkspace, setDesktopMenuLocale, commitArtifactExport,
  prepareArtifactExport, type ArtifactExportReceipt, type ArtifactPreview,
  type HostArtifact, type HostArtifactPage, type HostTimelinePage,
  type WorkspaceAuthorization,
  type WorkspaceAttachment, type WorkspaceEntry, type WorkspaceGrant, type WorkspaceRecoveryStatus,
} from "./ipc/host";
import { startProductTurnWithWorkspaceContext } from "./ipc/productHost";
import type { DesktopUpdateClient } from "./ipc/desktop-update";
import { canSubmit, initialWorkState, reduceWork, type WorkState } from "./state/workspace";
import type { DesktopUpdateState } from "./state/desktop-update";
import { Icon, type IconName } from "./ui/Icon";
import { UsageBudgetCard, UsageBudgetTrigger, type UsageBudgetSnapshot } from "./ui/UsageBudget";
import { SetupFlow } from "./features/setup/SetupFlow";
import { WorkspacePicker } from "./workspace/WorkspacePicker";
import { decodeDesktopMenuIntent, DESKTOP_MENU_EVENT } from "./desktopMenu";
import {
  readDesktopPreferences, writeDesktopPreferences, type DesktopDensity,
  type DesktopLocalePreference, type DesktopPreferences, type DesktopTheme,
} from "./preferences";
import { createTranslator, resolveDesktopLocale, type MessageKey } from "./i18n";
import { shouldSubmitComposer } from "./composer";
import { nextDesktopZoom } from "./zoom";
import { useDesktopProduct } from "./app/useDesktopProduct";
import type { ProductEffectPort } from "./app/ProductRuntime";
import type { AppIntent } from "./state/controller";
import {
  classifyTask, filterAndOrderTasks, summarizeTasks, type RecentTask, type TaskFilter,
} from "./taskPresentation";

type Screen = "work" | "search" | "agents" | "settings";
type WorkDispatch = React.Dispatch<Parameters<typeof reduceWork>[1]>;
interface SelectedContext {
  readonly grant: WorkspaceGrant;
  readonly entries: readonly WorkspaceEntry[];
}

const errorKeys: Record<string, MessageKey> = {
  not_configured: "error.notConfigured",
  invalid_configuration: "error.invalidConfiguration",
  host_failure: "error.hostFailure",
  execution_failure: "error.executionFailure",
  projection_failure: "error.projectionFailure",
  workspace_capability_invalid: "error.workspaceCapability",
  workspace_unavailable: "error.workspaceUnavailable",
  workspace_bound_exceeded: "error.workspaceBound",
  desktop_unavailable: "error.desktopUnavailable",
};

const visualTestMode = new URLSearchParams(window.location.search).get("visual-test");
const visualTest = import.meta.env.DEV && visualTestMode !== null;
const visualCapabilities = {
  configured: visualTestMode !== "setup",
  agent_definition_id: "garive-work",
  multi_turn: true,
  durable_navigation: visualTestMode !== "setup",
  activity: visualTestMode !== "setup",
  setup: visualTestMode === "setup",
  workspaces: visualTestMode !== "setup",
  artifacts: visualTestMode === "artifact",
  updater: false,
} as const;
const visualArtifactTimeline = {
  api_version: "v1", session_id: "visual-artifact-session", scanned_through_position: 24,
  observed_max_position: 24, has_more: false, items: [{ turn_id: "visual-artifact-turn",
    started_position: 3, latest_position: 24, state: "completed",
    user_text: "Create a concise launch decision memo in my Workspace",
    completion_text: "The launch decision memo was created in your authorized Workspace.",
    content_truncated: false, activities: [],
  }],
} satisfies HostTimelinePage;
const visualArtifactPage = {
  api_version: "v1", session_id: "visual-artifact-session", scanned_through_position: 23,
  observed_max_position: 24, has_more: false, items: [{ api_version: "v1",
    artifact_id: "artifact-launch-memo", revision: 1, session_id: "visual-artifact-session",
    turn_id: "visual-artifact-turn", display_name: "launch-decision.md", kind: "document",
    mime_type: "text/markdown", byte_size: 714, content_digest: "7".repeat(64),
    committed_position: 23, verification: "not_run", preview: "text",
    workspace_id: "workspace-preview", revealable: true, exportable: true,
  }],
} satisfies HostArtifactPage;
const visualArtifactPreview = {
  schema_version: 1, artifact_id: "artifact-launch-memo", revision: 1, kind: "text",
  content_utf8: "# Launch decision\n\nProceed with a reversible pilot for the design-partner cohort.\n\n## Decision\n\n- Owner: Product Operations\n- Review gate: 14 September\n- Rollback: pause new invitations while preserving collected feedback\n\n## Next step\n\nPublish the pilot brief and confirm the named launch owner.",
  truncated: false,
} satisfies ArtifactPreview;
const visualUsageBudget = {
  source: "included_plan", state: "watch", scopeLabel: "Personal plan",
  periodLabel: "5-hour window", remainingPercent: 28, resetsAtLabel: "Resets in 1h 40m",
  attribution: "reported", modelPostureLabel: "Balanced", activeTurnMayFinish: true,
} satisfies UsageBudgetSnapshot;

export interface AppProps {
  readonly client?: "desktop" | "web";
  readonly webCapabilities?: WorkState["capabilities"];
  readonly createProductPort?: () => ProductEffectPort;
  readonly usageBudget?: UsageBudgetSnapshot;
}

export function App({ client = "desktop", webCapabilities, createProductPort,
  usageBudget }: AppProps = {}) {
  const desktop = client === "desktop";
  const [state, dispatch] = useReducer(reduceWork, initialWorkState);
  const [screen, setScreen] = useState<Screen>("work");
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [recents, setRecents] = useState<readonly RecentTask[]>([]);
  const [recentTitles, setRecentTitles] = useState<Readonly<Record<string, string>>>({});
  const [commandOpen, setCommandOpen] = useState(false);
  const [selectedContext, setSelectedContext] = useState<SelectedContext>();
  const [pickerGrant, setPickerGrant] = useState<WorkspaceGrant>();
  const [detachingWorkspaceId, setDetachingWorkspaceId] = useState<string>();
  const [preferences, setPreferences] = useState(readDesktopPreferences);
  const [systemDark, setSystemDark] = useState(() =>
    window.matchMedia("(prefers-color-scheme: dark)").matches);
  const [smallWindow, setSmallWindow] = useState(() =>
    window.matchMedia("(max-width: 480px)").matches);
  const locale = resolveDesktopLocale(preferences.locale);
  const t = useMemo(() => createTranslator(locale), [locale]);
  const taskSummary = useMemo(() => summarizeTasks(recents), [recents]);
  const orderedRecents = useMemo(() => filterAndOrderTasks(recents, "all", "", recentTitles),
    [recentTitles, recents]);
  const visibleUsage = usageBudget ?? (visualTestMode === "usage" ? visualUsageBudget : undefined);
  const composer = useRef<HTMLTextAreaElement>(null);
  const approvalAction = useRef<HTMLButtonElement>(null);
  const desktopZoom = useRef(1);
  const pendingDraft = useRef("");
  const [queuedSubmission, setQueuedSubmission] = useState<string>();
  const [desktopUpdate, setDesktopUpdate] = useState<DesktopUpdateState>({
    kind: "unavailable", currentVersion: "—",
  });
  const desktopUpdateClient = useRef<DesktopUpdateClient | null>(null);
  const product = useDesktopProduct(state.capabilities
    ? state.capabilities.configured ? "configured" : "not_configured" : undefined, !visualTest,
    createProductPort);

  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const changed = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, []);
  useEffect(() => {
    const query = window.matchMedia("(max-width: 480px)");
    const changed = (event: MediaQueryListEvent) => {
      setSmallWindow(event.matches);
      if (!event.matches) setNavigationOpen(false);
    };
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, []);
  useEffect(() => { try { writeDesktopPreferences(preferences); } catch { /* optional */ } },
    [preferences]);
  useEffect(() => { document.documentElement.lang = locale === "en-XA" ? "en-XA" : locale; },
    [locale]);
  useEffect(() => {
    if (desktop && !visualTest) void setDesktopMenuLocale(locale).catch(() => undefined);
  }, [desktop, locale]);
  useEffect(() => {
    if (!desktop || !state.capabilities) return;
    if (visualTest) {
      setDesktopUpdate({ kind: "unavailable", currentVersion: "0.1.0" });
      return;
    }
    let cancelled = false;
    let unsubscribe: () => void = () => undefined;
    void import("./ipc/desktop-update").then(({ DesktopUpdateClient }) => {
      if (cancelled) return;
      const client = new DesktopUpdateClient(state.capabilities?.updater ?? false);
      desktopUpdateClient.current = client;
      unsubscribe = client.subscribe(setDesktopUpdate);
      void client.initialize().catch(() => setDesktopUpdate({
        kind: "failed", currentVersion: "—", reason: "update_outcome_unknown",
      }));
    });
    return () => { cancelled = true; unsubscribe(); };
  }, [desktop, state.capabilities?.updater]);

  const runUpdateAction = () => {
    const client = desktopUpdateClient.current;
    if (!client) return;
    if (["idle", "current", "refused", "failed"].includes(desktopUpdate.kind)) void client.check();
    else if (desktopUpdate.kind === "available") void client.download();
    else if (desktopUpdate.kind === "ready_to_install") void client.install();
    else if (desktopUpdate.kind === "restart_required") void client.restart();
  };

  const loadSessionExtras = useCallback(async (sessionId: string) => {
    const [artifacts, workspaces] = await Promise.all([
      listAllArtifacts(sessionId), getSessionWorkspaces(sessionId),
    ]);
    dispatch({ type: "artifacts_loaded", page: artifacts });
    dispatch({ type: "workspaces_loaded", sessionId, workspaces });
  }, []);

  useEffect(() => {
    if (!product.view) return;
    dispatch({ type: "product_projected", view: product.view });
    setRecents(product.view.sessions.map((session) => ({ session_id: session.sessionId,
      definition_id: session.definitionId, opened_at: session.openedAt,
      latest_turn_state: admittedTurnState(session.state), turn_count: session.turnCount })));
    const selected = product.view.selectedSessionId;
    if (selected) {
      const first = product.view.timeline[0]?.userText;
      if (first) setRecentTitles((current) => ({ ...current, [selected]: first }));
    }
  }, [product.view]);

  useEffect(() => {
    const sessionId = product.view?.selectedSessionId;
    if (!sessionId || product.view?.timelineSessionId !== sessionId) return;
    void loadSessionExtras(sessionId).catch(() => undefined);
  }, [loadSessionExtras, product.view?.selectedSessionId, product.view?.timelineSessionId]);

  useEffect(() => {
    const sessionId = product.current()?.selectedSessionId;
    if (!sessionId) return;
    if (pendingDraft.current) {
      const text = pendingDraft.current; pendingDraft.current = "";
      product.dispatch({ type: "edit_draft", sessionId, text });
    }
    if (queuedSubmission) {
      setQueuedSubmission(undefined);
      void issueStartTurn(product.dispatch, sessionId, queuedSubmission);
    }
  }, [product.current, product.dispatch, product.view, queuedSubmission]);

  const ensureProductSession = useCallback(async () => {
    const current = product.current();
    if (visualTest || current?.selectedSessionId || current?.pending.length) return;
    const definitionId = current?.definitions[0]?.definitionId
      ?? state.capabilities?.agent_definition_id;
    if (!definitionId) return;
    const commandId = commandIdentity("create");
    product.dispatch({ type: "create_session", definitionId, commandId,
      requestDigest: await semanticDigest({ kind: "create_session", definitionId }) });
  }, [product.current, product.dispatch, state.capabilities?.agent_definition_id]);

  const beginNewWork = useCallback(() => {
    dispatch({ type: "new_work" }); pendingDraft.current = ""; setQueuedSubmission(undefined);
    setSelectedContext(undefined); setScreen("work"); setNavigationOpen(false);
    void ensureProductSession();
    requestAnimationFrame(() => composer.current?.focus());
  }, [ensureProductSession]);

  const workDispatch = useCallback<WorkDispatch>((event) => {
    if (!visualTest && event.type === "draft_changed") {
      const sessionId = product.current()?.selectedSessionId;
      if (sessionId) product.dispatch({ type: "edit_draft", sessionId, text: event.value });
      else { pendingDraft.current = event.value; void ensureProductSession(); }
    } else if (!visualTest && event.type === "error_dismissed") {
      product.dispatch({ type: "dismiss_notice" });
    }
    dispatch(event);
  }, [ensureProductSession, product.current, product.dispatch]);

  useEffect(() => {
    if (visualTest) {
      dispatch({ type: "capabilities_loaded", capabilities: visualCapabilities });
      if (visualTestMode === "queue") {
        const previewRecents: readonly RecentTask[] = [{ session_id: "queue-approval",
          definition_id: "garive-work", opened_at: "2026-08-31T00:10:00Z",
          latest_turn_state: "suspended", turn_count: 3 }, { session_id: "queue-running",
          definition_id: "garive-work", opened_at: "2026-08-31T00:09:00Z",
          latest_turn_state: "running", turn_count: 2 }, { session_id: "queue-failed",
          definition_id: "garive-work", opened_at: "2026-08-31T00:08:00Z",
          latest_turn_state: "failed", turn_count: 1 }, { session_id: "queue-complete",
          definition_id: "garive-work", opened_at: "2026-08-31T00:07:00Z",
          latest_turn_state: "completed", turn_count: 4 }];
        setRecents(previewRecents); setRecentTitles({
          "queue-approval": "Approve the launch decision memo",
          "queue-running": "Synthesize customer research into themes",
          "queue-failed": "Prepare the quarterly operating review",
          "queue-complete": "Draft the partner onboarding brief",
        });
        setCommandOpen(true);
      }
      if (visualTestMode === "usage") setScreen("settings");
      if (visualTestMode === "approval") dispatch({ type: "session_loaded", timeline: {
        api_version: "v1", session_id: "visual-session", scanned_through_position: 12,
        observed_max_position: 12, has_more: false, items: [{
          turn_id: "visual-turn", started_position: 3, latest_position: 12, state: "suspended",
          user_text: "Create a decision memo in the selected Workspace", content_truncated: false,
          suspension: { suspension_id: "visual-approval", session_version: 5,
            kind: "approval_required" }, activities: [{ api_version: "v1",
              activity_id: "visual-effect", kind: "tool", label_key: "agent.activity.write_file",
              state: "prepared", source_position: 10, terminal: false }, { api_version: "v1",
              activity_id: "visual-approval", kind: "interaction", label_key: "agent.activity.approval",
              state: "waiting_for_input", source_position: 12, terminal: false }],
        }],
      } });
      if (visualTestMode === "approval") dispatch({ type: "workspaces_loaded",
        sessionId: "visual-session", workspaces: [{ api_version: "v1",
          session_id: "visual-session", workspace_id: "workspace-preview",
          display_name: "Launch materials", grant_revision: 2, access: "read_write",
          attached_position: 4 }] });
      if (visualTestMode === "artifact") {
        dispatch({ type: "session_loaded", timeline: visualArtifactTimeline });
        dispatch({ type: "artifacts_loaded", page: visualArtifactPage });
        dispatch({ type: "inspector_selected", tab: "artifacts" });
      }
      if (visualTestMode === "running") {
        dispatch({ type: "session_loaded", timeline: {
          api_version: "v1", session_id: "visual-running", scanned_through_position: 9,
          observed_max_position: 9, has_more: false, items: [{ turn_id: "running-turn",
            started_position: 3, latest_position: 9, state: "running",
            user_text: "Compare the launch research and prepare a decision memo",
            content_truncated: false, activities: [{ api_version: "v1",
              activity_id: "read-research", kind: "tool", label_key: "agent.activity.read_file",
              state: "completed", source_position: 6, terminal: true }, { api_version: "v1",
              activity_id: "draft-memo", kind: "tool", label_key: "agent.activity.write_file",
              state: "running", source_position: 9, terminal: false }] }],
        } });
        dispatch({ type: "submission_started" });
      }
      return;
    }
    if (!desktop) {
      dispatch({ type: "capabilities_loaded", capabilities: webCapabilities ?? {
        configured: false, multi_turn: false, durable_navigation: false, activity: false,
        setup: false, workspaces: false, artifacts: false, updater: false,
      } });
      return;
    }
    void getDesktopCapabilities()
      .then((capabilities) => {
        dispatch({ type: "capabilities_loaded", capabilities });
      })
      .catch(() => dispatch({ type: "capabilities_failed" }));
  }, [desktop, webCapabilities]);

  useEffect(() => {
    if (!desktop || visualTest) return;
    let active = true;
    let stop: (() => void) | undefined;
    void listen<unknown>(DESKTOP_MENU_EVENT, (event) => {
      const intent = decodeDesktopMenuIntent(event.payload);
      if (intent === "desktop.new-work") {
        beginNewWork();
      } else if (intent === "desktop.search") setScreen("search");
      else if (intent === "desktop.settings") setScreen("settings");
      else if (intent === "desktop.toggle-inspector") {
        dispatch({ type: "inspector_toggled" }); setScreen("work");
      } else if (intent === "desktop.zoom-in" || intent === "desktop.zoom-out"
        || intent === "desktop.actual-size") {
        const next = nextDesktopZoom(desktopZoom.current, intent);
        void getCurrentWebview().setZoom(next).then(() => {
          desktopZoom.current = next;
          document.documentElement.dataset.zoom = String(next);
        }).catch(() => undefined);
      }
    }).then((unlisten) => {
      if (active) stop = unlisten;
      else unlisten();
    }).catch(() => undefined);
    return () => { active = false; stop?.(); };
  }, [beginNewWork, desktop]);

  useEffect(() => {
    const shortcuts = (event: KeyboardEvent) => {
      if (event.key === "Escape" && navigationOpen) {
        event.preventDefault(); setNavigationOpen(false); return;
      }
      if (!event.metaKey) return;
      if (event.key.toLowerCase() === "n") {
        event.preventDefault(); beginNewWork();
      }
      if (event.key === ",") { event.preventDefault(); setScreen("settings"); }
      if (event.key.toLowerCase() === "k") { event.preventDefault(); setCommandOpen(true); }
      if (event.key.toLowerCase() === "f") { event.preventDefault(); setScreen("search"); }
      if (event.shiftKey && event.key.toLowerCase() === "a") {
        event.preventDefault(); dispatch({ type: "inspector_toggled" });
      }
    };
    window.addEventListener("keydown", shortcuts);
    return () => window.removeEventListener("keydown", shortcuts);
  }, [beginNewWork, navigationOpen]);

  const title = useMemo(() => {
    const first = state.messages.find((message) => message.role === "user")?.text;
    return first ? first.slice(0, 54) : t("work.new");
  }, [state.messages, t]);

  const submit = async () => {
    if (!canSubmit(state)) return;
    const input = state.draft.trim();
    const definition = state.capabilities?.agent_definition_id;
    if (!definition) { dispatch({ type: "submission_failed", code: "not_configured" }); return; }
    dispatch({ type: "submission_started" });
    if (visualTest) {
      dispatch({ type: "submission_succeeded", input, result: {
        session_id: state.sessionId ?? "visual-session",
        turn_id: `visual-turn-${state.messages.length}`,
        execution_id: "visual-execution",
        cursor: state.messages.length + 9,
        text: "## Decision brief\n\nThe outcome is ready to move forward with a clear owner and a reversible first step.\n\n| Priority | Next action |\n| --- | --- |\n| High | Confirm the launch owner |\n| Next | Share the review draft |\n\n- [x] Decisions separated from assumptions\n- [ ] Confirm the final date",
        terminal: "completed",
      } });
      setSelectedContext(undefined);
      return;
    }
    try {
      const current = product.current();
      const sessionId = current?.selectedSessionId;
      if (!sessionId) {
        pendingDraft.current = input; setQueuedSubmission(input);
        await ensureProductSession(); return;
      }
      const suspended = current.timeline.find((item) => item.state === "suspended")?.suspension;
      if (suspended) {
        const commandId = commandIdentity("continue");
        product.dispatch({ type: "continue_suspension", sessionId,
          turnId: current.timeline.find((item) => item.suspension === suspended)!.turnId,
          input, commandId, requestDigest: await semanticDigest({ kind: "continue_turn",
            sessionId, suspensionId: suspended.suspensionId, input }) });
      } else if (selectedContext) {
        await attachWorkspaceToSession(sessionId, selectedContext.grant.workspace_id);
        await startProductTurnWithWorkspaceContext(commandIdentity("context-turn"), sessionId, input,
          selectedContext.grant.workspace_id, selectedContext.entries.map((entry) => entry.entry_id));
        product.dispatch({ type: "select_session", sessionId });
      } else {
        await issueStartTurn(product.dispatch, sessionId, input);
      }
      setSelectedContext(undefined);
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "host_failure" });
    }
  };

  const cancelTurn = async () => {
    const current = product.current();
    const sessionId = current?.selectedSessionId;
    const turn = current?.timeline.find((item) => item.state === "running");
    if (!sessionId || !turn || current?.pending.length) return;
    const commandId = commandIdentity("cancel");
    product.dispatch({ type: "cancel_turn", sessionId, turnId: turn.turnId, commandId,
      requestDigest: await semanticDigest({ kind: "cancel_turn", sessionId, turnId: turn.turnId,
        throughPosition: String(current.cursor) }) });
  };

  const retryPending = () => product.dispatch({ type: "retry_pending",
    sessionId: product.view?.selectedSessionId });
  const reconnect = () => {
    const sessionId = product.current()?.selectedSessionId;
    if (sessionId) product.dispatch({ type: "reconnect", sessionId });
  };

  const openContext = async () => {
    try {
      const grant = visualTest ? {
        schema_version: 1, workspace_id: "workspace-preview", display_name: "Launch materials",
        access: "enumerate", grant_revision: 1, state: "active",
        expires_at: "2026-08-30T15:00:00Z",
      } satisfies WorkspaceGrant : await chooseWorkspace();
      if (grant) setPickerGrant(grant);
    } catch {
      dispatch({ type: "submission_failed", code: "workspace_unavailable" });
    }
  };

  const authorizeOutputs = async () => {
    if (!selectedContext || state.phase === "submitting") return;
    try {
      const grant = visualTest ? { ...selectedContext.grant, access: "read_write" as const,
        grant_revision: selectedContext.grant.grant_revision + 1 }
        : await authorizeWorkspaceWrites(selectedContext.grant.workspace_id);
      if (grant) setSelectedContext({ ...selectedContext, grant });
    } catch {
      dispatch({ type: "submission_failed", code: "workspace_unavailable" });
    }
  };

  const resolveApproval = async (approved: boolean) => {
    const message = [...state.messages].reverse().find((item) =>
      item.suspension?.kind === "approval_required");
    if (!message?.suspension || !state.sessionId || state.phase === "submitting") return;
    dispatch({ type: "submission_started" });
    try {
      if (visualTest) {
        dispatch({ type: "session_loaded", timeline: {
          api_version: "v1", session_id: state.sessionId, scanned_through_position: 18,
          observed_max_position: 18, has_more: false, items: [{ turn_id: message.id,
            started_position: 3, latest_position: 18, state: "completed",
            user_text: "Create a decision memo in the selected Workspace",
            completion_text: approved ? "The approved artifact was created." : "The write was declined.",
            content_truncated: false, activities: [],
          }],
        } });
        return;
      }
      const input = approved ? "true" : "false";
      const commandId = commandIdentity("continue");
      product.dispatch({ type: "continue_suspension", sessionId: state.sessionId,
        turnId: message.id, input, commandId,
        requestDigest: await semanticDigest({ kind: "continue_turn", sessionId: state.sessionId,
          turnId: message.id, suspensionId: message.suspension.suspension_id,
          sessionVersion: String(message.suspension.session_version), input }) });
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "host_failure" });
    }
  };

  const startSuggestion = (text: string) => {
    workDispatch({ type: "draft_changed", value: text });
    requestAnimationFrame(() => composer.current?.focus());
  };

  const openRecent = async (sessionId: string) => {
    setScreen("work");
    setSelectedContext(undefined);
    try {
      product.dispatch({ type: "select_session", sessionId });
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "projection_failure" });
    }
  };

  const detachWorkspace = async (attachment: WorkspaceAttachment) => {
    if (state.phase === "submitting" || detachingWorkspaceId) return;
    setDetachingWorkspaceId(attachment.workspace_id);
    try {
      if (visualTest) {
        dispatch({ type: "workspaces_loaded", sessionId: attachment.session_id,
          workspaces: state.workspaces.filter((item) =>
            item.workspace_id !== attachment.workspace_id) });
      } else {
        await detachWorkspaceFromSession(
          attachment.session_id, attachment.workspace_id, attachment.grant_revision,
        );
        product.dispatch({ type: "select_session", sessionId: attachment.session_id });
      }
    } catch (cause) {
      dispatch({ type: "submission_failed",
        code: typeof cause === "string" ? cause : "host_failure" });
    } finally {
      setDetachingWorkspaceId(undefined);
      requestAnimationFrame(() => {
        if (composer.current?.disabled) approvalAction.current?.focus();
        else composer.current?.focus();
      });
    }
  };

  const effectiveTheme = preferences.theme === "system"
    ? systemDark ? "dark" : "light" : preferences.theme;
  return <div className={`desktop-root theme-${effectiveTheme} density-${preferences.density}`}>
    <div className="app-shell" inert={Boolean(pickerGrant) || commandOpen}
      aria-hidden={Boolean(pickerGrant) || commandOpen}>
      <aside id="primary-navigation" className={navigationOpen ? "sidebar navigation-open" : "sidebar"}
        aria-label={t("shell.primaryNavigation")} inert={smallWindow && !navigationOpen}
        aria-hidden={smallWindow && !navigationOpen} onClickCapture={(event) => {
          if ((event.target as HTMLElement).closest("button")) setNavigationOpen(false);
        }}>
        <div className="titlebar-drag" data-tauri-drag-region />
        <button className="new-work" type="button" aria-label={t("nav.newWork")} onClick={beginNewWork}>
          <Icon name="plus" /><span>{t("nav.newWork")}</span><kbd>⌘N</kbd>
        </button>
        <nav className="nav-stack">
          <NavItem icon="work" label={t("nav.work")} selected={screen === "work"} onClick={() => setScreen("work")} />
          <NavItem icon="search" label={t("nav.search")} selected={screen === "search"}
            disabled={!state.capabilities?.durable_navigation} hint={t("shell.searchHint")}
            onClick={() => setScreen("search")} />
        </nav>
        <div className="sidebar-section">
          <div className="section-label"><span>{t("nav.recents")}</span>{taskSummary.attention > 0
            ? <span className="attention-count" aria-label={`${taskSummary.attention} ${t("tasks.attention")}`}>{taskSummary.attention}</span>
            : !state.capabilities?.durable_navigation && <span className="beta-tag">{t("shell.live")}</span>}</div>
          {orderedRecents.length > 0 ? orderedRecents.slice(0, 6).map((recent) => (
            <button className={recent.session_id === state.sessionId ? "recent-item selected" : "recent-item"}
              type="button" key={recent.session_id} onClick={() => void openRecent(recent.session_id)}>
              <span>{recent.session_id === state.sessionId && state.messages.length ? title : recentTitles[recent.session_id] || recentLabel(recent)}</span>
              <small><TaskStateDot task={recent} />{taskStateCopy(recent, t)}</small>
            </button>
          )) : state.messages.length > 0 ? (
            <button className="recent-item selected" type="button" onClick={() => setScreen("work")}>
              <span>{title}</span><small>{state.phase === "submitting" ? t("status.working")
                : terminalCopy(state.messages.at(-1)?.terminal, t)}</small>
            </button>
          ) : <p className="sidebar-empty">{t("shell.recentsEmpty")}</p>}
        </div>
        <div className="sidebar-section library">
          <div className="section-label">{t("nav.library")}</div>
          <NavItem icon="agent" label={t("nav.agents")} selected={screen === "agents"} onClick={() => setScreen("agents")} />
          <NavItem icon="memory" label={t("shell.memory")} disabled hint={t("shell.requiresMemory")} soon={t("shell.soon")} />
        </div>
        <div className="sidebar-footer">
          <NavItem icon="settings" label={t("nav.settings")} selected={screen === "settings"} onClick={() => setScreen("settings")} />
          <div className={`runtime-state ${state.capabilities?.configured ? "online" : "offline"}`}>
            <span className="status-dot" /><span>{state.capabilities?.configured ? t("shell.runtimeReady") : t("shell.setupRequired")}</span>
          </div>
        </div>
      </aside>
      {navigationOpen && <button className="navigation-backdrop" type="button"
        aria-label={t("shell.closeNavigation")} onClick={() => setNavigationOpen(false)} />}

      <main className="main-surface" inert={smallWindow && navigationOpen}
        aria-hidden={smallWindow && navigationOpen}>
        <header className="topbar" data-tauri-drag-region>
          <div className="topbar-title"><button className="navigation-trigger icon-button" type="button"
            aria-label={t("shell.openNavigation")} aria-expanded={navigationOpen}
            aria-controls="primary-navigation" onClick={() => setNavigationOpen((open) => !open)}><Icon name="panel" /></button>
            <span>{screen === "work" ? title : screen === "search" ? t("nav.search") : screen === "agents" ? t("nav.agents") : t("nav.settings")}</span>
            {screen === "work" && <span className={state.phase === "submitting" ? "local-badge working" : "local-badge"}>
              <span />{t(state.phase === "submitting" ? "status.working" : "shell.local")}</span>}
            {visualTest && <span className="local-badge qa-badge">{t("shell.qaPreview")}</span>}
          </div>
          <div className="topbar-actions">
            {visibleUsage && <UsageBudgetTrigger value={visibleUsage} label={t("usage.trigger")}
              onOpen={() => setScreen("settings")} />}
            <button className="command-trigger" type="button" onClick={() => setCommandOpen(true)}
              aria-label={t("command.open")}><Icon name="search" /><span>{t("command.open")}</span><kbd>⌘K</kbd></button>
            {screen === "work" && <button className={state.inspectorOpen ? "icon-button active" : "icon-button"}
              type="button" aria-label={t("shell.toggleInspector")} title={`${t("shell.toggleInspector")} (⌘⇧A)`}
              onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="panel" /></button>}
            <button className="avatar" type="button" aria-label={t("shell.accountMenu")}>G</button>
          </div>
        </header>

        {screen === "work" ? <WorkSurface state={state} composer={composer} submit={submit}
          startSuggestion={startSuggestion} dispatch={workDispatch} context={selectedContext}
          cancelTurn={cancelTurn} retryPending={retryPending} reconnect={reconnect}
          openContext={openContext} authorizeOutputs={authorizeOutputs}
          resolveApproval={resolveApproval} removeContext={() => setSelectedContext(undefined)}
          detachWorkspace={detachWorkspace} detachingWorkspaceId={detachingWorkspaceId}
          approvalAction={approvalAction} t={t} />
          : screen === "search" ? <SearchScreen recents={recents} titles={recentTitles} onOpen={openRecent} t={t} />
            : screen === "agents" ? <AgentsScreen definition={state.capabilities?.agent_definition_id} t={t} />
            : <SettingsScreen capabilities={state.capabilities} preferences={preferences}
              setPreferences={setPreferences} update={desktopUpdate} runUpdate={runUpdateAction}
              restartBlocked={state.phase === "submitting"} usage={visibleUsage} t={t} />}
      </main>
      {screen === "work" && state.inspectorOpen && <Inspector state={state} dispatch={workDispatch} t={t} />}
    </div>
    {pickerGrant && <WorkspacePicker grant={pickerGrant} preview={visualTest} t={t}
      onCancel={() => { setPickerGrant(undefined);
        requestAnimationFrame(() => composer.current?.focus()); }} onConfirm={(entries) => {
        setSelectedContext({ grant: pickerGrant, entries }); setPickerGrant(undefined);
        requestAnimationFrame(() => composer.current?.focus());
      }} />}
    {commandOpen && <CommandCenter recents={orderedRecents} titles={recentTitles}
      onClose={() => setCommandOpen(false)} onNewWork={() => { setCommandOpen(false); beginNewWork(); }}
      onSearch={() => { setCommandOpen(false); setScreen("search"); }}
      onSettings={() => { setCommandOpen(false); setScreen("settings"); }}
      onToggleInspector={() => { setCommandOpen(false); setScreen("work");
        dispatch({ type: "inspector_toggled" }); }}
      onOpen={(sessionId) => { setCommandOpen(false); void openRecent(sessionId); }} t={t} />}
  </div>;
}

function WorkSurface({ state, composer, submit, startSuggestion, dispatch, context, openContext,
  authorizeOutputs, resolveApproval, removeContext, detachWorkspace, detachingWorkspaceId,
  approvalAction, cancelTurn, retryPending, reconnect, t }: {
  state: WorkState;
  composer: React.RefObject<HTMLTextAreaElement | null>;
  submit: () => Promise<void>;
  cancelTurn: () => Promise<void>;
  retryPending: () => void;
  reconnect: () => void;
  startSuggestion: (text: string) => void;
  dispatch: WorkDispatch;
  context?: SelectedContext;
  openContext: () => Promise<void>;
  authorizeOutputs: () => Promise<void>;
  resolveApproval: (approved: boolean) => Promise<void>;
  removeContext: () => void;
  detachWorkspace: (attachment: WorkspaceAttachment) => Promise<void>;
  detachingWorkspaceId?: string;
  approvalAction: React.RefObject<HTMLButtonElement | null>;
  t: (key: MessageKey) => string;
}) {
  if (state.boot === "loading") return <div className="center-state"><span className="orb loading"><Icon name="sparkle" /></span><h1>{t("work.boot.title")}</h1><p>{t("work.boot.body")}</p></div>;
  if (state.boot === "unavailable") return <StatusCard icon="warning" title={t("work.unavailable.title")} body={t("error.desktopUnavailable")} />;
  if (!state.capabilities?.configured) {
    return state.capabilities?.setup ? <SetupFlow preview={visualTest} t={t} /> : <SetupRequired t={t} />;
  }
  const suspension = [...state.messages].reverse().find((message) => message.suspension)?.suspension;
  const needsInput = suspension?.kind === "partial_output" || suspension?.kind === "external_input_required";
  const blockedSuspension = Boolean(suspension && !needsInput);
  const needsApproval = suspension?.kind === "approval_required";
  const approvalEffect = [...state.activities].reverse().find((activity) =>
    activity.kind === "tool" && !activity.terminal);
  const approvalWorkspace = state.workspaces.find((workspace) => workspace.access === "read_write")
    ?? state.workspaces[0];

  const disconnected = state.execution === "disconnected";
  const reconnecting = state.execution === "reconnecting";
  return <section className={state.messages.length ? "work-surface" : "work-surface new-work-surface"}>
    <div className={state.messages.length ? "conversation" : "conversation empty-conversation"}>
      {state.messages.length === 0 ? <Welcome onSelect={startSuggestion} t={t} />
        : <Timeline state={state} dispatch={dispatch} t={t} />}
    </div>
    {(state.error || disconnected || reconnecting) && <div className={disconnected || reconnecting
      ? "error-banner connection-banner" : "error-banner"} role={state.error ? "alert" : "status"}>
      <Icon name={reconnecting ? "activity" : "warning"} /><span>{reconnecting ? t("connection.reconnecting")
        : disconnected ? t("connection.disconnected") : t(errorKeys[state.error!] ?? "error.default")}</span>
      {disconnected && <button className="error-action" type="button" onClick={reconnect}>{t("connection.reconnect")}</button>}
      {state.error === "mutation_outcome_unknown" && <button className="error-action" type="button" onClick={retryPending}>{t("workspace.retry")}</button>}
      {state.error && <button type="button" onClick={() => dispatch({ type: "error_dismissed" })}
        aria-label={t("error.dismiss")}><Icon name="close" /></button>}</div>}
    <div className="composer-wrap">
      <div className={state.phase === "submitting" ? "composer busy" : "composer"}>
        {state.phase === "submitting" && <TurnProgress activities={state.activities}
          onOpen={() => dispatch({ type: "inspector_selected", tab: "activity" })} t={t} />}
        {needsApproval && <div className="approval-card" role="alert" aria-live="assertive" aria-label={t("approval.aria")}>
          <span className="approval-icon"><Icon name="shield" /></span><div><strong>{approvalEffect
            ? `${activityLabel(approvalEffect.label_key, t)} · ` : `${t("approval.operationPrefix")} `}<bdi>{approvalWorkspace?.display_name ?? t("approval.attachedWorkspace")}</bdi>?</strong>
            <div className="approval-facts"><span><b>{t("approval.scope")}</b>{t(approvalWorkspace?.access === "read_write" ? "approval.createOne" : "approval.exactOperation")}</span>
              <span><b>{t("approval.duration")}</b>{t("approval.durationValue")}</span><span><b>{t("approval.overwrite")}</b>{t("approval.overwriteValue")}</span></div>
            <p>{t("approval.changed")}</p></div>
          <div className="approval-actions"><button ref={approvalAction} type="button" autoFocus disabled={state.phase === "submitting"}
            onClick={() => void resolveApproval(false)}>{t("approval.decline")}</button><button className="primary" type="button"
              disabled={state.phase === "submitting"} onClick={() => void resolveApproval(true)}>{t("approval.approveOnce")}</button></div>
        </div>}
        {state.workspaces.length > 0 && <div className="attached-workspaces"
          aria-label={t("context.attached")}>
          {state.workspaces.map((workspace) => <span className="context-chip workspace-chip"
            key={`${workspace.workspace_id}-${workspace.grant_revision}`}>
            <Icon name="work" /><span><strong dir="auto">{workspace.display_name}</strong>
              <small>{t(workspace.access === "read_write" ? "context.readOutput" : "context.readOnly")} · {t("context.attachedState")}</small></span>
            <button type="button" title={t("context.detach")}
              aria-label={t("context.detach")}
              disabled={state.phase === "submitting" || Boolean(detachingWorkspaceId)}
              onClick={() => void detachWorkspace(workspace)}>{detachingWorkspaceId === workspace.workspace_id
                ? <span className="spinner" /> : <Icon name="close" />}</button>
          </span>)}</div>}
        {context && <div className="context-chips" aria-label={t("context.nextTurn")}>
          {context.entries.map((entry) => <span className="context-chip" key={entry.entry_id}>
            <Icon name="file" /><span><strong dir="auto">{entry.display_name}</strong>
              <small>{state.phase === "submitting" ? t("context.committing") : context.grant.display_name}</small></span>
            <button type="button" disabled={state.phase === "submitting"} onClick={removeContext}
              aria-label={t("context.remove")}><Icon name="close" /></button>
          </span>)}</div>}
        <textarea ref={composer} value={state.draft} disabled={state.phase === "submitting" || blockedSuspension}
          aria-label={t(needsInput ? "work.composer.continue" : "work.composer.describe")}
          placeholder={t(blockedSuspension ? "work.composer.governed" : needsInput ? "work.composer.continuePlaceholder" : "work.composer.describePlaceholder")}
          onChange={(event) => dispatch({ type: "draft_changed", value: event.target.value })}
          onKeyDown={(event) => { if (shouldSubmitComposer({ key: event.key,
            shiftKey: event.shiftKey, isComposing: event.nativeEvent.isComposing })) {
            event.preventDefault(); void submit();
          } }} />
        <div className="composer-toolbar">
          <div className="composer-tools"><button type="button"
            disabled={!state.capabilities?.workspaces || state.phase === "submitting" || Boolean(suspension)}
            title={t(state.capabilities?.workspaces ? "work.composer.chooseFiles" : "work.composer.noWorkspaces")}
            onClick={() => void openContext()}><Icon name="paperclip" /><span>{t("work.composer.addContext")}</span></button>
            {context?.grant.access === "enumerate" && <button type="button" disabled={state.phase === "submitting"}
              onClick={() => void authorizeOutputs()}><Icon name="shield" /><span>{t("work.composer.allowOutputs")}</span></button>}
            <span className="access-pill"><Icon name="shield" />{needsInput ? t("work.composer.resume")
              : context?.grant.access === "read_write" ? t("work.composer.outputEnabled")
                : context ? `${context.entries.length} ${t(context.entries.length === 1 ? "workspace.file" : "workspace.filesPlural")}` : t("work.composer.localText")}</span></div>
          {state.phase === "submitting" && !reconnecting && <button className="secondary-button" type="button"
            onClick={() => void cancelTurn()}>{t("work.composer.requestStop")}</button>}
          <button className="send-button" type="button" disabled={!canSubmit(state)} aria-label={t("work.composer.send")} onClick={() => void submit()}>
            {state.phase === "submitting" ? <span className="spinner" /> : <Icon name="send" />}
          </button>
        </div>
      </div>
      <p className="composer-note">{t("work.composer.commitNote")}</p>
    </div>
  </section>;
}

function Welcome({ onSelect, t }: { onSelect: (text: string) => void; t: (key: MessageKey) => string }) {
  const suggestions = [[t("work.suggestion.synthesize"), t("work.suggestion.synthesizeBody")],
    [t("work.suggestion.analyze"), t("work.suggestion.analyzeBody")],
    [t("work.suggestion.create"), t("work.suggestion.createBody")]] as const;
  return <div className="welcome"><h1>{t("work.welcome.title")}</h1>
    <p className="welcome-copy">{t("work.welcome.description")}</p>
    <div className="suggestion-grid">{suggestions.map(([label, text]) => <button type="button" key={label} onClick={() => onSelect(text)}><span>{label}</span><p>{text}</p><Icon name="chevron" /></button>)}</div>
  </div>;
}

function Timeline({ state, dispatch, t }: { state: WorkState; dispatch: WorkDispatch;
  t: (key: MessageKey) => string }) {
  const [copiedId, setCopiedId] = useState<string>();
  const copyResult = async (id: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      window.setTimeout(() => setCopiedId(undefined), 1_500);
    } catch {
      setCopiedId(undefined);
    }
  };
  const latest = state.messages.at(-1);
  const announcement = state.phase === "submitting" ? t("timeline.workingAnnouncement")
    : latest?.role === "assistant" ? `${t("timeline.turn")} ${terminalCopy(latest.terminal, t)}。` : "";
  return <div className="timeline">{state.messages.map((message) => message.role === "user"
    ? <article className="message user-message" key={message.id}><div>{message.text}</div></article>
    : <article className="message assistant-message" key={message.id}><div><div className="result-markdown"><Markdown skipHtml remarkPlugins={[remarkGfm]}
      components={{ a: ({ children }) => <span className="safe-link">{children}</span> }}>{message.text || terminalCopy(message.terminal, t)}</Markdown></div>
      <div className="result-meta"><span><Icon name={message.terminal === "completed" ? "check" : "warning"} />{terminalCopy(message.terminal, t)}</span><div className="result-actions"><button type="button" disabled={!message.text} onClick={() => downloadMarkdown(message.id, message.text)}>{t("timeline.export")}</button><button type="button" onClick={() => void copyResult(message.id, message.text)}>{t(copiedId === message.id ? "timeline.copied" : "timeline.copy")}</button></div></div></div></article>)}
    {state.livePreview && <article className="message assistant-message live-answer" aria-label={t("timeline.liveAnswer")}>
      {state.livePreview.available && state.livePreview.text
        ? <div className="result-markdown"><Markdown skipHtml remarkPlugins={[remarkGfm]}>{state.livePreview.text}</Markdown></div>
        : <p><span className="live-pulse"><span /></span>{livePhaseCopy(state.livePreview.labelKey, t)}</p>}
    </article>}
    <p className="sr-only" aria-live="polite" aria-atomic="true">{announcement}</p>
  </div>;
}

function livePhaseCopy(key: string | undefined, t: (key: MessageKey) => string): string {
  const labels: Record<string, MessageKey> = { "agent.live.preparing": "timeline.livePreparing",
    "agent.live.generating": "timeline.liveGenerating", "agent.live.finalizing": "timeline.liveFinalizing" };
  return t(labels[key ?? ""] ?? "timeline.working");
}

export function TurnProgress({ activities, onOpen, t }: { activities: WorkState["activities"];
  onOpen: () => void; t: (key: MessageKey) => string }) {
  const recent = activities.slice(-3);
  return <article className="turn-progress" aria-label={t("timeline.progressTitle")}>
    <div className="turn-progress-head"><span className="live-pulse"><span /></span><div>
      <strong>{t("timeline.progressTitle")}</strong><p>{t("timeline.progressBody")}</p></div>
      <button type="button" onClick={onOpen}>{t("timeline.openActivity")}<Icon name="chevron" /></button></div>
    {recent.length > 0 && <div className="turn-progress-steps">{recent.map((activity) =>
      <div className={activity.terminal ? "complete" : "active"} key={`${activity.kind}-${activity.activity_id}`}>
        <span>{activity.terminal ? <Icon name="check" /> : <span className="spinner" />}</span>
        <strong>{activityLabel(activity.label_key, t)}</strong><small>{activityState(activity.state, t)}</small>
      </div>)}</div>}
  </article>;
}

function Inspector({ state, dispatch, t }: { state: WorkState; dispatch: WorkDispatch; t: (key: MessageKey) => string }) {
  const mode = state.inspectorTab === "activity" ? "environment-panel" : "workspace-panel";
  const [workspaceTitle, setWorkspaceTitle] = useState<string>();
  return <aside className={`inspector ${mode}`} aria-label={t("inspector.aria")}><header><div className="inspector-tabs" role="tablist" aria-label={t("inspector.views")}><button type="button" role="tab" aria-selected={state.inspectorTab === "activity"} className={state.inspectorTab === "activity" ? "active" : ""} onClick={() => { setWorkspaceTitle(undefined); dispatch({ type: "inspector_selected", tab: "activity" }); }}>{t("inspector.activity")}</button><button type="button" role="tab" aria-selected={state.inspectorTab === "artifacts"} className={state.inspectorTab === "artifacts" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "artifacts" })}>{workspaceTitle ?? t("inspector.artifacts")}</button></div>
    <button className="icon-button" type="button" aria-label={t("inspector.close")} onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="close" /></button></header>
    {state.inspectorTab === "activity" ? <div className="inspector-body" role="tabpanel"><CommittedActivity state={state} t={t} /></div>
      : <div className="inspector-body" role="tabpanel"><ResultDeliverables state={state} t={t} onPreviewTitle={setWorkspaceTitle} /></div>}
  </aside>;
}

function ResultDeliverables({ state, t, onPreviewTitle }: { state: WorkState; t: (key: MessageKey) => string;
  onPreviewTitle: (title?: string) => void }) {
  const [selected, setSelected] = useState<HostArtifact>();
  const [preview, setPreview] = useState<ArtifactPreview>();
  const [previewState, setPreviewState] = useState<"idle" | "loading" | "unavailable">("idle");
  const [exportStates, setExportStates] = useState<Readonly<Record<string,
    "exporting" | "exported" | "exists" | "unavailable">>>({});
  const [exportReceipts, setExportReceipts] = useState<Readonly<Record<string,
    ArtifactExportReceipt>>>({});
  const results = state.messages.filter((message) => message.role === "assistant" && message.text);
  const openPreview = async (artifact: HostArtifact) => {
    setSelected(artifact); onPreviewTitle(artifact.display_name); setPreview(undefined); setPreviewState("loading");
    try {
      const content = visualTest ? visualArtifactPreview
        : await getArtifactPreview(state.sessionId ?? "", artifact);
      setPreview(content); setPreviewState("idle");
    } catch { setPreviewState("unavailable"); }
  };
  const exportCopy = async (artifact: HostArtifact) => {
    const key = `${artifact.artifact_id}-${artifact.revision}`;
    setExportStates((current) => ({ ...current, [key]: "exporting" }));
    try {
      if (visualTest) {
        const receipt = { schema_version: 1, artifact_id: artifact.artifact_id,
          revision: artifact.revision, display_name: "launch-decision-copy.md",
          byte_size: artifact.byte_size, content_digest: artifact.content_digest,
          state: "exported" } satisfies ArtifactExportReceipt;
        setExportReceipts((current) => ({ ...current, [key]: receipt }));
        setExportStates((current) => ({ ...current, [key]: "exported" }));
        return;
      }
      const target = await prepareArtifactExport(state.sessionId ?? "", artifact);
      if (!target) {
        setExportStates((current) => {
          const next = { ...current }; delete next[key]; return next;
        });
        return;
      }
      const receipt = await commitArtifactExport(
        state.sessionId ?? "", artifact, target.export_target_id,
      );
      setExportReceipts((current) => ({ ...current, [key]: receipt }));
      setExportStates((current) => ({ ...current, [key]: "exported" }));
    } catch (cause) {
      const exists = String(cause).includes("artifact_overwrite_required");
      setExportStates((current) => ({ ...current, [key]: exists ? "exists" : "unavailable" }));
    }
  };
  if (!results.length && !state.artifacts.length) return <div className="inspector-empty"><Icon name="file" /><h2>{t("artifact.emptyTitle")}</h2><p>{t("artifact.emptyBody")}</p></div>;
  return <div className="deliverable-list"><div className="activity-intro"><h2>{t("artifact.title")}</h2><p>{t("artifact.description")}</p></div>
    {state.artifacts.map((artifact) => { const key = `${artifact.artifact_id}-${artifact.revision}`;
      const exportState = exportStates[key]; const receipt = exportReceipts[key];
      return <article className="artifact-card" key={key}>
      <span className="deliverable-icon"><Icon name="file" /></span><div className="artifact-card-body">
        <div className="artifact-title"><strong dir="auto">{artifact.display_name}</strong><span>v{artifact.revision}</span></div>
        <p>{formatBytes(artifact.byte_size)} · {artifact.mime_type} · {t("artifact.committed")}</p>
        <div className="artifact-actions"><div><button type="button" disabled={artifact.preview !== "text"}
          onClick={() => void openPreview(artifact)}>{t("artifact.preview")}</button><button type="button"
            disabled={!artifact.exportable || exportState === "exporting"}
            onClick={() => void exportCopy(artifact)}>{t(exportState === "exporting" ? "artifact.choosing" : "artifact.exportCopy")}</button></div>
          {artifact.workspace_id && <span><Icon name="shield" />{t("artifact.authorizedWorkspace")}</span>}</div>
        {exportState === "exported" && receipt && <p className="artifact-export-state success" role="status"><Icon name="check" />{t("artifact.exportedAs")} <bdi>{receipt.display_name}</bdi></p>}
        {exportState === "exists" && <p className="artifact-export-state error" role="alert"><Icon name="warning" />{t("artifact.overwriteError")}</p>}
        {exportState === "unavailable" && <p className="artifact-export-state error" role="alert"><Icon name="warning" />{t("artifact.exportError")}</p>}
      </div>
    </article>; })}
    {selected && <section className="artifact-preview" aria-label={t("artifact.previewAria")}><header><div><span>{t("artifact.previewVerified")}</span><strong dir="auto">{selected.display_name}</strong></div><button type="button" aria-label={t("artifact.closePreview")}
      onClick={() => { setSelected(undefined); onPreviewTitle(undefined); setPreview(undefined); setPreviewState("idle"); }}><Icon name="close" /></button></header>
      {previewState === "loading" ? <div className="preview-state" role="status"><span className="spinner" />{t("artifact.verifying")}</div>
        : previewState === "unavailable" ? <div className="preview-state error" role="alert"><Icon name="warning" />{t("artifact.changed")}</div>
          : preview && <div className="artifact-preview-content"><Markdown skipHtml remarkPlugins={[remarkGfm]}
            components={{ a: ({ children }) => <span className="safe-link">{children}</span> }}>{preview.content_utf8}</Markdown></div>}
      <footer><Icon name="shield" />{t("artifact.digestPrefix")} {selected.revision}</footer>
    </section>}
    {results.length > 0 && <div className="deliverable-section-label">{t("artifact.snapshots")}</div>}
    {results.map((result, index) => <article className="deliverable-card" key={result.id}><span className="deliverable-icon"><Icon name="file" /></span><div><strong>{t("artifact.result")} {index + 1}.md</strong><p>{result.text.replace(/[#|*`>\[\]]/g, " ").trim().slice(0, 92)}</p><button type="button" onClick={() => downloadMarkdown(result.id, result.text)}>{t("artifact.exportMarkdown")}</button></div></article>)}
    {!state.capabilities?.artifacts && <p className="activity-gate"><Icon name="shield" />{t("artifact.gated")}</p>}
  </div>;
}

function formatBytes(bytes: number) {
  return bytes < 1_024 ? `${bytes} B` : `${(bytes / 1_024).toFixed(bytes < 10_240 ? 1 : 0)} KB`;
}

function CommittedActivity({ state, t }: { state: WorkState; t: (key: MessageKey) => string }) {
  if (state.capabilities?.activity && state.activities.length) {
    const activities = [...state.activities].sort((left, right) =>
      Number(right.state === "attention_required") - Number(left.state === "attention_required")
        || left.source_position - right.source_position);
    return <div className="activity-list"><div className="activity-intro"><h2>{t("activity.committedTitle")}</h2><p>{t("activity.committedBody")}</p></div>
      {activities.map((activity) => <div className="activity-row" key={`${activity.kind}-${activity.activity_id}`}>
        <span className={`activity-status ${activity.state}`}><Icon name={activityIcon(activity.state)} /></span>
        <div><strong>{activityLabel(activity.label_key, t)}</strong><small>{activityState(activity.state, t)}</small></div>
      </div>)}
    </div>;
  }
  const turns = state.messages.filter((message) => message.role === "assistant");
  if (!turns.length && state.phase !== "submitting") return <div className="inspector-empty"><Icon name="activity" /><h2>{t("activity.emptyTitle")}</h2><p>{t("activity.emptyBody")}</p></div>;
  return <div className="activity-list"><div className="activity-intro"><h2>{t("activity.turnTitle")}</h2><p>{t("activity.turnBody")}</p></div>
    {turns.map((turn, index) => <div className="activity-row" key={turn.id}><span className={`activity-status ${turn.terminal ?? "running"}`}><Icon name={turn.terminal === "completed" ? "check" : "warning"} /></span>
      <div><strong>{t("timeline.turn")} {index + 1}</strong><small>{terminalCopy(turn.terminal, t)}</small></div></div>)}
    {state.phase === "submitting" && <div className="activity-row"><span className="activity-status running"><span className="spinner" /></span><div><strong>{t("activity.currentTurn")}</strong><small>{t("status.working")}</small></div></div>}
    {!state.capabilities?.activity && <p className="activity-gate"><Icon name="shield" />{t("activity.gated")}</p>}
  </div>;
}

function activityLabel(key: string, t: (key: MessageKey) => string) {
  const labels: Record<string, MessageKey> = {
    "agent.activity.read_file": "activity.readFile",
    "agent.activity.write_file": "activity.writeFile",
    "agent.activity.approval": "activity.approval",
    "agent.activity.external_input": "activity.externalInput",
    "agent.activity.tool_rejected": "activity.rejected",
  };
  return t(labels[key] ?? "activity.generic");
}
function activityState(state: string, t: (key: MessageKey) => string) {
  const labels: Record<string, MessageKey> = {
    prepared: "activity.state.prepared", waiting_for_input: "activity.state.waiting",
    input_received: "activity.state.received", authorized: "activity.state.authorized",
    running: "activity.state.running", completed: "activity.state.completed",
    denied: "activity.state.denied", failed: "activity.state.failed",
    cancelled: "activity.state.cancelled", attention_required: "activity.state.attention",
  };
  return t(labels[state] ?? "activity.state.updated");
}
function activityIcon(state: string): IconName {
  return state === "completed" || state === "input_received" ? "check"
    : state === "running" || state === "authorized" || state === "prepared" ? "activity"
      : "warning";
}

function SearchScreen({ recents, titles, onOpen, t }: {
  recents: readonly RecentTask[];
  titles: Readonly<Record<string, string>>;
  onOpen: (sessionId: string) => Promise<void>;
  t: (key: MessageKey) => string;
}) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<TaskFilter>("all");
  const results = filterAndOrderTasks(recents, filter, query, titles);
  return <section className="search-page"><div className="search-heading"><p className="eyebrow">{t("search.eyebrow")}</p><h1>{t("search.title")}</h1><p>{t("search.description")}</p></div>
    <div className="search-box"><Icon name="search" /><input autoFocus aria-label={t("search.label")}
      value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("search.placeholder")} /><kbd>⌘F</kbd></div>
    <div className="task-filters" role="group" aria-label={t("tasks.filterAria")}>
      {(["all", "attention", "active", "completed"] as const).map((value) => <button type="button"
        className={filter === value ? "selected" : ""} aria-pressed={filter === value}
        onClick={() => setFilter(value)} key={value}>{t(`tasks.${value}`)}</button>)}
    </div>
    <div className="search-results" aria-live="polite">{results.length ? results.map((recent) => <button type="button" key={recent.session_id} onClick={() => void onOpen(recent.session_id)}>
      <span className="search-result-icon"><Icon name="work" /></span><span><strong>{titles[recent.session_id] || recentLabel(recent)}</strong><small>{recent.turn_count} {t(recent.turn_count === 1 ? "search.turn" : "search.turns")} · {terminalCopy(recent.latest_turn_state, t)}</small></span><Icon name="chevron" /></button>)
      : <div className="search-empty"><Icon name="search" /><h2>{t(query ? "search.noMatch" : "search.noWork")}</h2><p>{t(query ? "search.tryDifferent" : "search.completedHint")}</p></div>}</div>
  </section>;
}

function CommandCenter({ recents, titles, onClose, onNewWork, onSearch, onSettings,
  onToggleInspector, onOpen, t }: {
  recents: readonly RecentTask[]; titles: Readonly<Record<string, string>>;
  onClose: () => void; onNewWork: () => void; onSearch: () => void; onSettings: () => void;
  onToggleInspector: () => void; onOpen: (sessionId: string) => void;
  t: (key: MessageKey) => string;
}) {
  const [query, setQuery] = useState("");
  const dialog = useRef<HTMLElement>(null);
  const matches = filterAndOrderTasks(recents, "all", query, titles).slice(0, 8);
  const actions = query ? [] : [
    { icon: "plus" as IconName, label: t("nav.newWork"), hint: "⌘N", run: onNewWork },
    { icon: "search" as IconName, label: t("command.searchAll"), hint: "⌘F", run: onSearch },
    { icon: "panel" as IconName, label: t("shell.toggleInspector"), hint: "⌘⇧A", run: onToggleInspector },
    { icon: "settings" as IconName, label: t("nav.settings"), hint: "⌘,", run: onSettings },
  ];
  const handleKeys = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") { event.preventDefault(); onClose(); return; }
    const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>("input, button") ?? [])]
      .filter((element) => !element.hasAttribute("disabled"));
    if (event.key === "ArrowDown" && event.target instanceof HTMLInputElement && focusable[1]) {
      event.preventDefault(); focusable[1].focus(); return;
    }
    if (event.key !== "Tab" || !focusable.length) return;
    const first = focusable[0]!; const last = focusable.at(-1)!;
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  };
  return <div className="command-backdrop" role="presentation" onMouseDown={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}><section ref={dialog} className="command-center" role="dialog" aria-modal="true"
      aria-label={t("command.title")} onKeyDown={handleKeys}>
      <div className="command-search"><Icon name="search" /><input autoFocus value={query}
        aria-label={t("command.searchLabel")} placeholder={t("command.placeholder")}
        onChange={(event) => setQuery(event.target.value)} /><kbd>esc</kbd></div>
      <div className="command-scroll">
        {actions.length > 0 && <div className="command-group"><p>{t("command.actions")}</p>{actions.map((action) =>
          <button type="button" aria-label={action.label} onClick={action.run} key={action.label}><span className="command-icon"><Icon name={action.icon} /></span>
            <strong>{action.label}</strong><kbd>{action.hint}</kbd></button>)}</div>}
        <div className="command-group"><p>{query ? t("command.matches") : t("command.work")}</p>
          {matches.map((task) => <button type="button" onClick={() => onOpen(task.session_id)} key={task.session_id}>
            <span className={`command-icon task-${classifyTask(task)}`}><TaskStateDot task={task} /></span>
            <span><strong>{titles[task.session_id] || recentLabel(task)}</strong><small>{taskStateCopy(task, t)}</small></span>
            <Icon name="chevron" /></button>)}
          {!matches.length && <div className="command-empty">{t("command.empty")}</div>}
        </div>
      </div>
      <footer><span>{t("command.keyboardHint")}</span><span>{recents.length} {t("command.durable")}</span></footer>
    </section></div>;
}

function SetupRequired({ t }: { t: (key: MessageKey) => string }) { return <StatusCard icon="shield" title={t("shell.setupRequired")} body={t("setup.unavailable")} />; }
function AgentsScreen({ definition, t }: { definition?: string; t: (key: MessageKey) => string }) { return <section className="content-page"><p className="eyebrow">{t("agents.eyebrow")}</p><h1>{t("agents.title")}</h1><p>{t("agents.description")}</p><div className="agent-card"><span className="agent-avatar"><Icon name="agent" /></span><div><h2>{definition ?? t("agents.none")}</h2><p>{t(definition ? "agents.readyBody" : "agents.configureBody")}</p></div><span className={definition ? "state-chip ready" : "state-chip"}>{t(definition ? "common.ready" : "common.unavailable")}</span></div></section>; }
function SettingsScreen({ capabilities, preferences, setPreferences, update, runUpdate,
  restartBlocked, usage, t }: {
  capabilities?: WorkState["capabilities"];
  preferences: DesktopPreferences;
  setPreferences: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
  update: DesktopUpdateState;
  runUpdate: () => void;
  restartBlocked: boolean;
  usage?: UsageBudgetSnapshot;
  t: (key: MessageKey) => string;
}) {
  const [workspaceRecovery, setWorkspaceRecovery] = useState<WorkspaceRecoveryStatus>();
  const [authorizations, setAuthorizations] = useState<readonly WorkspaceAuthorization[]>([]);
  const [restoring, setRestoring] = useState<string>();
  const [confirmingRevocation, setConfirmingRevocation] = useState<string>();
  const [revoking, setRevoking] = useState<string>();
  const [recoveryError, setRecoveryError] = useState<MessageKey>();
  const [recoveryNotice, setRecoveryNotice] = useState<MessageKey>();
  const workspaceHeading = useRef<HTMLHeadingElement>(null);
  const loadWorkspaceHealth = useCallback(async () => {
    if (!capabilities?.workspaces) return;
    if (visualTest) {
      const needsAccess = visualTestMode === "workspace-recovery";
      setWorkspaceRecovery({ schema_version: 1,
        state: needsAccess ? "attention_required" : "ready", restored_count: needsAccess ? 0 : 1,
        needs_reauthorization_count: needsAccess ? 1 : 0 });
      setAuthorizations([{ schema_version: 1, workspace_id: "workspace-preview",
        display_name: "Launch materials", grant_revision: 1,
        state: needsAccess ? "needs_reauthorization" : "active" }]);
      return;
    }
    const [status, items] = await Promise.all([
      getWorkspaceRecoveryStatus(), listWorkspaceAuthorizations(),
    ]);
    setWorkspaceRecovery(status); setAuthorizations(items); setRecoveryError(undefined);
  }, [capabilities?.workspaces]);
  useEffect(() => {
    void loadWorkspaceHealth().catch(() => { setWorkspaceRecovery({
      schema_version: 1, state: "index_unavailable", restored_count: 0,
      needs_reauthorization_count: 0,
    }); setRecoveryError("settings.workspace.statusUnavailable"); });
  }, [loadWorkspaceHealth]);
  const restoreAccess = async (workspace: WorkspaceAuthorization) => {
    setRestoring(workspace.workspace_id); setRecoveryError(undefined); setRecoveryNotice(undefined);
    try {
      if (visualTest) {
        setAuthorizations([{ ...workspace, grant_revision: workspace.grant_revision + 1,
          state: "active" }]);
        setWorkspaceRecovery({ schema_version: 1, state: "ready", restored_count: 1,
          needs_reauthorization_count: 0 });
      } else {
        const renewed = await reauthorizeWorkspace(workspace.workspace_id);
        if (renewed) await loadWorkspaceHealth();
      }
    } catch (error) {
      setRecoveryError(error instanceof Error && error.message === "workspace_capability_invalid"
        ? "settings.workspace.wrongFolder" : "settings.workspace.restoreError");
    } finally { setRestoring(undefined); }
  };
  const removeAccess = async (workspace: WorkspaceAuthorization) => {
    if (confirmingRevocation !== workspace.workspace_id) {
      setConfirmingRevocation(workspace.workspace_id);
      setRecoveryNotice("settings.workspace.confirmNotice");
      return;
    }
    setRevoking(workspace.workspace_id); setRecoveryError(undefined); setRecoveryNotice(undefined);
    try {
      const receipt = visualTest ? { cleanup_pending: false }
        : await revokeWorkspace(workspace.workspace_id, workspace.grant_revision);
      if (visualTest) {
        setAuthorizations((items) => items.filter((item) =>
          item.workspace_id !== workspace.workspace_id));
        setWorkspaceRecovery({ schema_version: 1, state: "ready", restored_count: 0,
          needs_reauthorization_count: 0 });
      } else await loadWorkspaceHealth();
      setRecoveryNotice(receipt.cleanup_pending
        ? "settings.workspace.cleanupPending" : "settings.workspace.removed");
      requestAnimationFrame(() => workspaceHeading.current?.focus());
    } catch {
      setRecoveryError("settings.workspace.revokeError");
    } finally { setRevoking(undefined); setConfirmingRevocation(undefined); }
  };
  const rows: readonly (readonly [MessageKey, boolean | undefined])[] = [
    ["settings.runtime.multiTurn", capabilities?.multi_turn],
    ["settings.runtime.recents", capabilities?.durable_navigation],
    ["settings.runtime.activity", capabilities?.activity],
    ["settings.runtime.setup", capabilities?.setup],
    ["settings.runtime.workspaces", capabilities?.workspaces],
    ["settings.runtime.artifacts", capabilities?.artifacts],
  ];
  const recoveryReady = workspaceRecovery?.state === "ready";
  return <section className="content-page settings-page"><p className="eyebrow">{t("settings.eyebrow")}</p><h1>{t("settings.title")}</h1>
    {usage && <UsageBudgetCard value={usage} copy={{ title: t("usage.title"),
      description: t("usage.description"), remaining: t("usage.remaining"),
      reported: t("usage.reported"), estimated: t("usage.estimated"), reset: t("usage.reset"),
      modelPosture: t("usage.modelPosture"), activeMayFinish: t("usage.activeMayFinish"),
      activeMayStop: t("usage.activeMayStop") }} />}
    <div className="settings-card"><h2>{t("settings.appearance.title")}</h2><p>{t("settings.appearance.description")}</p>
      <div className="setting-row"><span>{t("settings.theme")}</span><ThemeOptions value={preferences.theme} onChange={(theme) => setPreferences((current) => ({ ...current, theme }))} t={t} /></div>
      <div className="setting-row"><span>{t("settings.density")}</span><DensityOptions value={preferences.density} onChange={(density) => setPreferences((current) => ({ ...current, density }))} t={t} /></div>
    </div>
    <div className="settings-card"><h2>{t("settings.language.title")}</h2><p>{t("settings.language.description")}</p>
      <div className="setting-row"><span>{t("settings.language.label")}</span><LocaleOptions value={preferences.locale} onChange={(locale) => setPreferences((current) => ({ ...current, locale }))} t={t} /></div>
    </div>
    <UpdateSettings state={update} run={runUpdate} restartBlocked={restartBlocked} t={t} />
    <div className="settings-card"><h2>{t("settings.runtime.title")}</h2><p>{t("settings.runtime.description")}</p>{rows.map(([label, available]) => <div className="setting-row" key={label}><span>{t(label)}</span><span className={available ? "state-chip ready" : "state-chip"}>{t(available ? "settings.runtime.available" : "settings.runtime.notInstalled")}</span></div>)}</div>
    {capabilities?.workspaces && <div className="settings-card"><h2 ref={workspaceHeading} tabIndex={-1}>{t("settings.workspace.title")}</h2><p>{t("settings.workspace.description")}</p><div className="setting-row"><span>{t("settings.workspace.recovery")}</span><span className={recoveryReady ? "state-chip ready" : "state-chip attention"}>{workspaceRecovery ? recoveryReady ? `${workspaceRecovery.restored_count} ${t("settings.workspace.restored")}` : workspaceRecovery.state === "attention_required" ? `${workspaceRecovery.needs_reauthorization_count} ${t("settings.workspace.needsAccess")}` : t("settings.workspace.indexUnavailable") : t("settings.workspace.checking")}</span></div>
      {authorizations.map((workspace) => <div className="workspace-auth-row" key={workspace.workspace_id}><span className="workspace-auth-icon"><Icon name="work" /></span><span><strong dir="auto">{workspace.display_name}</strong><small>{workspace.state === "active" ? `${t("settings.workspace.readOnly")} ${workspace.grant_revision}` : t("settings.workspace.expired")}</small></span><span className="workspace-auth-actions">{workspace.state === "active" ? <span className="state-chip ready">{t("settings.workspace.active")}</span> : <button className="secondary-button" type="button" disabled={restoring === workspace.workspace_id || Boolean(revoking)} onClick={() => void restoreAccess(workspace)}>{restoring === workspace.workspace_id ? <><span className="spinner" />{t("settings.workspace.opening")}</> : t("settings.workspace.restore")}</button>}<button className={confirmingRevocation === workspace.workspace_id ? "danger-button confirming" : "danger-button"} type="button" aria-label={t("settings.workspace.removeAria")} disabled={Boolean(restoring) || Boolean(revoking)} onClick={() => void removeAccess(workspace)}>{revoking === workspace.workspace_id ? <><span className="spinner" />{t("settings.workspace.removing")}</> : t(confirmingRevocation === workspace.workspace_id ? "settings.workspace.confirmRemove" : "settings.workspace.remove")}</button></span></div>)}
      {recoveryNotice && <div className="workspace-recovery-notice" role="status"><Icon name="shield" /><span>{t(recoveryNotice)}</span></div>}
      {recoveryError && <div className="workspace-recovery-error" role="alert"><Icon name="warning" /><span>{t(recoveryError)}</span></div>}
    </div>}
    <div className="settings-card"><h2>{t("settings.privacy.title")}</h2><p>{t("settings.privacy.description")}</p></div></section>;
}

function UpdateSettings({ state, run, restartBlocked, t }: {
  state: DesktopUpdateState; run: () => void; restartBlocked: boolean;
  t: (key: MessageKey) => string;
}) {
  const active = ["checking", "downloading", "installing"].includes(state.kind);
  const reasonKey = state.kind === "failed" || state.kind === "refused"
    ? `settings.update.error.${state.reason}` as MessageKey : undefined;
  const statusKey = reasonKey ?? `settings.update.state.${state.kind}` as MessageKey;
  const actionKey: MessageKey | undefined = state.kind === "idle" ? "settings.update.check"
    : state.kind === "current" || state.kind === "refused" || (state.kind === "failed"
      && state.reason !== "update_outcome_unknown") ? "settings.update.checkAgain"
      : state.kind === "available" ? "settings.update.download"
      : state.kind === "ready_to_install" ? "settings.update.install"
      : state.kind === "restart_required" ? "settings.update.restart" : undefined;
  const target = "targetVersion" in state ? state.targetVersion : undefined;
  const progress = state.kind === "downloading" && state.totalBytes
    ? Math.min(100, Math.round(state.receivedBytes / state.totalBytes * 100)) : undefined;
  return <div className="settings-card update-card"><h2>{t("settings.update.title")}</h2>
    <p>{t("settings.update.description")}</p>
    <div className="setting-row"><span>{t("settings.update.currentVersion")}</span><code>{state.currentVersion}</code></div>
    {target && <div className="setting-row"><span>{t("settings.update.targetVersion")}</span><code>{target}</code></div>}
    <div className={reasonKey ? "update-status error" : "update-status"}
      role={reasonKey ? "alert" : "status"} aria-live="polite">
      {active && <span className="spinner" />}{t(statusKey)}
    </div>
    {state.kind === "downloading" && <progress max={100} value={progress}
      aria-label={t("settings.update.progress")} />}
    {actionKey && <button className="secondary-button" type="button" onClick={run}
      disabled={state.kind === "restart_required" && restartBlocked}>{t(actionKey)}</button>}
    {state.kind === "restart_required" && restartBlocked
      && <small>{t("settings.update.restartBlocked")}</small>}
  </div>;
}

function ThemeOptions({ value, onChange, t }: {
  value: DesktopTheme; onChange: (value: DesktopTheme) => void; t: (key: MessageKey) => string;
}) {
  return <span className="preference-options" role="radiogroup" aria-label={t("settings.theme.aria")}>
    {(["system", "light", "dark"] as const).map((theme) => <label className={value === theme ? "selected" : ""} key={theme}>
      <input className="sr-only" type="radio" name="desktop-theme" value={theme}
        checked={value === theme} onChange={() => onChange(theme)} />
      {t(`settings.theme.${theme}`)}
    </label>)}
  </span>;
}

function DensityOptions({ value, onChange, t }: {
  value: DesktopDensity; onChange: (value: DesktopDensity) => void; t: (key: MessageKey) => string;
}) {
  return <span className="preference-options" role="radiogroup" aria-label={t("settings.density.aria")}>
    {(["comfortable", "compact"] as const).map((density) => <label className={value === density ? "selected" : ""} key={density}>
      <input className="sr-only" type="radio" name="desktop-density" value={density}
        checked={value === density} onChange={() => onChange(density)} />
      {t(`settings.density.${density}`)}
    </label>)}
  </span>;
}
function LocaleOptions({ value, onChange, t }: {
  value: DesktopLocalePreference; onChange: (value: DesktopLocalePreference) => void;
  t: (key: MessageKey) => string;
}) {
  const locales: readonly DesktopLocalePreference[] = import.meta.env.DEV
    ? ["system", "en", "zh-Hans", "en-XA"] : ["system", "en", "zh-Hans"];
  return <span className="preference-options" role="radiogroup" aria-label={t("settings.language.label")}>
    {locales.map((locale) => <label className={value === locale ? "selected" : ""} key={locale}>
      <input className="sr-only" type="radio" name="desktop-locale" value={locale}
        checked={value === locale} onChange={() => onChange(locale)} />
      {t(`settings.language.${locale}`)}
    </label>)}
  </span>;
}
function StatusCard({ icon, title, body, action }: { icon: IconName; title: string; body: string; action?: string }) { return <div className="center-state"><span className="orb"><Icon name={icon} /></span><h1>{title}</h1><p>{body}</p>{action && <button className="primary-button" type="button" disabled>{action}</button>}</div>; }
function NavItem({ icon, label, selected, disabled, hint, onClick, soon = "Soon" }: { icon: IconName; label: string; selected?: boolean; disabled?: boolean; hint?: string; onClick?: () => void; soon?: string }) { return <button type="button" className={selected ? "nav-item selected" : "nav-item"} aria-label={label} disabled={disabled} title={hint} onClick={onClick}><Icon name={icon} /><span>{label}</span>{disabled && <small>{soon}</small>}</button>; }
function terminalCopy(terminal?: "running" | "completed" | "suspended" | "stopped" | "failed", t?: (key: MessageKey) => string) { const key = terminal === "completed" ? "status.completed" : terminal === "suspended" ? "status.needsInput" : terminal === "stopped" ? "status.stopped" : terminal === "failed" ? "status.failed" : "status.working"; return t ? t(key) : key === "status.completed" ? "Completed" : key === "status.needsInput" ? "Needs input" : key === "status.stopped" ? "Stopped" : key === "status.failed" ? "Failed" : "Working"; }
function TaskStateDot({ task }: { task: RecentTask }) {
  return <span className={`task-state-dot ${classifyTask(task)}`} aria-hidden="true" />;
}
function taskStateCopy(task: RecentTask, t: (key: MessageKey) => string) {
  const category = classifyTask(task);
  return category === "attention" ? t("tasks.needsInput")
    : category === "active" ? t("tasks.running")
      : category === "failed" ? t("tasks.failed")
        : category === "completed" ? terminalCopy(task.latest_turn_state, t) : t("tasks.ready");
}
function admittedTurnState(value?: string): RecentTask["latest_turn_state"] {
  return value === "running" || value === "completed" || value === "suspended"
    || value === "stopped" || value === "failed" ? value : undefined;
}
function recentLabel(session: RecentTask) {
  const opened = new Date(session.opened_at ?? "");
  return Number.isNaN(opened.valueOf())
    ? "Durable work"
    : `Work · ${opened.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
}

function downloadMarkdown(id: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/markdown;charset=utf-8" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `garive-result-${id.slice(0, 12)}.md`;
  anchor.click();
  URL.revokeObjectURL(url);
}

async function issueStartTurn(
  dispatch: (intent: AppIntent) => void, sessionId: string, input: string,
): Promise<void> {
  const commandId = commandIdentity("turn");
  const requestDigest = await semanticDigest({ kind: "start_turn", sessionId, input });
  dispatch({ type: "edit_draft", sessionId, text: input });
  dispatch({ type: "submit_draft", sessionId, commandId,
    requestDigest });
}

function commandIdentity(purpose: string): string {
  return `desktop-${purpose}-${crypto.randomUUID()}`;
}

async function semanticDigest(value: Record<string, string>): Promise<string> {
  const keys = Object.keys(value).sort();
  const canonical = `{${keys.map((key) => `${JSON.stringify(key)}:${JSON.stringify(value[key])}`).join(",")}}`;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(canonical));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
