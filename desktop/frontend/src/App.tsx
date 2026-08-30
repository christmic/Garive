import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  attachWorkspaceToSession, authorizeWorkspaceWrites, chooseWorkspace, continueAgentTurn,
  createWorkSession, detachWorkspaceFromSession,
  getArtifactPreview, getDesktopCapabilities, getRecentSessions, getSessionTimeline,
  getSessionWorkspaces, getWorkspaceRecoveryStatus, listArtifacts, listWorkspaceAuthorizations, reauthorizeWorkspace,
  resolveTurnApproval, revokeWorkspace, runAgentTurn, runAgentTurnWithWorkspaceContext, commitArtifactExport,
  prepareArtifactExport, type ArtifactExportReceipt, type ArtifactPreview,
  type HostArtifact, type HostArtifactPage, type HostSessionSummary, type HostTimelinePage,
  type WorkspaceAuthorization,
  type WorkspaceAttachment, type WorkspaceEntry, type WorkspaceGrant, type WorkspaceRecoveryStatus,
} from "./ipc/host";
import { canSubmit, initialWorkState, reduceWork, type WorkState } from "./state/workspace";
import { Icon, type IconName } from "./ui/Icon";
import { SetupFlow } from "./setup/SetupFlow";
import { WorkspacePicker } from "./workspace/WorkspacePicker";

type Screen = "work" | "search" | "agents" | "settings";
type WorkDispatch = React.Dispatch<Parameters<typeof reduceWork>[1]>;
interface SelectedContext {
  readonly grant: WorkspaceGrant;
  readonly entries: readonly WorkspaceEntry[];
}

const suggestions = [
  ["Synthesize", "Turn notes into a clear decision memo"],
  ["Analyze", "Find the key patterns and recommend next steps"],
  ["Create", "Draft a polished project brief from my outline"],
] as const;

