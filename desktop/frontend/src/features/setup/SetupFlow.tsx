import { useEffect, useMemo, useRef, useState, type Ref } from "react";
import {
  cancelSetup, commitSetup, getSetupCatalogue, prepareSetup, restartDesktop,
  type SetupCatalogue, type SetupInput, type SetupPlan,
} from "../../ipc/host";
import { createTranslator, type MessageKey, type Translator } from "../../i18n";
import { ChoicePicker } from "../../ui/ChoicePicker";

type Stage = "details" | "review" | "ready";
type DetailsStep = "connection" | "runtime";

/** Injectable write-only setup boundary used by the product flow and UI tests. */
export interface SetupFlowApi {
  readonly catalogue: () => Promise<SetupCatalogue>;
  readonly prepare: (input: SetupInput) => Promise<SetupPlan>;
  readonly commit: (planDigest: string, credential: string) => Promise<unknown>;
  readonly cancel: (planDigest: string) => Promise<unknown>;
  readonly restart: () => Promise<void>;
}

export function SetupRecovery({ code, onRetry, onReview, t = createTranslator("en") }: {
  code: string; onRetry: () => void; onReview: () => void; t?: Translator;
}) {
  const known = (["secret_unavailable", "unknown_profile", "construction_failure",
    "read_failure", "invalid_document"] as const).find((item) => item === code);
  return <main className="setup-shell"><section className="setup-card setup-recovery"
    aria-labelledby="setup-recovery-title">
    <p className="setup-kicker">{t("setup.recovery.kicker")}</p>
    <h1 id="setup-recovery-title">{t("setup.recovery.title")}</h1>
    <p className="setup-lede">{t("setup.recovery.body")}</p>
    <div className="setup-recovery-status" role="status">
      <strong>{t("setup.recovery.preserved")}</strong>
      <span>{t(known ? `setup.recovery.${known}` : "setup.recovery.unknown")}</span>
    </div>
    <div className="setup-actions"><button type="button" onClick={onReview}>
      {t("setup.recovery.review")}</button><button className="primary" type="button"
      onClick={onRetry}>{t("setup.recovery.retry")}</button></div>
  </section></main>;
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

const ERROR_KEYS: Readonly<Record<string, MessageKey>> = {
  setup_not_allowed: "setup.error.notAllowed",
  setup_input_invalid: "setup.error.invalid",
  setup_plan_stale: "setup.error.stale",
  setup_plan_conflict: "setup.error.conflict",
  setup_credential_rejected: "setup.error.credential",
  setup_persistence_failed: "setup.error.persistence",
  setup_recovery_failed: "setup.error.recovery",
};

/** Renders the three-step first-run or reconfiguration flow without reading secrets or config. */
export function SetupFlow({
  api,
  nonce = () => crypto.randomUUID(),
  reconfigure = false,
  preview = false,
  t = createTranslator("en"),
}: {
  api?: SetupFlowApi;
  nonce?: () => string;
  reconfigure?: boolean;
  preview?: boolean;
  t?: Translator;
}) {
  const boundary = api ?? (preview ? PREVIEW_API : DEFAULT_API);
  const [catalogue, setCatalogue] = useState<SetupCatalogue>();
  const [stage, setStage] = useState<Stage>("details");
  const [detailsStep, setDetailsStep] = useState<DetailsStep>("connection");
  const [values, setValues] = useState<Record<string, string>>({});
  const [advanced, setAdvanced] = useState(false);
  const [plan, setPlan] = useState<SetupPlan>();
  const [credential, setCredential] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const credentialRef = useRef<HTMLInputElement>(null);
  const deploymentRef = useRef<HTMLInputElement>(null);
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

  const connectionValid = useMemo(() => catalogue !== undefined && [
    values.preset, values.profile, values.target, values.model,
  ].every((value) => bounded(value, catalogue.limits.max_text_bytes))
    && (!advanced || bounded(values.endpoint, catalogue.limits.max_endpoint_bytes)),
  [advanced, catalogue, values]);
  const runtimeValid = useMemo(() => catalogue !== undefined
    && [values.deployment, values.definition]
      .every((value) => bounded(value, catalogue.limits.max_text_bytes)), [catalogue, values]);
  const valid = connectionValid && runtimeValid;

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
    clearCredential(); setPlan(undefined); setError(""); setDetailsStep("runtime"); setStage("details");
  };

  const advanceDetails = () => {
    if (!connectionValid) return;
    setDetailsStep("runtime");
    queueMicrotask(() => deploymentRef.current?.focus());
  };

  if (!catalogue) return <main className="setup-shell"><p role="status">{t("setup.loading")}</p>
    {error && <p role="alert">{ERROR_KEYS[error] ? t(ERROR_KEYS[error]) : t("setup.unavailable")}</p>}</main>;

  return <main className="setup-shell"><section className="setup-card" aria-labelledby="setup-title">
    <h1 id="setup-title">{t(`setup.title.${stage}`)}</h1>
    <p className="setup-lede">{t("setup.lede")}</p>
    {reconfigure && <p className="setup-warning" role="note">{t("setup.reconfigure")}</p>}
    <ol className="setup-progress" aria-label={t("setup.progress")}><li aria-current={stage === "details" ? "step" : undefined}>{t("setup.step.connect")}</li><li aria-current={stage === "review" ? "step" : undefined}>{t("setup.step.review")}</li><li aria-current={stage === "ready" ? "step" : undefined}>{t("setup.step.restart")}</li></ol>

    {stage === "details" && <form onSubmit={(event) => { event.preventDefault();
      if (detailsStep === "connection") advanceDetails(); else void review(); }}>
      {detailsStep === "connection" ? <fieldset className="setup-step">
        <legend>{t("setup.connect.connection")}</legend><div className="setup-grid">
          <ChoicePicker label={t("setup.field.preset")} value={values.preset}
            onChange={(preset) => update("preset", preset)} options={catalogue.presets.map((item) => [item.preset_id, label(item.display_name_key)])} />
          <ChoicePicker label={t("setup.field.profile")} value={values.profile}
            onChange={(profile) => update("profile", profile)} options={catalogue.profiles.map((item) => [item.profile_id, label(item.display_name_key)])} />
          <Field label={t("setup.field.target")} value={values.target} change={(value) => update("target", value)} />
          <Field label={t("setup.field.model")} value={values.model} change={(value) => update("model", value)} />
        </div><button className="disclosure" type="button" aria-expanded={advanced} onClick={() => setAdvanced(!advanced)}>{t("setup.advanced")}</button>
        {advanced && <Field label={t("setup.field.endpoint")} value={values.endpoint} change={(value) => update("endpoint", value)} />}
      </fieldset> : <fieldset className="setup-step">
        <legend>{t("setup.connect.runtime")}</legend><div className="setup-grid">
          <Field inputRef={deploymentRef} label={t("setup.field.deployment")} value={values.deployment} change={(value) => update("deployment", value)} />
          <Field label={t("setup.field.agent")} value={values.definition} change={(value) => update("definition", value)} />
        </div>
      </fieldset>}
      <div className="setup-actions">{detailsStep === "runtime" && <button type="button" onClick={() => setDetailsStep("connection")}>{t("setup.back")}</button>}<button className="primary" disabled={detailsStep === "connection" ? !connectionValid : !valid || busy} type="submit">{t(detailsStep === "connection" ? "setup.continue" : busy ? "setup.preparing" : "setup.review")}</button></div>
    </form>}

    {stage === "review" && plan && <div><dl className="setup-summary">
      <Summary name={t("setup.summary.preset")} value={label(catalogue.presets.find((item) => item.preset_id === plan.summary.preset_id)?.display_name_key ?? plan.summary.preset_id)} />
      <Summary name={t("setup.summary.profile")} value={label(catalogue.profiles.find((item) => item.profile_id === plan.summary.profile_id)?.display_name_key ?? plan.summary.profile_id)} />
      <Summary name={t("setup.summary.model")} value={plan.summary.model_id} /><Summary name={t("setup.summary.agent")} value={plan.summary.definition_id} />
    </dl><label className="field">{t("setup.field.credential")}<input ref={credentialRef} autoFocus type="password" autoComplete="new-password" value={credential} onChange={(event) => setCredential(event.target.value)} /></label>
      <p className="field-note">{t("setup.credential.note")}</p>
      <div className="setup-actions"><button type="button" onClick={back}>{t("setup.back")}</button><button className="primary" type="button" disabled={!credential || busy} onClick={() => void commit()}>{t(busy ? "setup.committing" : "setup.commit")}</button></div></div>}

    {stage === "ready" && <div className="setup-ready"><p>{t("setup.ready")}</p><button className="primary" type="button" onClick={() => void boundary.restart()}>{t("setup.restart")}</button></div>}
    {error && <p className="setup-error" role="alert">{ERROR_KEYS[error] ? t(ERROR_KEYS[error]) : t("setup.error.default")}</p>}
  </section></main>;

  function update(key: string, value: string) { setValues((current) => ({ ...current, [key]: value })); }
  function clearCredential() {
    if (credentialRef.current) credentialRef.current.value = "";
    setCredential("");
  }
}

function Field({ label: name, value = "", change, inputRef }: { label: string; value?: string; change: (value: string) => void; inputRef?: Ref<HTMLInputElement> }) {
  return <label className="field">{name}<input ref={inputRef} value={value} onChange={(event) => change(event.target.value)} autoComplete="off" /></label>;
}
function Summary({ name, value }: { name: string; value: string }) { return <div><dt>{name}</dt><dd>{value}</dd></div>; }
function utf8Bytes(value: string) { return new TextEncoder().encode(value).byteLength; }
function bounded(value: string | undefined, max: number) { return Boolean(value?.trim()) && utf8Bytes(value!) <= max; }
function errorCode(cause: unknown) { return typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "setup_persistence_failed"; }
function label(key: string) { return key.split(".").at(-1)?.replaceAll("_", " ") ?? key; }
