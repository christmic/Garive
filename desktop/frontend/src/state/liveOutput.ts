import type { LiveOutputItem } from "./controller";

/** Decodes the closed H4 protocol without granting ephemeral output durable authority. */
export function decodeLiveOutput(value: unknown, sessionId: string): LiveOutputItem {
  const raw = record(value);
  if (raw.api_version !== "v1" || raw.session_id !== sessionId) invalid();
  const common = { turnId: identity(raw.turn_id), executionId: identity(raw.execution_id),
    streamId: identity(raw.stream_id), sequence: position(raw.sequence) };
  switch (raw.kind) {
    case "snapshot": {
      const throughSequence = position(raw.through_sequence);
      if (throughSequence !== common.sequence || typeof raw.text !== "string") invalid();
      return { ...common, kind: "snapshot", text: raw.text, throughSequence };
    }
    case "text_delta":
      if (typeof raw.text !== "string" || !raw.text) invalid();
      return { ...common, kind: "text_delta", text: raw.text };
    case "phase_changed":
      if (!PHASES.has(String(raw.phase)) || !LABELS.has(String(raw.label_key))) invalid();
      return { ...common, kind: "phase_changed", phase: String(raw.phase), labelKey: String(raw.label_key) };
    case "preview_unavailable": return { ...common, kind: "preview_unavailable" };
    case "ended":
      if (!END_REASONS.has(String(raw.reason))) invalid();
      return { ...common, kind: "ended", reason: String(raw.reason) };
    default: return invalid();
  }
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) invalid();
  return value as Record<string, unknown>;
}
function identity(value: unknown): string {
  if (typeof value !== "string" || !value || value.length > 128 || !/^[\x21-\x7e]+$/.test(value)) invalid();
  return value;
}
function position(value: unknown): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) invalid(); return Number(value);
}
function invalid(): never { throw new Error("invalid_live_output"); }
const PHASES = new Set(["preparing", "generating", "finalizing"]);
const LABELS = new Set(["agent.live.preparing", "agent.live.generating", "agent.live.finalizing"]);
const END_REASONS = new Set(["terminal_committed", "suspended", "stopped", "failed", "publisher_closed"]);
