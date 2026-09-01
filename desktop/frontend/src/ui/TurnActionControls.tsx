import type { ReactNode } from "react";
import type { HostResult } from "../ipc/host";
import { Icon } from "./Icon";

interface TurnActionControlsProps {
  readonly terminal: HostResult["terminal"] | undefined;
  readonly terminalLabel: string;
  readonly children: ReactNode;
}

/** Stable answer-tail geometry; individual actions remain progressive. */
export function TurnActionControls({ terminal, terminalLabel, children }: TurnActionControlsProps) {
  const completed = terminal === "completed";
  return <div className={completed ? "result-meta" : "result-meta attention"}
    data-terminal={terminal} data-turn-action-controls="">
    <span className={completed ? "result-terminal sr-only" : "result-terminal"}>
      <Icon name={completed ? "check" : "warning"} />{terminalLabel}
    </span>
    <div className="result-actions">{children}</div>
  </div>;
}
