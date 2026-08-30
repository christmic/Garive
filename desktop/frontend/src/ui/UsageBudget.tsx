import { Icon } from "./Icon";

export type UsageBudgetState = "normal" | "watch" | "critical" | "exhausted";
export interface UsageBudgetSnapshot {
  readonly source: "included_plan" | "workspace_credits" | "provider_api" | "execution";
  readonly state: UsageBudgetState;
  readonly scopeLabel: string;
  readonly periodLabel: string;
  readonly remainingPercent?: number;
  readonly resetsAtLabel?: string;
  readonly attribution: "reported" | "estimated";
  readonly modelPostureLabel?: string;
  readonly activeTurnMayFinish: boolean;
}

export function validUsageBudget(value: UsageBudgetSnapshot): boolean {
  return value.scopeLabel.trim().length > 0 && value.periodLabel.trim().length > 0
    && (value.remainingPercent === undefined || (Number.isInteger(value.remainingPercent)
      && value.remainingPercent >= 0 && value.remainingPercent <= 100));
}

export function UsageBudgetTrigger({ value, label, onOpen }: {
  value: UsageBudgetSnapshot;
  label: string;
  onOpen: () => void;
}) {
  if (!validUsageBudget(value)) return null;
  return <button className={`usage-trigger ${value.state}`} type="button" onClick={onOpen}
    aria-label={`${label}: ${usageAmount(value)}`}>
    <span className="usage-trigger-meter" aria-hidden="true"
      style={{ "--usage-value": value.remainingPercent ?? 0 } as React.CSSProperties} />
    <span><strong>{label}</strong><small>{usageAmount(value)}</small></span>
  </button>;
}

export function UsageBudgetCard({ value, copy }: {
  value: UsageBudgetSnapshot;
  copy: {
    title: string; description: string; remaining: string; reported: string; estimated: string;
    reset: string; modelPosture: string; activeMayFinish: string; activeMayStop: string;
  };
}) {
  if (!validUsageBudget(value)) return null;
  const percentage = value.remainingPercent;
  return <section className={`settings-card usage-card ${value.state}`} aria-labelledby="usage-title">
    <header><span className="usage-card-icon"><Icon name="activity" /></span><span>
      <h2 id="usage-title">{copy.title}</h2><p>{copy.description}</p></span>
      <span className={`state-chip ${value.state === "normal" ? "ready" : "attention"}`}>
        {value.scopeLabel}</span></header>
    <div className="usage-summary">
      <div><strong>{percentage === undefined ? "—" : `${percentage}%`}</strong>
        <span>{copy.remaining}</span></div>
      <div className="usage-facts"><span><b>{value.periodLabel}</b>
        <small>{value.attribution === "reported" ? copy.reported : copy.estimated}</small></span>
        {value.resetsAtLabel && <span><b>{value.resetsAtLabel}</b><small>{copy.reset}</small></span>}
        {value.modelPostureLabel && <span><b>{value.modelPostureLabel}</b>
          <small>{copy.modelPosture}</small></span>}</div>
    </div>
    {percentage !== undefined && <progress className="usage-progress" max={100} value={percentage}
      aria-label={`${percentage}% ${copy.remaining}`} />}
    <div className="usage-policy"><Icon name={value.activeTurnMayFinish ? "check" : "warning"} />
      <span>{value.activeTurnMayFinish ? copy.activeMayFinish : copy.activeMayStop}</span></div>
  </section>;
}

function usageAmount(value: UsageBudgetSnapshot): string {
  return value.remainingPercent === undefined ? value.periodLabel
    : `${value.remainingPercent}% · ${value.periodLabel}`;
}
