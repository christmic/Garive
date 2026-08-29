/** Minimal Host v1 event fields consumed by the web fake-shell reducer. */
export interface HostEvent {
  readonly position: number;
  readonly event: string;
  readonly text?: string;
}

/** Deterministic ordered event stream used until a live durable Host exists. */
export const fakeEvents: readonly HostEvent[] = [
  { position: 1, event: "session.created" },
  { position: 2, event: "turn.started" },
  { position: 3, event: "output.delta", text: "hello " },
  { position: 4, event: "output.delta", text: "from Garive" },
  { position: 5, event: "turn.completed" },
];

/** Validates a complete fake Host stream and returns its concatenated output. */
export function runFakeHost(events: readonly HostEvent[] = fakeEvents): string {
  let previous = 0;
  let terminal = false;
  let output = "";
  for (const item of events) {
    if (terminal || item.position !== previous + 1) throw new Error("invalid Host API sequence");
    previous = item.position;
    if (item.event === "output.delta") output += item.text ?? "";
    if (item.event === "turn.completed") terminal = true;
  }
  if (!terminal) throw new Error("missing Host API terminal");
  return output;
}
