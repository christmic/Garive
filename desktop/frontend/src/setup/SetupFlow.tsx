import { useEffect, useMemo, useState } from "react";
import {
  cancelSetup, commitSetup, getSetupCatalogue, prepareSetup, restartDesktop,
  type SetupCatalogue, type SetupInput, type SetupPlan,
} from "../ipc/host";
import { Icon } from "../ui/Icon";

type Stage = "details" | "review" | "ready";

const setupErrors: Record<string, string> = {
  setup_input_invalid: "Review the connection details and try again.",
  setup_plan_stale: "This setup plan expired. Review your choices again.",
  setup_plan_conflict: "This setup attempt conflicts with an existing review. Start again.",
  setup_credential_rejected: "The credential could not be saved to macOS Keychain.",
  setup_persistence_failed: "Garive could not commit its local configuration.",
};

export function SetupFlow({ preview = false }: { preview?: boolean }) {
  const [catalogue, setCatalogue] = useState<SetupCatalogue>();
  const [stage, setStage] = useState<Stage>("details");
  const [profileId, setProfileId] = useState("");
  const [modelId, setModelId] = useState("");
  const [definitionId, setDefinitionId] = useState("garive-work");
  const [advanced, setAdvanced] = useState(false);
  const [endpoint, setEndpoint] = useState("");
  const [plan, setPlan] = useState<SetupPlan>();
  const [credential, setCredential] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (preview) {
      setCatalogue({ schema_version: 1, catalogue_revision: "preview", max_text_bytes: 256,
        max_endpoint_bytes: 2048, max_secret_bytes: 16384, profiles: [
          { profile_id: "anthropic.messages.v1", display_name_key: "setup.profile.anthropic", endpoint_mode: "optional_override", supported_capabilities: ["text"] },
          { profile_id: "openai.responses.v1", display_name_key: "setup.profile.openai", endpoint_mode: "optional_override", supported_capabilities: ["text"] },
        ] });
      setProfileId("anthropic.messages.v1");
      return;
    }
    void getSetupCatalogue().then((value) => {
      setCatalogue(value);
      setProfileId(value.profiles[0]?.profile_id ?? "");
    }).catch(() => setError("setup_persistence_failed"));
  }, [preview]);

  const valid = useMemo(() => Boolean(
    catalogue && profileId && modelId.trim() && definitionId.trim()
      && (!advanced || endpoint.trim()),
  ), [advanced, catalogue, definitionId, endpoint, modelId, profileId]);

  const review = async () => {
    if (!catalogue || !valid) return;
    setBusy(true); setError(undefined);
    const input: SetupInput = {
      schema_version: 1,
      caller_nonce: crypto.randomUUID(),
      catalogue_revision: catalogue.catalogue_revision,
      profile_id: profileId,
      endpoint_override: advanced ? endpoint.trim() : undefined,
      model_target_id: "desktop-primary",
      model_id: modelId.trim(),
      deployment_id: "desktop-primary",
      definition_id: definitionId.trim(),
    };
    try {
      setPlan(preview ? {
        schema_version: 1, setup_id: "preview", caller_nonce: input.caller_nonce,
        catalogue_revision: input.catalogue_revision, effective_configuration_digest: "preview",
        expires_at: "2099-01-01T00:00:00Z",
        summary: { profile_id: input.profile_id, endpoint_mode: input.endpoint_override ? "override" : "fixed",
          endpoint_override: input.endpoint_override, model_target_id: input.model_target_id,
          model_id: input.model_id, deployment_id: input.deployment_id, definition_id: input.definition_id },
        plan_digest: "preview",
      } : await prepareSetup(input));
      setStage("review");
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "setup_input_invalid");
    } finally { setBusy(false); }
  };

  const commit = async () => {
    if (!plan || !credential) return;
    setBusy(true); setError(undefined);
    try {
      if (!preview) await commitSetup(plan.plan_digest, credential);
      setCredential("");
      setStage("ready");
    } catch (cause) {
      setCredential("");
      setError(typeof cause === "string" ? cause : "setup_persistence_failed");
    } finally { setBusy(false); }
  };

  const back = async () => {
    if (!plan) return;
    setBusy(true); setCredential(""); setError(undefined);
    try {
      if (!preview) await cancelSetup(plan.plan_digest);
      setPlan(undefined);
      setStage("details");
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "setup_plan_stale");
    } finally { setBusy(false); }
  };

  if (!catalogue && !error) {
    return <div className="center-state"><span className="orb loading"><Icon name="shield" /></span>
      <h1>Preparing secure setup</h1><p>Loading installed connection profiles…</p></div>;
  }

  return <section className="setup-flow">
    <div className="setup-ambient" aria-hidden="true" />
    <div className="setup-card">
      <header className="setup-heading">
        <span className="setup-logo"><Icon name={stage === "ready" ? "check" : "sparkle"} /></span>
        <div><p className="eyebrow">PRIVATE BY DESIGN</p>
          <h1>{stage === "details" ? "Make Garive yours" : stage === "review" ? "Review your workspace" : "Your workspace is ready"}</h1>
          <p>{stage === "details" ? "Connect one model. Your credential stays in macOS Keychain and never enters your work history."
            : stage === "review" ? "Garive will create a local Runtime from these choices. Nothing is sent until you begin work."
              : "The secure configuration is committed. Restart once to open your local Work surface."}</p></div>
      </header>

      <div className="setup-progress" aria-label={`Setup step ${stage === "details" ? 1 : stage === "review" ? 2 : 3} of 3`}>
        {["Connect", "Review", "Ready"].map((label, index) => <div className={(stage === "ready" ? 2 : stage === "review" ? 1 : 0) >= index ? "active" : ""} key={label}><span>{index + 1}</span>{label}</div>)}
      </div>

      {stage === "details" && catalogue && <div className="setup-fields">
        <fieldset><legend>Connection</legend><div className="profile-options">{catalogue.profiles.map((profile) => <button
          className={profileId === profile.profile_id ? "profile-option selected" : "profile-option"}
          type="button" key={profile.profile_id} onClick={() => setProfileId(profile.profile_id)}>
          <span className="profile-glyph">{profile.profile_id.startsWith("openai") ? "O" : "A"}</span>
          <span><strong>{profile.profile_id.startsWith("openai") ? "OpenAI" : "Anthropic"}</strong><small>Official {profile.supported_capabilities.join(" · ")} profile</small></span>
          <span className="radio-dot" /></button>)}</div></fieldset>
        <label>Model ID<input autoFocus value={modelId} maxLength={catalogue.max_text_bytes}
          onChange={(event) => setModelId(event.target.value)} placeholder="Enter the exact model ID" autoComplete="off" /></label>
        <label>Agent name<input value={definitionId} maxLength={catalogue.max_text_bytes}
          onChange={(event) => setDefinitionId(event.target.value)} autoComplete="off" /></label>
        <button className="advanced-toggle" type="button" aria-expanded={advanced} onClick={() => setAdvanced(!advanced)}>
          <Icon name="chevron" />Advanced endpoint override</button>
        {advanced && <label>HTTPS endpoint<input value={endpoint} maxLength={catalogue.max_endpoint_bytes}
          onChange={(event) => setEndpoint(event.target.value)} placeholder="https://…" inputMode="url" autoComplete="off" /></label>}
      </div>}

      {stage === "review" && plan && <div className="setup-review">
        <dl><div><dt>Connection</dt><dd>{providerName(plan.summary.profile_id)}</dd></div>
          <div><dt>Model</dt><dd>{plan.summary.model_id}</dd></div><div><dt>Agent</dt><dd>{plan.summary.definition_id}</dd></div>
          <div><dt>Data</dt><dd>Local SQLite · macOS Keychain</dd></div></dl>
        <label>API credential<div className="secure-input"><Icon name="shield" /><input type="password" autoFocus
          value={credential} onChange={(event) => setCredential(event.target.value)}
          placeholder="Paste once to save securely" autoComplete="new-password" /></div>
          <small>Write-only: Garive cannot display this value again.</small></label>
      </div>}

      {stage === "ready" && <div className="setup-ready"><div className="ready-orbit"><span><Icon name="check" /></span></div>
        <h2>Configuration committed</h2><p>Your credential is in Keychain and your Runtime configuration is ready locally.</p></div>}

      {error && <div className="setup-error" role="alert"><Icon name="warning" />{setupErrors[error] ?? "Setup could not continue."}</div>}
      <footer className="setup-actions">
        {stage === "review" && <button className="secondary-button" type="button" disabled={busy} onClick={() => void back()}>Back</button>}
        {stage === "details" && <button className="primary-button" type="button" disabled={!valid || busy} onClick={() => void review()}>{busy ? "Preparing…" : "Review setup"}<Icon name="chevron" /></button>}
        {stage === "review" && <button className="primary-button" type="button" disabled={!credential || busy} onClick={() => void commit()}>{busy ? "Saving securely…" : "Save to Keychain"}<Icon name="shield" /></button>}
        {stage === "ready" && <button className="primary-button" type="button" onClick={() => { if (!preview) void restartDesktop(); }}>Restart Garive<Icon name="chevron" /></button>}
      </footer>
    </div>
    <p className="setup-trust"><Icon name="shield" /> No environment discovery · No secret read API · No setup network request</p>
  </section>;
}

function providerName(profileId: string) {
  return profileId.startsWith("openai") ? "OpenAI Responses" : "Anthropic Messages";
}
