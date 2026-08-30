import { useEffect, useMemo, useRef, useState } from "react";
import {
  cancelSetup, commitSetup, getSetupCatalogue, prepareSetup, restartDesktop,
  type SetupCatalogue, type SetupInput, type SetupPlan,
} from "../../ipc/host";

type Stage = "details" | "review" | "ready";

/** Injectable write-only setup boundary used by the product flow and UI tests. */
export interface SetupFlowApi {
  readonly catalogue: () => Promise<SetupCatalogue>;
  readonly prepare: (input: SetupInput) => Promise<SetupPlan>;
  readonly commit: (planDigest: string, credential: string) => Promise<unknown>;
  readonly cancel: (planDigest: string) => Promise<unknown>;
  readonly restart: () => Promise<void>;
}

const DEFAULT_API: SetupFlowApi = {
  catalogue: getSetupCatalogue,
  prepare: prepareSetup,
  commit: commitSetup,
  cancel: cancelSetup,
  restart: restartDesktop,
};

const PREVIEW_API: SetupFlowApi = {
  catalogue: async () => ({ schema_version: 1, catalogue_revision: "preview-1",
    profiles: [{ profile_id: "openai-responses", display_name_key: "setup.profile.openai",
      endpoint_mode: "optional_override", model_mode: "exact_id",
      credential_label_key: "setup.credential.connection", supported_capabilities: ["text"] }],
    presets: [{ preset_id: "balanced", display_name_key: "setup.preset.balanced",
      supported_profile_ids: ["openai-responses"] }],
    limits: { max_profiles: 2, max_text_bytes: 256, max_endpoint_bytes: 2048,
      max_secret_bytes: 16384, max_plan_count: 16, plan_lifetime_seconds: 900 } }),
  prepare: async (input) => ({ schema_version: 1, setup_id: "preview-setup",
    caller_nonce: input.caller_nonce, catalogue_revision: input.catalogue_revision,
    effective_configuration_digest: "a".repeat(64), expires_at: "2030-01-01T00:00:00Z",
    summary: { ...input, endpoint_mode: input.endpoint_override ? "override" : "fixed" },
    plan_digest: "b".repeat(64) }),
  commit: async () => undefined, cancel: async () => undefined, restart: async () => undefined,
};

const ERROR_COPY: Readonly<Record<string, string>> = {
  setup_not_allowed: "This window is not allowed to change Desktop configuration.",
  setup_input_invalid: "Review the setup values and their limits.",
  setup_plan_stale: "This review expired. Prepare a new setup plan.",
  setup_plan_conflict: "These choices conflict with another setup attempt.",
  setup_credential_rejected: "The credential could not be stored securely.",
  setup_persistence_failed: "The local configuration could not be committed.",
  setup_recovery_failed: "A prior setup attempt needs diagnostics before continuing.",
};

