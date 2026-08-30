import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import {
  getDesktopCapabilities, getRecentSessions, getSessionTimeline, runAgentTurn,
  type HostSessionSummary,
} from "./ipc/host";
import { canSubmit, initialWorkState, reduceWork, type WorkState } from "./state/workspace";
import { Icon, type IconName } from "./ui/Icon";

type Screen = "work" | "agents" | "settings";
type WorkDispatch = React.Dispatch<Parameters<typeof reduceWork>[1]>;

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
  desktop_unavailable: "The Desktop backend is unavailable. Restart Garive and try again.",
};

const visualTest = import.meta.env.DEV
  && new URLSearchParams(window.location.search).has("visual-test");
const visualCapabilities = {
  configured: true,
  agent_definition_id: "garive-work",
  multi_turn: true,
  durable_navigation: false,
  activity: false,
  setup: false,
  workspaces: false,
  artifacts: false,
} as const;

export function App() {
  const [state, dispatch] = useReducer(reduceWork, initialWorkState);
  const [screen, setScreen] = useState<Screen>("work");
  const [recents, setRecents] = useState<readonly HostSessionSummary[]>([]);
  const composer = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (visualTest) {
      dispatch({ type: "capabilities_loaded", capabilities: visualCapabilities });
      return;
    }
    void getDesktopCapabilities()
      .then((capabilities) => {
        dispatch({ type: "capabilities_loaded", capabilities });
        if (capabilities.durable_navigation) {
          void getRecentSessions().then(setRecents).catch(() => setRecents([]));
        }
      })
      .catch(() => dispatch({ type: "capabilities_failed" }));
  }, []);

  useEffect(() => {
    const shortcuts = (event: KeyboardEvent) => {
      if (!event.metaKey) return;
      if (event.key.toLowerCase() === "n") {
        event.preventDefault(); dispatch({ type: "new_work" }); setScreen("work");
        requestAnimationFrame(() => composer.current?.focus());
      }
      if (event.key === ",") { event.preventDefault(); setScreen("settings"); }
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
        text: "I turned the outcome into a concise work brief with decisions, risks, and the next actions clearly separated.",
        terminal: "completed",
      } });
      return;
    }
    try {
      const result = await runAgentTurn(definition, input, state.sessionId);
      dispatch({ type: "submission_succeeded", input, result });
      if (state.capabilities?.durable_navigation) {
        void getRecentSessions().then(setRecents).catch(() => undefined);
      }
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
    try {
      const timeline = await getSessionTimeline(sessionId);
      dispatch({ type: "session_loaded", timeline });
    } catch (cause) {
      dispatch({ type: "submission_failed", code: typeof cause === "string" ? cause : "projection_failure" });
    }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="titlebar-drag" data-tauri-drag-region />
        <div className="brand"><span className="brand-mark"><Icon name="sparkle" /></span><span>Garive</span></div>
        <button className="new-work" type="button" onClick={() => { dispatch({ type: "new_work" }); setScreen("work"); }}>
          <Icon name="plus" /><span>New work</span><kbd>⌘N</kbd>
        </button>
        <nav className="nav-stack">
          <NavItem icon="work" label="Work" selected={screen === "work"} onClick={() => setScreen("work")} />
          <NavItem icon="search" label="Search" disabled hint="Requires H2" />
        </nav>
        <div className="sidebar-section">
          <div className="section-label"><span>Recents</span>{!state.capabilities?.durable_navigation && <span className="beta-tag">Live</span>}</div>
          {recents.length > 0 ? recents.map((recent) => (
            <button className={recent.session_id === state.sessionId ? "recent-item selected" : "recent-item"}
              type="button" key={recent.session_id} onClick={() => void openRecent(recent.session_id)}>
              <span>{recent.session_id === state.sessionId && state.messages.length ? title : recentLabel(recent)}</span>
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
          <div className="topbar-title"><span>{screen === "work" ? title : screen === "agents" ? "Agents" : "Settings"}</span>
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

        {screen === "work" ? <WorkSurface state={state} composer={composer} submit={submit} startSuggestion={startSuggestion} dispatch={dispatch} />
          : screen === "agents" ? <AgentsScreen definition={state.capabilities?.agent_definition_id} />
            : <SettingsScreen capabilities={state.capabilities} />}
      </main>
      {screen === "work" && state.inspectorOpen && <Inspector state={state} dispatch={dispatch} />}
    </div>
  );
}

function WorkSurface({ state, composer, submit, startSuggestion, dispatch }: {
  state: WorkState;
  composer: React.RefObject<HTMLTextAreaElement | null>;
  submit: () => Promise<void>;
  startSuggestion: (text: string) => void;
  dispatch: WorkDispatch;
}) {
  if (state.boot === "loading") return <div className="center-state"><span className="orb loading"><Icon name="sparkle" /></span><h1>Opening your workspace</h1><p>Recovering the local Runtime…</p></div>;
  if (state.boot === "unavailable") return <StatusCard icon="warning" title="Garive could not start" body={errorCopy.desktop_unavailable} />;
  if (!state.capabilities?.configured) return <SetupRequired />;

  return <section className="work-surface">
    <div className={state.messages.length ? "conversation" : "conversation empty-conversation"}>
      {state.messages.length === 0 ? <Welcome onSelect={startSuggestion} /> : <Timeline state={state} />}
    </div>
    {state.error && <div className="error-banner" role="alert"><Icon name="warning" /><span>{errorCopy[state.error] ?? "This work could not continue."}</span>
      <button type="button" onClick={() => dispatch({ type: "error_dismissed" })} aria-label="Dismiss error"><Icon name="close" /></button></div>}
    <div className="composer-wrap">
      <div className={state.phase === "submitting" ? "composer busy" : "composer"}>
        <textarea ref={composer} value={state.draft} disabled={state.phase === "submitting"}
          aria-label="Describe the outcome you want" placeholder="Describe the outcome you want…"
          onChange={(event) => dispatch({ type: "draft_changed", value: event.target.value })}
          onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void submit(); } }} />
        <div className="composer-toolbar">
          <div className="composer-tools"><button type="button" disabled title="Workspace attachments require opaque capabilities"><Icon name="paperclip" /><span>Add context</span></button>
            <span className="access-pill"><Icon name="shield" />Local · text only</span></div>
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
    : <article className="message assistant-message" key={message.id}><span className="message-mark"><Icon name="sparkle" /></span><div><p>{message.text || terminalCopy(message.terminal)}</p>
      <div className="result-meta"><span><Icon name={message.terminal === "completed" ? "check" : "warning"} />{terminalCopy(message.terminal)}</span><button type="button" onClick={() => void copyResult(message.id, message.text)}>{copiedId === message.id ? "Copied" : "Copy"}</button></div></div></article>)}
    {state.phase === "submitting" && <article className="message assistant-message working"><span className="message-mark"><Icon name="sparkle" /></span><div><p>Working on your outcome…</p><span className="working-line" /></div></article>}
  </div>;
}

function Inspector({ state, dispatch }: { state: WorkState; dispatch: WorkDispatch }) {
  return <aside className="inspector" aria-label="Work inspector"><header><div className="inspector-tabs"><button className={state.inspectorTab === "activity" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "activity" })}>Activity</button><button className={state.inspectorTab === "artifacts" ? "active" : ""} onClick={() => dispatch({ type: "inspector_selected", tab: "artifacts" })}>Artifacts</button></div>
    <button className="icon-button" type="button" aria-label="Close inspector" onClick={() => dispatch({ type: "inspector_toggled" })}><Icon name="close" /></button></header>
    {state.inspectorTab === "activity" ? <div className="inspector-body"><div className="inspector-empty"><Icon name="activity" /><h2>{state.phase === "submitting" ? "Turn in progress" : "Committed activity"}</h2><p>{state.capabilities?.activity ? "Runtime activity appears here." : "Detailed activity arrives with the H3 committed projection. Garive will not invent steps from animations or logs."}</p></div></div>
      : <div className="inspector-body"><div className="inspector-empty"><Icon name="file" /><h2>No artifacts yet</h2><p>Verified files and deliverables will appear here when the artifact capability is installed.</p></div></div>}
  </aside>;
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