const errorCopy: Record<string, string> = {
  not_configured: "Finish Desktop setup before starting work.",
  invalid_configuration: "The local configuration needs attention.",
  host_failure: "Garive could not commit this request. Your draft is still here.",
  execution_failure: "The local Runtime could not finish this Turn.",
  projection_failure: "The result committed, but its public view is unavailable.",
  workspace_capability_invalid: "That local folder or file selection is no longer available.",
  workspace_unavailable: "Garive could not safely re-open the selected local file.",
  workspace_bound_exceeded: "Select fewer or smaller text files for this Turn.",
  desktop_unavailable: "The Desktop backend is unavailable. Restart Garive and try again.",
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

export function App() {
  const [state, dispatch] = useReducer(reduceWork, initialWorkState);
  const [screen, setScreen] = useState<Screen>("work");
  const [recents, setRecents] = useState<readonly HostSessionSummary[]>([]);
  const [recentTitles, setRecentTitles] = useState<Readonly<Record<string, string>>>({});
  const [selectedContext, setSelectedContext] = useState<SelectedContext>();
  const [pickerGrant, setPickerGrant] = useState<WorkspaceGrant>();
  const [preparedSessionId, setPreparedSessionId] = useState<string>();
  const [detachingWorkspaceId, setDetachingWorkspaceId] = useState<string>();
  const composer = useRef<HTMLTextAreaElement>(null);
  const approvalAction = useRef<HTMLButtonElement>(null);

  const refreshRecents = useCallback(async () => {
    const sessions = await getRecentSessions();
    setRecents(sessions);
    const titles = await Promise.all(sessions.map(async (session) => {
      try {
        const timeline = await getSessionTimeline(session.session_id);
        return [session.session_id, timeline.items[0]?.user_text ?? ""] as const;
      } catch { return [session.session_id, ""] as const; }
    }));
    setRecentTitles(Object.fromEntries(titles));
  }, []);

  const loadSession = useCallback(async (sessionId: string) => {
    const timeline = await getSessionTimeline(sessionId);
    dispatch({ type: "session_loaded", timeline });
    const [artifacts, workspaces] = await Promise.all([
      listArtifacts(sessionId), getSessionWorkspaces(sessionId),
    ]);
    dispatch({ type: "artifacts_loaded", page: artifacts });
    dispatch({ type: "workspaces_loaded", sessionId, workspaces });
  }, []);

  useEffect(() => {
    if (visualTest) {
      dispatch({ type: "capabilities_loaded", capabilities: visualCapabilities });
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
      return;
    }
    void getDesktopCapabilities()
      .then((capabilities) => {
        dispatch({ type: "capabilities_loaded", capabilities });
        if (capabilities.durable_navigation) {
          void refreshRecents().catch(() => setRecents([]));
        }
      })
      .catch(() => dispatch({ type: "capabilities_failed" }));
  }, [refreshRecents]);

  useEffect(() => {
    const shortcuts = (event: KeyboardEvent) => {
      if (!event.metaKey) return;
      if (event.key.toLowerCase() === "n") {
        event.preventDefault(); dispatch({ type: "new_work" }); setSelectedContext(undefined);
        setPreparedSessionId(undefined); setScreen("work");
        requestAnimationFrame(() => composer.current?.focus());
      }
      if (event.key === ",") { event.preventDefault(); setScreen("settings"); }
      if (event.key.toLowerCase() === "k") { event.preventDefault(); setScreen("search"); }
      if (event.key.toLowerCase() === "f") { event.preventDefault(); setScreen("search"); }
      if (event.shiftKey && event.key.toLowerCase() === "a") {
        event.preventDefault(); dispatch({ type: "inspector_toggled" });
      }
    };
    window.addEventListener("keydown", shortcuts);
    return () => window.removeEventListener("keydown", shortcuts);
  }, []);

  const title = useMemo(() => {
    const first = state.messages.find((message) => message.role === "user")?.text;
    return first ? first.slice(0, 54) : "New work";
  }, [state.messages]);

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
      const suspended = [...state.messages].reverse().find((message) => message.suspension);
      let result;
      if (suspended?.suspension && state.sessionId) {
        result = await continueAgentTurn(state.sessionId, suspended.id, suspended.suspension, input);
      } else if (selectedContext) {
        const sessionId = state.sessionId ?? preparedSessionId ?? await createWorkSession(definition);
        if (!state.sessionId && !preparedSessionId) setPreparedSessionId(sessionId);
        await attachWorkspaceToSession(sessionId, selectedContext.grant.workspace_id);
        result = await runAgentTurnWithWorkspaceContext(
          definition, sessionId, input, selectedContext.grant.workspace_id,
          selectedContext.entries.map((entry) => entry.entry_id),
        );
      } else {
        result = await runAgentTurn(definition, input, state.sessionId ?? preparedSessionId);
      }
      if (suspended || result.terminal === "suspended" || state.capabilities?.activity
          || state.capabilities?.artifacts) {
        await loadSession(result.session_id);
      } else {
        dispatch({ type: "submission_succeeded", input, result });
      }
      if (state.capabilities?.durable_navigation) {
        void refreshRecents().catch(() => undefined);
      }
      setSelectedContext(undefined);
      setPreparedSessionId(undefined);
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "host_failure" });
    }
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
      const result = await resolveTurnApproval(
        state.sessionId, message.id, message.suspension, approved,
      );
      await loadSession(result.session_id);
      void refreshRecents().catch(() => undefined);
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "host_failure" });
    }
  };

  const startSuggestion = (text: string) => {
    dispatch({ type: "draft_changed", value: text });
    requestAnimationFrame(() => composer.current?.focus());
  };

  const openRecent = async (sessionId: string) => {
    setScreen("work");
    setSelectedContext(undefined); setPreparedSessionId(undefined);
    try {
      await loadSession(sessionId);
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
        await loadSession(attachment.session_id);
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

  return <>
    <div className="app-shell" inert={Boolean(pickerGrant)} aria-hidden={Boolean(pickerGrant)}>
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="titlebar-drag" data-tauri-drag-region />
        <div className="brand"><span className="brand-mark"><Icon name="sparkle" /></span><span>Garive</span></div>
        <button className="new-work" type="button" onClick={() => { dispatch({ type: "new_work" }); setSelectedContext(undefined); setPreparedSessionId(undefined); setScreen("work"); }}>
          <Icon name="plus" /><span>New work</span><kbd>⌘N</kbd>
        </button>
        <nav className="nav-stack">
          <NavItem icon="work" label="Work" selected={screen === "work"} onClick={() => setScreen("work")} />
          <NavItem icon="search" label="Search" selected={screen === "search"}
            disabled={!state.capabilities?.durable_navigation} hint="Search durable work (⌘K)"
            onClick={() => setScreen("search")} />
        </nav>
        <div className="sidebar-section">
          <div className="section-label"><span>Recents</span>{!state.capabilities?.durable_navigation && <span className="beta-tag">Live</span>}</div>
          {recents.length > 0 ? recents.map((recent) => (
            <button className={recent.session_id === state.sessionId ? "recent-item selected" : "recent-item"}
              type="button" key={recent.session_id} onClick={() => void openRecent(recent.session_id)}>
              <span>{recent.session_id === state.sessionId && state.messages.length ? title : recentTitles[recent.session_id] || recentLabel(recent)}</span>
              <small>{recent.latest_turn_state ? terminalCopy(recent.latest_turn_state) : "Empty"}</small>
            </button>
          )) : state.messages.length > 0 ? (
            <button className="recent-item selected" type="button" onClick={() => setScreen("work")}>
              <span>{title}</span><small>{state.phase === "submitting" ? "Working"
                : terminalCopy(state.messages.at(-1)?.terminal)}</small>
            </button>
          ) : <p className="sidebar-empty">Durable work will appear here.</p>}
        </div>
        <div className="sidebar-section library">
          <div className="section-label">Library</div>
          <NavItem icon="agent" label="Agents" selected={screen === "agents"} onClick={() => setScreen("agents")} />
          <NavItem icon="memory" label="Memory" disabled hint="Requires M2-D" />
        </div>
        <div className="sidebar-footer">
          <NavItem icon="settings" label="Settings" selected={screen === "settings"} onClick={() => setScreen("settings")} />
          <div className={`runtime-state ${state.capabilities?.configured ? "online" : "offline"}`}>
            <span className="status-dot" /><span>{state.capabilities?.configured ? "Local Runtime ready" : "Setup required"}</span>
          </div>
        </div>
      </aside>

      <main className="main-surface">
        <header className="topbar" data-tauri-drag-region>
          <div className="topbar-title"><span>{screen === "work" ? title : screen === "search" ? "Search" : screen === "agents" ? "Agents" : "Settings"}</span>
            {screen === "work" && <span className="local-badge"><span />Local</span>}
            {visualTest && <span className="local-badge qa-badge">QA preview</span>}
          </div>
          <div className="topbar-actions">
            {screen === "work" && <button className={state.inspectorOpen ? "icon-button active" : "icon-button"}
              type="button" aria-label="Toggle inspector" title="Toggle inspector (⌘⇧A)"
              onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="panel" /></button>}
            <button className="avatar" type="button" aria-label="Account and app menu">G</button>
          </div>
        </header>

        {screen === "work" ? <WorkSurface state={state} composer={composer} submit={submit}
          startSuggestion={startSuggestion} dispatch={dispatch} context={selectedContext}
          openContext={openContext} authorizeOutputs={authorizeOutputs}
          resolveApproval={resolveApproval} removeContext={() => setSelectedContext(undefined)}
          detachWorkspace={detachWorkspace} detachingWorkspaceId={detachingWorkspaceId}
          approvalAction={approvalAction} />
          : screen === "search" ? <SearchScreen recents={recents} titles={recentTitles} onOpen={openRecent} />
            : screen === "agents" ? <AgentsScreen definition={state.capabilities?.agent_definition_id} />
            : <SettingsScreen capabilities={state.capabilities} />}
      </main>
      {screen === "work" && state.inspectorOpen && <Inspector state={state} dispatch={dispatch} />}
    </div>
    {pickerGrant && <WorkspacePicker grant={pickerGrant} preview={visualTest}
      onCancel={() => { setPickerGrant(undefined);
        requestAnimationFrame(() => composer.current?.focus()); }} onConfirm={(entries) => {
        setSelectedContext({ grant: pickerGrant, entries }); setPickerGrant(undefined);
        requestAnimationFrame(() => composer.current?.focus());
      }} />}
  </>;
}

function WorkSurface({ state, composer, submit, startSuggestion, dispatch, context, openContext,
  authorizeOutputs, resolveApproval, removeContext, detachWorkspace, detachingWorkspaceId,
  approvalAction }: {
  state: WorkState;
  composer: React.RefObject<HTMLTextAreaElement | null>;
  submit: () => Promise<void>;
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
}) {
  if (state.boot === "loading") return <div className="center-state"><span className="orb loading"><Icon name="sparkle" /></span><h1>Opening your workspace</h1><p>Recovering the local Runtime…</p></div>;
  if (state.boot === "unavailable") return <StatusCard icon="warning" title="Garive could not start" body={errorCopy.desktop_unavailable} />;
  if (!state.capabilities?.configured) {
    return state.capabilities?.setup ? <SetupFlow preview={visualTest} /> : <SetupRequired />;
  }
  const suspension = [...state.messages].reverse().find((message) => message.suspension)?.suspension;
  const needsInput = suspension?.kind === "partial_output" || suspension?.kind === "external_input_required";
  const blockedSuspension = Boolean(suspension && !needsInput);
  const needsApproval = suspension?.kind === "approval_required";
  const approvalEffect = [...state.activities].reverse().find((activity) =>
    activity.kind === "tool" && !activity.terminal);
  const approvalWorkspace = state.workspaces.find((workspace) => workspace.access === "read_write")
    ?? state.workspaces[0];

  return <section className="work-surface">
    <div className={state.messages.length ? "conversation" : "conversation empty-conversation"}>
      {state.messages.length === 0 ? <Welcome onSelect={startSuggestion} /> : <Timeline state={state} />}
    </div>
    {state.error && <div className="error-banner" role="alert"><Icon name="warning" /><span>{errorCopy[state.error] ?? "This work could not continue."}</span>
      <button type="button" onClick={() => dispatch({ type: "error_dismissed" })} aria-label="Dismiss error"><Icon name="close" /></button></div>}
    <div className="composer-wrap">
      <div className={state.phase === "submitting" ? "composer busy" : "composer"}>
        {needsApproval && <div className="approval-card" role="alert" aria-live="assertive" aria-label="Workspace write approval required">
          <span className="approval-icon"><Icon name="shield" /></span><div><strong>{approvalEffect
            ? `${activityLabel(approvalEffect.label_key)} in ` : "Approve one local operation in "}<bdi>{approvalWorkspace?.display_name ?? "the attached Workspace"}</bdi>?</strong>
            <div className="approval-facts"><span><b>Scope</b>{approvalWorkspace?.access === "read_write" ? "Create one new file" : "Exact prepared operation"}</span>
              <span><b>Duration</b>Once · this prepared call only</span><span><b>Overwrite</b>Never</span></div>
            <p>A changed request, Workspace grant, or destination requires a new approval.</p></div>
          <div className="approval-actions"><button ref={approvalAction} type="button" autoFocus disabled={state.phase === "submitting"}
            onClick={() => void resolveApproval(false)}>Decline</button><button className="primary" type="button"
              disabled={state.phase === "submitting"} onClick={() => void resolveApproval(true)}>Approve once</button></div>
        </div>}
        {state.workspaces.length > 0 && <div className="attached-workspaces"
          aria-label="Workspaces attached to this work">
          {state.workspaces.map((workspace) => <span className="context-chip workspace-chip"
            key={`${workspace.workspace_id}-${workspace.grant_revision}`}>
            <Icon name="work" /><span><strong dir="auto">{workspace.display_name}</strong>
              <small>{workspace.access === "read_write" ? "Read and output" : "Read-only"} · attached</small></span>
            <button type="button" title="Detach from this work"
              aria-label="Detach Workspace from this work"
              disabled={state.phase === "submitting" || Boolean(detachingWorkspaceId)}
              onClick={() => void detachWorkspace(workspace)}>{detachingWorkspaceId === workspace.workspace_id
                ? <span className="spinner" /> : <Icon name="close" />}</button>
          </span>)}</div>}
        {context && <div className="context-chips" aria-label="Context selected for next Turn">
          {context.entries.map((entry) => <span className="context-chip" key={entry.entry_id}>
            <Icon name="file" /><span><strong dir="auto">{entry.display_name}</strong>
              <small>{state.phase === "submitting" ? "Committing with Turn…" : context.grant.display_name}</small></span>
            <button type="button" disabled={state.phase === "submitting"} onClick={removeContext}
              aria-label="Remove selected context file"><Icon name="close" /></button>
          </span>)}</div>}
        <textarea ref={composer} value={state.draft} disabled={state.phase === "submitting" || blockedSuspension}
          aria-label={needsInput ? "Continue suspended work" : "Describe the outcome you want"}
          placeholder={blockedSuspension ? "This suspension requires a governed action." : needsInput ? "Provide the input needed to continue…" : "Describe the outcome you want…"}
          onChange={(event) => dispatch({ type: "draft_changed", value: event.target.value })}
          onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void submit(); } }} />
        <div className="composer-toolbar">
          <div className="composer-tools"><button type="button"
            disabled={!state.capabilities?.workspaces || state.phase === "submitting" || Boolean(suspension)}
            title={state.capabilities?.workspaces ? "Choose local text files" : "Local Workspaces are not installed"}
            onClick={() => void openContext()}><Icon name="paperclip" /><span>Add context</span></button>
            {context?.grant.access === "enumerate" && <button type="button" disabled={state.phase === "submitting"}
              onClick={() => void authorizeOutputs()}><Icon name="shield" /><span>Allow outputs</span></button>}
            <span className="access-pill"><Icon name="shield" />{needsInput ? "Resume exact suspension"
              : context?.grant.access === "read_write" ? "Output folder enabled"
                : context ? `${context.entries.length} local ${context.entries.length === 1 ? "file" : "files"}` : "Local · text only"}</span></div>
          <button className="send-button" type="button" disabled={!canSubmit(state)} aria-label="Send work" onClick={() => void submit()}>
            {state.phase === "submitting" ? <span className="spinner" /> : <Icon name="send" />}
          </button>
        </div>
      </div>
      <p className="composer-note">Garive shows results only after they are committed by the local Runtime.</p>
    </div>
  </section>;
}