/** Renders the three-step first-run or reconfiguration flow without reading secrets or config. */
export function SetupFlow({
  api,
  nonce = () => crypto.randomUUID(),
  reconfigure = false,
  preview = false,
}: {
  api?: SetupFlowApi;
  nonce?: () => string;
  reconfigure?: boolean;
  preview?: boolean;
}) {
  const boundary = api ?? (preview ? PREVIEW_API : DEFAULT_API);
  const [catalogue, setCatalogue] = useState<SetupCatalogue>();
  const [stage, setStage] = useState<Stage>("details");
  const [values, setValues] = useState<Record<string, string>>({});
  const [advanced, setAdvanced] = useState(false);
  const [plan, setPlan] = useState<SetupPlan>();
  const [credential, setCredential] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const credentialRef = useRef<HTMLInputElement>(null);
  const livePlan = useRef<string | undefined>(undefined);

  useEffect(() => {
    let active = true;
    void boundary.catalogue().then((loaded) => {
      if (!active) return;
      setCatalogue(loaded);
      setValues({ preset: loaded.presets[0]?.preset_id ?? "", profile: loaded.profiles[0]?.profile_id ?? "" });
    }).catch((cause) => active && setError(errorCode(cause)));
    return () => {
      active = false;
      const digest = livePlan.current;
      livePlan.current = undefined;
      if (digest) void boundary.cancel(digest);
    };
  }, [boundary]);

  const valid = useMemo(() => catalogue !== undefined && [
    values.preset, values.profile, values.target, values.model, values.deployment, values.definition,
  ].every((value) => bounded(value, catalogue.limits.max_text_bytes))
    && (!advanced || bounded(values.endpoint, catalogue.limits.max_endpoint_bytes)),
  [advanced, catalogue, values]);

  const review = async () => {
    if (!catalogue || !valid) return;
    setBusy(true); setError(""); setCredential("");
    const input: SetupInput = {
      schema_version: 1, caller_nonce: nonce(), catalogue_revision: catalogue.catalogue_revision,
      preset_id: values.preset!, profile_id: values.profile!,
      endpoint_override: advanced ? values.endpoint!.trim() : undefined,
      model_target_id: values.target!.trim(), model_id: values.model!.trim(),
      deployment_id: values.deployment!.trim(), definition_id: values.definition!.trim(),
    };
    try {
      const prepared = await boundary.prepare(input);
      setPlan(prepared); livePlan.current = prepared.plan_digest; setStage("review");
    } catch (cause) { setError(errorCode(cause)); }
    finally { setBusy(false); }
  };

  const commit = async () => {
    if (!plan || !credential || utf8Bytes(credential) > (catalogue?.limits.max_secret_bytes ?? 0)) return;
    const submitted = credential;
    setBusy(true); setError("");
    try {
      await boundary.commit(plan.plan_digest, submitted);
      livePlan.current = undefined; clearCredential(); setStage("ready");
    } catch (cause) {
      clearCredential(); setError(errorCode(cause));
      queueMicrotask(() => credentialRef.current?.focus());
    } finally { setBusy(false); }
  };

  const back = () => {
    const digest = livePlan.current;
    livePlan.current = undefined;
    if (digest) void boundary.cancel(digest);
    clearCredential(); setPlan(undefined); setError(""); setStage("details");
  };

  if (!catalogue) return <main className="setup-shell"><p role="status">Loading secure setup…</p>
    {error && <p role="alert">{ERROR_COPY[error] ?? "Setup is unavailable."}</p>}</main>;

  return <main className="setup-shell"><section className="setup-card" aria-labelledby="setup-title">
    <p className="eyebrow">LOCAL RUNTIME SETUP</p>
    <h1 id="setup-title">{stage === "details" ? "Configure Garive" : stage === "review" ? "Review setup" : "Restart required"}</h1>
    <p className="setup-lede">Credentials are submitted once to the operating-system store and are never readable from this app.</p>
    {reconfigure && <p className="setup-warning" role="note">Changes require an explicit restart. The current Runtime remains immutable until then.</p>}
    <ol className="setup-progress" aria-label="Setup progress"><li aria-current={stage === "details" ? "step" : undefined}>Connect</li><li aria-current={stage === "review" ? "step" : undefined}>Review</li><li aria-current={stage === "ready" ? "step" : undefined}>Restart</li></ol>

    {stage === "details" && <form onSubmit={(event) => { event.preventDefault(); void review(); }}>
      <div className="setup-grid">
        <Select label="Runtime preset" value={values.preset} change={(preset) => update("preset", preset)} options={catalogue.presets.map((item) => [item.preset_id, label(item.display_name_key)])} />
        <Select label="Connection profile" value={values.profile} change={(profile) => update("profile", profile)} options={catalogue.profiles.map((item) => [item.profile_id, label(item.display_name_key)])} />
        <Field label="Model target" value={values.target} change={(value) => update("target", value)} />
        <Field label="Model ID" value={values.model} change={(value) => update("model", value)} />
        <Field label="Deployment" value={values.deployment} change={(value) => update("deployment", value)} />
        <Field label="Agent definition" value={values.definition} change={(value) => update("definition", value)} />
      </div>
      <button className="disclosure" type="button" aria-expanded={advanced} onClick={() => setAdvanced(!advanced)}>Advanced endpoint override</button>
      {advanced && <Field label="HTTPS endpoint" value={values.endpoint} change={(value) => update("endpoint", value)} />}
      <div className="setup-actions"><button className="primary" disabled={!valid || busy} type="submit">{busy ? "Preparing…" : "Review setup"}</button></div>
    </form>}

    {stage === "review" && plan && <div><dl className="setup-summary">
      <Summary name="Preset" value={label(catalogue.presets.find((item) => item.preset_id === plan.summary.preset_id)?.display_name_key ?? plan.summary.preset_id)} />
      <Summary name="Profile" value={label(catalogue.profiles.find((item) => item.profile_id === plan.summary.profile_id)?.display_name_key ?? plan.summary.profile_id)} />
      <Summary name="Model" value={plan.summary.model_id} /><Summary name="Agent" value={plan.summary.definition_id} />
    </dl><label className="field">Credential<input ref={credentialRef} autoFocus type="password" autoComplete="new-password" value={credential} onChange={(event) => setCredential(event.target.value)} /></label>
      <p className="field-note">Write-only. This value is cleared after every commit attempt.</p>
      <div className="setup-actions"><button type="button" onClick={back}>Back</button><button className="primary" type="button" disabled={!credential || busy} onClick={() => void commit()}>{busy ? "Committing…" : "Commit configuration"}</button></div></div>}

    {stage === "ready" && <div className="setup-ready"><p>The new immutable configuration is committed. Restart to construct Runtime from it.</p><button className="primary" type="button" onClick={() => void boundary.restart()}>Restart Garive</button></div>}
    {error && <p className="setup-error" role="alert">{ERROR_COPY[error] ?? "Setup could not continue."}</p>}
  </section></main>;

  function update(key: string, value: string) { setValues((current) => ({ ...current, [key]: value })); }
  function clearCredential() {
    if (credentialRef.current) credentialRef.current.value = "";
    setCredential("");
  }
}

function Field({ label: name, value = "", change }: { label: string; value?: string; change: (value: string) => void }) {
  return <label className="field">{name}<input value={value} onChange={(event) => change(event.target.value)} autoComplete="off" /></label>;
}
function Select({ label: name, value = "", change, options }: { label: string; value?: string; change: (value: string) => void; options: readonly (readonly [string, string])[] }) {
  return <label className="field">{name}<select value={value} onChange={(event) => change(event.target.value)}>{options.map(([id, copy]) => <option key={id} value={id}>{copy}</option>)}</select></label>;
}
function Summary({ name, value }: { name: string; value: string }) { return <div><dt>{name}</dt><dd>{value}</dd></div>; }
function utf8Bytes(value: string) { return new TextEncoder().encode(value).byteLength; }
function bounded(value: string | undefined, max: number) { return Boolean(value?.trim()) && utf8Bytes(value!) <= max; }
function errorCode(cause: unknown) { return typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "setup_persistence_failed"; }
function label(key: string) { return key.split(".").at(-1)?.replaceAll("_", " ") ?? key; }
