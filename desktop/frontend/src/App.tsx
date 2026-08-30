import { useEffect, useState } from "react";
import { SetupFlow, type SetupFlowApi } from "./features/setup/SetupFlow";
import { getSetupState, type SetupState } from "./ipc/host";

/** Injectable Desktop composition boundary for configured-state UI tests. */
export interface AppApi {
  readonly setupState: () => Promise<SetupState>;
  readonly setupFlow?: SetupFlowApi;
}

const DEFAULT_API: AppApi = { setupState: getSetupState };

/** Selects the redacted setup or configured route without reading effective configuration. */
export function App({ api = DEFAULT_API }: { api?: AppApi }) {
  const [state, setState] = useState<SetupState>();
  const [showSetup, setShowSetup] = useState(false);
  const [unavailable, setUnavailable] = useState(false);

  useEffect(() => {
    let active = true;
    void api.setupState().then((next) => active && setState(next)).catch(() => active && setUnavailable(true));
    return () => { active = false; };
  }, [api]);

  if (unavailable) return <main className="status-shell"><section className="status-card"><h1>Garive could not start</h1><p role="alert">The Desktop backend is unavailable. Restart the app and open diagnostics if this continues.</p></section></main>;
  if (!state || state.state === "setup_recovering") return <main className="status-shell"><p role="status">Recovering secure setup…</p></main>;
  if (showSetup || state.state === "not_configured") return <SetupFlow api={api.setupFlow} reconfigure={state.state !== "not_configured"} />;

  if (state.state === "invalid_configuration") return <main className="status-shell"><section className="status-card">
    <p className="eyebrow">CONFIGURATION ATTENTION</p><h1>Garive needs reconfiguration</h1>
    <p role="alert">The stored configuration was rejected without starting Runtime.</p>
    <details><summary>Diagnostics</summary><code>{state.code}</code></details>
    <button className="primary" type="button" onClick={() => setShowSetup(true)}>Reconfigure</button>
  </section></main>;

  return <main className="status-shell"><section className="status-card">
    <p className="eyebrow">LOCAL RUNTIME</p><h1>{state.restart_required ? "Restart required" : "Garive is configured"}</h1>
    <p>{state.restart_required ? "Restart to activate the committed configuration." : "The embedded Runtime is ready for the product workspace."}</p>
    <button type="button" onClick={() => setShowSetup(true)}>Reconfigure</button>
  </section></main>;
}