function Welcome({ onSelect }: { onSelect: (text: string) => void }) {
  return <div className="welcome"><span className="hero-mark"><Icon name="sparkle" /></span><p className="eyebrow">LOCAL WORK AGENT</p>
    <h1>What should we accomplish?</h1><p className="welcome-copy">Describe the finished outcome. Garive will keep the work local and make durable results clear.</p>
    <div className="suggestion-grid">{suggestions.map(([label, text]) => <button type="button" key={label} onClick={() => onSelect(text)}><span>{label}</span><p>{text}</p><Icon name="chevron" /></button>)}</div>
  </div>;
}

function Timeline({ state }: { state: WorkState }) {
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
  const announcement = state.phase === "submitting" ? "Garive is working."
    : latest?.role === "assistant" ? `Turn ${terminalCopy(latest.terminal).toLocaleLowerCase()}.` : "";
  return <div className="timeline">{state.messages.map((message) => message.role === "user"
    ? <article className="message user-message" key={message.id}><div>{message.text}</div></article>
    : <article className="message assistant-message" key={message.id}><span className="message-mark"><Icon name="sparkle" /></span><div><div className="result-markdown"><Markdown skipHtml remarkPlugins={[remarkGfm]}
      components={{ a: ({ children }) => <span className="safe-link">{children}</span> }}>{message.text || terminalCopy(message.terminal)}</Markdown></div>
      <div className="result-meta"><span><Icon name={message.terminal === "completed" ? "check" : "warning"} />{terminalCopy(message.terminal)}</span><div className="result-actions"><button type="button" disabled={!message.text} onClick={() => downloadMarkdown(message.id, message.text)}>Export .md</button><button type="button" onClick={() => void copyResult(message.id, message.text)}>{copiedId === message.id ? "Copied" : "Copy"}</button></div></div></div></article>)}
    {state.phase === "submitting" && <article className="message assistant-message working"><span className="message-mark"><Icon name="sparkle" /></span><div><p>Working on your outcome…</p><span className="working-line" /></div></article>}
    <p className="sr-only" aria-live="polite" aria-atomic="true">{announcement}</p>
  </div>;
}

