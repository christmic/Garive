import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  attachWorkspaceToSession, chooseWorkspace, continueAgentTurn, createWorkSession,
  getDesktopCapabilities, getRecentSessions, getSessionTimeline, runAgentTurn,
  runAgentTurnWithWorkspaceContext, type HostSessionSummary, type WorkspaceEntry,
  type WorkspaceGrant,
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
  activity: false,
  setup: visualTestMode === "setup",
  workspaces: visualTestMode !== "setup",
  artifacts: false,
} as const;

export function App() {
  const [state, dispatch] = useReducer(reduceWork, initialWorkState);
  const [screen, setScreen] = useState<Screen>("work");
  const [recents, setRecents] = useState<readonly HostSessionSummary[]>([]);
  const [recentTitles, setRecentTitles] = useState<Readonly<Record<string, string>>>({});
  const [selectedContext, setSelectedContext] = useState<SelectedContext>();
  const [pickerGrant, setPickerGrant] = useState<WorkspaceGrant>();
  const [preparedSessionId, setPreparedSessionId] = useState<string>();
  const composer = useRef<HTMLTextAreaElement>(null);

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

  useEffect(() => {
    if (visualTest) {
      dispatch({ type: "capabilities_loaded", capabilities: visualCapabilities });
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
      if (suspended || result.terminal === "suspended" || state.capabilities?.activity) {
        dispatch({ type: "session_loaded", timeline: await getSessionTimeline(result.session_id) });
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

  const startSuggestion = (text: string) => {
    dispatch({ type: "draft_changed", value: text });
    requestAnimationFrame(() => composer.current?.focus());
  };

  const openRecent = async (sessionId: string) => {
    setScreen("work");
    setSelectedContext(undefined); setPreparedSessionId(undefined);
    try {
      const timeline = await getSessionTimeline(sessionId);
      dispatch({ type: "session_loaded", timeline });
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "projection_failure" });
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
              <span>{title}</span><small>{state.phase === "submitting" ? "Working" : "Completed"}</small>
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
          openContext={openContext} removeContext={() => setSelectedContext(undefined)} />
          : screen === "search" ? <SearchScreen recents={recents} titles={recentTitles} onOpen={openRecent} />
            : screen === "agents" ? <AgentsScreen definition={state.capabilities?.agent_definition_id} />
            : <SettingsScreen capabilities={state.capabilities} />}
      </main>
      {screen === "work" && state.inspectorOpen && <Inspector state={state} dispatch={dispatch} />}
    </div>
    {pickerGrant && <WorkspacePicker grant={pickerGrant} preview={visualTest}
      onCancel={() => setPickerGrant(undefined)} onConfirm={(entries) => {
        setSelectedContext({ grant: pickerGrant, entries }); setPickerGrant(undefined);
        requestAnimationFrame(() => composer.current?.focus());
      }} />}
  </>;
}

function WorkSurface({ state, composer, submit, startSuggestion, dispatch, context, openContext, removeContext }: {
  state: WorkState;
  composer: React.RefObject<HTMLTextAreaElement | null>;
  submit: () => Promise<void>;
  startSuggestion: (text: string) => void;
  dispatch: WorkDispatch;
  context?: SelectedContext;
  openContext: () => Promise<void>;
  removeContext: () => void;
}) {
  if (state.boot === "loading") return <div className="center-state"><span className="orb loading"><Icon name="sparkle" /></span><h1>Opening your workspace</h1><p>Recovering the local Runtime…</p></div>;
  if (state.boot === "unavailable") return <StatusCard icon="warning" title="Garive could not start" body={errorCopy.desktop_unavailable} />;
  if (!state.capabilities?.configured) {
    return state.capabilities?.setup ? <SetupFlow preview={visualTest} /> : <SetupRequired />;
  }
  const suspension = [...state.messages].reverse().find((message) => message.suspension)?.suspension;
  const needsInput = suspension?.kind === "partial_output" || suspension?.kind === "external_input_required";
  const blockedSuspension = Boolean(suspension && !needsInput);

  return <section className="work-surface">
    <div className={state.messages.length ? "conversation" : "conversation empty-conversation"}>
      {state.messages.length === 0 ? <Welcome onSelect={startSuggestion} /> : <Timeline state={state} />}
    </div>
    {state.error && <div className="error-banner" role="alert"><Icon name="warning" /><span>{errorCopy[state.error] ?? "This work could not continue."}</span>
      <button type="button" onClick={() => dispatch({ type: "error_dismissed" })} aria-label="Dismiss error"><Icon name="close" /></button></div>}
    <div className="composer-wrap">
      <div className={state.phase === "submitting" ? "composer busy" : "composer"}>
        {context && <div className="context-chips" aria-label="Context selected for next Turn">
          {context.entries.map((entry) => <span className="context-chip" key={entry.entry_id}>
            <Icon name="file" /><span><strong dir="auto">{entry.display_name}</strong>
              <small>{state.phase === "submitting" ? "Committing with Turn…" : context.grant.display_name}</small></span>
            <button type="button" disabled={state.phase === "submitting"} onClick={removeContext}
              aria-label={`Remove ${entry.display_name}`}><Icon name="close" /></button>
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
            <span className="access-pill"><Icon name="shield" />{needsInput ? "Resume exact suspension" : context ? `${context.entries.length} local ${context.entries.length === 1 ? "file" : "files"}` : "Local · text only"}</span></div>
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
  return <div className="timeline" aria-live="polite">{state.messages.map((message) => message.role === "user"
    ? <article className="message user-message" key={message.id}><div>{message.text}</div></article>
    : <article className="message assistant-message" key={message.id}><span className="message-mark"><Icon name="sparkle" /></span><div><div className="result-markdown"><Markdown skipHtml remarkPlugins={[remarkGfm]}
      components={{ a: ({ children }) => <span className="safe-link">{children}</span> }}>{message.text || terminalCopy(message.terminal)}</Markdown></div>
      <div className="result-meta"><span><Icon name={message.terminal === "completed" ? "check" : "warning"} />{terminalCopy(message.terminal)}</span><div className="result-actions"><button type="button" disabled={!message.text} onClick={() => downloadMarkdown(message.id, message.text)}>Export .md</button><button type="button" onClick={() => void copyResult(message.id, message.text)}>{copiedId === message.id ? "Copied" : "Copy"}</button></div></div></div></article>)}
    {state.phase === "submitting" && <article className="message assistant-message working"><span className="message-mark"><Icon name="sparkle" /></span><div><p>Working on your outcome…</p><span className="working-line" /></div></article>}
  </div>;
}

function Inspector({ state, dispatch }: { state: WorkState; dispatch: WorkDispatch }) {
  return <aside className="inspector" aria-label="Work inspector"><header><div className="inspector-tabs"><button className={state.inspectorTab === "activity" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "activity" })}>Activity</button><button className={state.inspectorTab === "artifacts" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "artifacts" })}>Artifacts</button></div>
    <button className="icon-button" type="button" aria-label="Close inspector" onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="close" /></button></header>
    {state.inspectorTab === "activity" ? <div className="inspector-body"><CommittedActivity state={state} /></div>
      : <div className="inspector-body"><ResultDeliverables state={state} /></div>}
  </aside>;
}

function ResultDeliverables({ state }: { state: WorkState }) {
  const results = state.messages.filter((message) => message.role === "assistant" && message.text);
  if (!results.length) return <div className="inspector-empty"><Icon name="file" /><h2>No deliverables yet</h2><p>Completed Markdown results become exportable here.</p></div>;
  return <div className="deliverable-list"><div className="activity-intro"><h2>Result deliverables</h2><p>Durable response projections from this Session.</p></div>
    {results.map((result, index) => <article className="deliverable-card" key={result.id}><span className="deliverable-icon"><Icon name="file" /></span><div><strong>Result {index + 1}.md</strong><p>{result.text.replace(/[#|*`>\[\]]/g, " ").trim().slice(0, 92)}</p><button type="button" onClick={() => downloadMarkdown(result.id, result.text)}>Export Markdown</button></div></article>)}
    {!state.capabilities?.artifacts && <p className="activity-gate"><Icon name="shield" />These are redacted Runtime results. Governed workspace files remain gated.</p>}
  </div>;
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
function SettingsScreen({ capabilities }: { capabilities?: WorkState["capabilities"] }) { const rows = [["Multi-turn work", capabilities?.multi_turn], ["Durable recents", capabilities?.durable_navigation], ["Committed activity", capabilities?.activity], ["Secure guided setup", capabilities?.setup], ["Local workspaces", capabilities?.workspaces], ["Artifact previews", capabilities?.artifacts]] as const; return <section className="content-page settings-page"><p className="eyebrow">DESKTOP</p><h1>Settings</h1><div className="settings-card"><h2>Local Runtime</h2><p>Capabilities are reported by the backend. Unavailable features remain gated.</p>{rows.map(([label, available]) => <div className="setting-row" key={label}><span>{label}</span><span className={available ? "state-chip ready" : "state-chip"}>{available ? "Available" : "Not installed"}</span></div>)}</div><div className="settings-card"><h2>Privacy</h2><p>Provider configuration and credentials stay in the Rust backend and macOS Keychain. This interface receives no secret, endpoint, database path, or raw Runtime fact.</p></div></section>; }
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