function Inspector({ state, dispatch }: { state: WorkState; dispatch: WorkDispatch }) {
  return <aside className="inspector" aria-label="Work inspector"><header><div className="inspector-tabs" role="tablist" aria-label="Inspector views"><button type="button" role="tab" aria-selected={state.inspectorTab === "activity"} className={state.inspectorTab === "activity" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "activity" })}>Activity</button><button type="button" role="tab" aria-selected={state.inspectorTab === "artifacts"} className={state.inspectorTab === "artifacts" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "artifacts" })}>Artifacts</button></div>
    <button className="icon-button" type="button" aria-label="Close inspector" onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="close" /></button></header>
    {state.inspectorTab === "activity" ? <div className="inspector-body" role="tabpanel"><CommittedActivity state={state} /></div>
      : <div className="inspector-body" role="tabpanel"><ResultDeliverables state={state} /></div>}
  </aside>;
}

function ResultDeliverables({ state }: { state: WorkState }) {
  const [selected, setSelected] = useState<HostArtifact>();
  const [preview, setPreview] = useState<ArtifactPreview>();
  const [previewState, setPreviewState] = useState<"idle" | "loading" | "unavailable">("idle");
  const [exportStates, setExportStates] = useState<Readonly<Record<string,
    "exporting" | "exported" | "exists" | "unavailable">>>({});
  const [exportReceipts, setExportReceipts] = useState<Readonly<Record<string,
    ArtifactExportReceipt>>>({});
  const results = state.messages.filter((message) => message.role === "assistant" && message.text);
  const openPreview = async (artifact: HostArtifact) => {
    setSelected(artifact); setPreview(undefined); setPreviewState("loading");
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
  if (!results.length && !state.artifacts.length) return <div className="inspector-empty"><Icon name="file" /><h2>No deliverables yet</h2><p>Committed results and created files will appear here.</p></div>;
  return <div className="deliverable-list"><div className="activity-intro"><h2>Deliverables</h2><p>Immutable results committed by the local Runtime.</p></div>
    {state.artifacts.map((artifact) => { const key = `${artifact.artifact_id}-${artifact.revision}`;
      const exportState = exportStates[key]; const receipt = exportReceipts[key];
      return <article className="artifact-card" key={key}>
      <span className="deliverable-icon"><Icon name="file" /></span><div className="artifact-card-body">
        <div className="artifact-title"><strong dir="auto">{artifact.display_name}</strong><span>v{artifact.revision}</span></div>
        <p>{formatBytes(artifact.byte_size)} · {artifact.mime_type} · Committed</p>
        <div className="artifact-actions"><div><button type="button" disabled={artifact.preview !== "text"}
          onClick={() => void openPreview(artifact)}>Preview</button><button type="button"
            disabled={!artifact.exportable || exportState === "exporting"}
            onClick={() => void exportCopy(artifact)}>{exportState === "exporting" ? "Choosing…" : "Export copy…"}</button></div>
          {artifact.workspace_id && <span><Icon name="shield" />Authorized Workspace</span>}</div>
        {exportState === "exported" && receipt && <p className="artifact-export-state success" role="status"><Icon name="check" />Exported as {receipt.display_name}</p>}
        {exportState === "exists" && <p className="artifact-export-state error" role="alert"><Icon name="warning" />Choose a new file name; Garive never overwrites.</p>}
        {exportState === "unavailable" && <p className="artifact-export-state error" role="alert"><Icon name="warning" />Export unavailable. Check Workspace access and try again.</p>}
      </div>
    </article>; })}
    {selected && <section className="artifact-preview" aria-label="Verified Artifact preview"><header><div><span>VERIFIED PREVIEW</span><strong dir="auto">{selected.display_name}</strong></div><button type="button" aria-label="Close Artifact preview"
      onClick={() => { setSelected(undefined); setPreview(undefined); setPreviewState("idle"); }}><Icon name="close" /></button></header>
      {previewState === "loading" ? <div className="preview-state" role="status"><span className="spinner" />Verifying committed bytes…</div>
        : previewState === "unavailable" ? <div className="preview-state error" role="alert"><Icon name="warning" />The backing file changed or access is unavailable.</div>
          : preview && <pre>{preview.content_utf8}</pre>}
      <footer><Icon name="shield" />SHA-256 checked against revision {selected.revision}</footer>
    </section>}
    {results.length > 0 && <div className="deliverable-section-label">Response snapshots</div>}
    {results.map((result, index) => <article className="deliverable-card" key={result.id}><span className="deliverable-icon"><Icon name="file" /></span><div><strong>Result {index + 1}.md</strong><p>{result.text.replace(/[#|*`>\[\]]/g, " ").trim().slice(0, 92)}</p><button type="button" onClick={() => downloadMarkdown(result.id, result.text)}>Export Markdown</button></div></article>)}
    {!state.capabilities?.artifacts && <p className="activity-gate"><Icon name="shield" />These are redacted Runtime results. Governed workspace files remain gated.</p>}
  </div>;
}

function formatBytes(bytes: number) {
  return bytes < 1_024 ? `${bytes} B` : `${(bytes / 1_024).toFixed(bytes < 10_240 ? 1 : 0)} KB`;
}

function CommittedActivity({ state }: { state: WorkState }) {
  if (state.capabilities?.activity && state.activities.length) {
    const activities = [...state.activities].sort((left, right) =>
      Number(right.state === "attention_required") - Number(left.state === "attention_required")
        || left.source_position - right.source_position);
    return <div className="activity-list"><div className="activity-intro"><h2>Committed activity</h2><p>Redacted lifecycle states verified by the local Runtime.</p></div>
      {activities.map((activity) => <div className="activity-row" key={`${activity.kind}-${activity.activity_id}`}>
        <span className={`activity-status ${activity.state}`}><Icon name={activityIcon(activity.state)} /></span>
        <div><strong>{activityLabel(activity.label_key)}</strong><small>{activityState(activity.state)}</small></div>
      </div>)}
    </div>;
  }
  const turns = state.messages.filter((message) => message.role === "assistant");
  if (!turns.length && state.phase !== "submitting") return <div className="inspector-empty"><Icon name="activity" /><h2>No committed Turns yet</h2><p>Durable Turn states appear here after work begins.</p></div>;
  return <div className="activity-list"><div className="activity-intro"><h2>Turn activity</h2><p>Only states committed by the local Runtime are shown.</p></div>
    {turns.map((turn, index) => <div className="activity-row" key={turn.id}><span className={`activity-status ${turn.terminal ?? "running"}`}><Icon name={turn.terminal === "completed" ? "check" : "warning"} /></span>
      <div><strong>Turn {index + 1}</strong><small>{terminalCopy(turn.terminal)}</small></div></div>)}
    {state.phase === "submitting" && <div className="activity-row"><span className="activity-status running"><span className="spinner" /></span><div><strong>Current Turn</strong><small>Working</small></div></div>}
    {!state.capabilities?.activity && <p className="activity-gate"><Icon name="shield" />Tool-level details stay hidden until the H3 committed projection is installed.</p>}
  </div>;
}

function activityLabel(key: string) {
  const labels: Record<string, string> = {
    "agent.activity.read_file": "Read scoped file",
    "agent.activity.write_file": "Write scoped file",
    "agent.activity.approval": "Approval decision",
    "agent.activity.external_input": "Requested input",
    "agent.activity.tool_rejected": "Rejected tool request",
  };
  return labels[key] ?? "Agent activity";
}
function activityState(state: string) {
  const labels: Record<string, string> = {
    prepared: "Prepared", waiting_for_input: "Waiting for input", input_received: "Input received",
    authorized: "Authorized", running: "Running", completed: "Completed", denied: "Denied",
    failed: "Failed", cancelled: "Cancelled", attention_required: "Needs reconciliation",
  };
  return labels[state] ?? "Updated";
}
function activityIcon(state: string): IconName {
  return state === "completed" || state === "input_received" ? "check"
    : state === "running" || state === "authorized" || state === "prepared" ? "activity"
      : "warning";
}

function SearchScreen({ recents, titles, onOpen }: {
  recents: readonly HostSessionSummary[];
  titles: Readonly<Record<string, string>>;
  onOpen: (sessionId: string) => Promise<void>;
}) {
  const [query, setQuery] = useState("");
  const results = recents.filter((recent) => {
    const searchable = `${titles[recent.session_id] ?? ""} ${recent.definition_id}`.toLocaleLowerCase();
    return searchable.includes(query.trim().toLocaleLowerCase());
  });
  return <section className="search-page"><div className="search-heading"><p className="eyebrow">DURABLE HISTORY</p><h1>Find your work</h1><p>Search the first request from recent local Sessions. No cloud index is created.</p></div>
    <div className="search-box"><Icon name="search" /><input autoFocus aria-label="Search durable work"
      value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search recent work…" /><kbd>⌘K</kbd></div>
    <div className="search-results" aria-live="polite">{results.length ? results.map((recent) => <button type="button" key={recent.session_id} onClick={() => void onOpen(recent.session_id)}>
      <span className="search-result-icon"><Icon name="work" /></span><span><strong>{titles[recent.session_id] || recentLabel(recent)}</strong><small>{recent.turn_count} {recent.turn_count === 1 ? "Turn" : "Turns"} · {terminalCopy(recent.latest_turn_state)}</small></span><Icon name="chevron" /></button>)
      : <div className="search-empty"><Icon name="search" /><h2>{query ? "No matching work" : "No durable work yet"}</h2><p>{query ? "Try a different word from the original request." : "Completed Sessions will become searchable here."}</p></div>}</div>
  </section>;
}

function SetupRequired() { return <StatusCard icon="shield" title="Connect the local Runtime" body="Garive found no Desktop configuration. The secure guided setup is not installed in this build yet; add desktop-v1.json and its credential to the macOS Keychain, then restart." action="View setup status" />; }
function AgentsScreen({ definition }: { definition?: string }) { return <section className="content-page"><p className="eyebrow">INSTALLED LOCALLY</p><h1>Your Agents</h1><p>Agents define the stable behavior and capabilities available to new work.</p><div className="agent-card"><span className="agent-avatar"><Icon name="agent" /></span><div><h2>{definition ?? "No Agent configured"}</h2><p>{definition ? "Ready for local text work" : "Complete Desktop configuration to install an Agent."}</p></div><span className={definition ? "state-chip ready" : "state-chip"}>{definition ? "Ready" : "Unavailable"}</span></div></section>; }
function SettingsScreen({ capabilities }: { capabilities?: WorkState["capabilities"] }) {
  const [workspaceRecovery, setWorkspaceRecovery] = useState<WorkspaceRecoveryStatus>();
  const [authorizations, setAuthorizations] = useState<readonly WorkspaceAuthorization[]>([]);
  const [restoring, setRestoring] = useState<string>();
  const [confirmingRevocation, setConfirmingRevocation] = useState<string>();
  const [revoking, setRevoking] = useState<string>();
  const [recoveryError, setRecoveryError] = useState<string>();
  const [recoveryNotice, setRecoveryNotice] = useState<string>();
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
    }); setRecoveryError("Workspace recovery status is unavailable."); });
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
        ? "That is not the original Workspace folder. Choose the same folder to restore access."
        : "Garive could not restore this folder safely.");
    } finally { setRestoring(undefined); }
  };
  const removeAccess = async (workspace: WorkspaceAuthorization) => {
    if (confirmingRevocation !== workspace.workspace_id) {
      setConfirmingRevocation(workspace.workspace_id); setRecoveryNotice(
        "Confirm removal. This immediately blocks future reads and outputs; prior receipts remain.",
      );
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
        ? "Access is revoked. Private Keychain cleanup will retry safely after restart."
        : "Workspace access was removed.");
      requestAnimationFrame(() => workspaceHeading.current?.focus());
    } catch {
      setRecoveryError("Garive could not durably revoke this Workspace. Access was not broadened.");
    } finally { setRevoking(undefined); setConfirmingRevocation(undefined); }
  };
  const rows = [["Multi-turn work", capabilities?.multi_turn], ["Durable recents", capabilities?.durable_navigation], ["Committed activity", capabilities?.activity], ["Secure guided setup", capabilities?.setup], ["Local workspaces", capabilities?.workspaces], ["Artifact previews", capabilities?.artifacts]] as const;
  const recoveryReady = workspaceRecovery?.state === "ready";
  return <section className="content-page settings-page"><p className="eyebrow">DESKTOP</p><h1>Settings</h1><div className="settings-card"><h2>Local Runtime</h2><p>Capabilities are reported by the backend. Unavailable features remain gated.</p>{rows.map(([label, available]) => <div className="setting-row" key={label}><span>{label}</span><span className={available ? "state-chip ready" : "state-chip"}>{available ? "Available" : "Not installed"}</span></div>)}</div>
    {capabilities?.workspaces && <div className="settings-card"><h2 ref={workspaceHeading} tabIndex={-1}>Workspace access</h2><p>Folder access is restored from read-only bookmarks stored in macOS Keychain. No filesystem path enters this interface.</p><div className="setting-row"><span>Authorization recovery</span><span className={recoveryReady ? "state-chip ready" : "state-chip attention"}>{workspaceRecovery ? recoveryReady ? `${workspaceRecovery.restored_count} restored` : workspaceRecovery.state === "attention_required" ? `${workspaceRecovery.needs_reauthorization_count} needs access` : "Index unavailable" : "Checking…"}</span></div>
      {authorizations.map((workspace) => <div className="workspace-auth-row" key={workspace.workspace_id}><span className="workspace-auth-icon"><Icon name="work" /></span><span><strong dir="auto">{workspace.display_name}</strong><small>{workspace.state === "active" ? `Read-only access · revision ${workspace.grant_revision}` : "Access expired · choose the original folder"}</small></span><span className="workspace-auth-actions">{workspace.state === "active" ? <span className="state-chip ready">Active</span> : <button className="secondary-button" type="button" disabled={restoring === workspace.workspace_id || Boolean(revoking)} onClick={() => void restoreAccess(workspace)}>{restoring === workspace.workspace_id ? <><span className="spinner" />Opening…</> : "Restore access"}</button>}<button className={confirmingRevocation === workspace.workspace_id ? "danger-button confirming" : "danger-button"} type="button" aria-label="Remove Workspace access" disabled={Boolean(restoring) || Boolean(revoking)} onClick={() => void removeAccess(workspace)}>{revoking === workspace.workspace_id ? <><span className="spinner" />Removing…</> : confirmingRevocation === workspace.workspace_id ? "Confirm remove" : "Remove access"}</button></span></div>)}
      {recoveryNotice && <div className="workspace-recovery-notice" role="status"><Icon name="shield" /><span>{recoveryNotice}</span></div>}
      {recoveryError && <div className="workspace-recovery-error" role="alert"><Icon name="warning" /><span>{recoveryError}</span></div>}
    </div>}
    <div className="settings-card"><h2>Privacy</h2><p>Provider configuration and credentials stay in the Rust backend and macOS Keychain. This interface receives no secret, endpoint, database path, bookmark data, or raw Runtime fact.</p></div></section>;
}
function StatusCard({ icon, title, body, action }: { icon: IconName; title: string; body: string; action?: string }) { return <div className="center-state"><span className="orb"><Icon name={icon} /></span><h1>{title}</h1><p>{body}</p>{action && <button className="primary-button" type="button" disabled>{action}</button>}</div>; }
function NavItem({ icon, label, selected, disabled, hint, onClick }: { icon: IconName; label: string; selected?: boolean; disabled?: boolean; hint?: string; onClick?: () => void }) { return <button type="button" className={selected ? "nav-item selected" : "nav-item"} disabled={disabled} title={hint} onClick={onClick}><Icon name={icon} /><span>{label}</span>{disabled && <small>Soon</small>}</button>; }
function terminalCopy(terminal?: "running" | "completed" | "suspended" | "stopped" | "failed") { return terminal === "completed" ? "Completed" : terminal === "suspended" ? "Needs input" : terminal === "stopped" ? "Stopped" : terminal === "failed" ? "Failed" : "Working"; }
function recentLabel(session: HostSessionSummary) {
  const opened = new Date(session.opened_at);
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
