/** Exact H1 public event consumed by browser presentation state. */
export interface HostEvent {
  readonly api_version: string; readonly session_id: string; readonly position: number;
  readonly event: string; readonly turn_id: string; readonly execution_id: string; readonly text: string;
}
export type HostTerminal = "completed" | "suspended" | "stopped" | "failed";
/** Ephemeral browser projection; durable truth remains in H1. */
export interface HostView {
  readonly cursor: number; readonly terminal?: HostTerminal; readonly text: string;
  readonly unknownEvents: readonly string[]; readonly fingerprints: Readonly<Record<number, string>>;
}
export interface HostClientLimits {
  readonly maxCommandBytes: number; readonly maxEventBytes: number;
  readonly maxEvents: number; readonly followDeadlineMs: number;
}
export const HOST_CLIENT_FAILURES = [
  "invalid_configuration", "invalid_command", "invalid_event", "event_order_violation",
  "event_limit_exceeded", "host_failure", "unknown_host_error", "authentication_required",
  "actor_forbidden", "device_reauth_required", "rate_limited", "runtime_unavailable",
  "pairing_rejected", "transport_failure", "follow_deadline",
] as const;
export type HostClientFailure = typeof HOST_CLIENT_FAILURES[number];
/** Stable safe client failure without command, event, header, or body content. */
export class HostClientError extends Error {
  public constructor(public readonly code: HostClientFailure, public readonly status?: number) {
    super(code); this.name = "HostClientError";
  }
}
export interface CreateSessionResponse {
  readonly session_id: string; readonly agent_instance_id: string; readonly committed_position: number;
}
export interface TurnCommandResponse {
  readonly session_id: string; readonly turn_id: string;
  readonly execution_id: string; readonly committed_position: number;
}
export type HostReadDocument = Record<string, unknown>;

const KNOWN_EVENTS = new Set([
  "session.created", "turn.started", "turn.completed", "turn.suspended", "turn.stopped", "turn.failed",
]);
const KNOWN_HOST_ERRORS = new Set([
  "invalid_request", "not_found", "command_conflict", "concurrent_modification",
  "precondition_failed", "durability_unavailable", "corrupt_state",
]);
const KNOWN_CLIENT_ERRORS = new Set<HostClientFailure>([
  "authentication_required", "actor_forbidden", "device_reauth_required", "rate_limited",
  "runtime_unavailable", "pairing_rejected",
]);

/** Reduces ordered replay/follow events without treating EOF as terminal. */
export function reduceHostEvents(
  sessionId: string, events: readonly HostEvent[],
  initial: HostView = { cursor: 0, text: "", unknownEvents: [], fingerprints: {} }, maxEvents = 16,
): HostView {
  if (!sessionId || maxEvents <= 0) throw new HostClientError("invalid_configuration");
  if (events.length > maxEvents) throw new HostClientError("event_limit_exceeded");
  let cursor = initial.cursor; let terminal = initial.terminal; let text = initial.text;
  const savedCursor = initial.cursor;
  const unknownEvents = [...initial.unknownEvents];
  const fingerprints: Record<number, string> = { ...initial.fingerprints };
  for (const event of events) {
    if (event.api_version !== "v1" || event.session_id !== sessionId ||
        !Number.isSafeInteger(event.position) || event.position <= 0) throw new HostClientError("invalid_event");
    const fingerprint = JSON.stringify(event); const prior = fingerprints[event.position];
    if (prior !== undefined) {
      if (prior !== fingerprint) throw new HostClientError("event_order_violation");
      continue;
    }
    if (event.position <= savedCursor) continue;
    if (event.position <= cursor) throw new HostClientError("event_order_violation");
    if (terminal !== undefined) throw new HostClientError("event_order_violation");
    cursor = event.position; fingerprints[event.position] = fingerprint;
    switch (event.event) {
      case "turn.completed": terminal = "completed"; text = event.text; break;
      case "turn.suspended": terminal = "suspended"; break;
      case "turn.stopped": terminal = "stopped"; break;
      case "turn.failed": terminal = "failed"; break;
      default: if (!KNOWN_EVENTS.has(event.event) && !unknownEvents.includes(event.event)) unknownEvents.push(event.event);
    }
  }
  return { cursor, terminal, text, unknownEvents, fingerprints };
}

export type FetchLike = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
/** Explicit loopback HTTP/SSE implementation of A1. */
export class FetchHostClient {
  private readonly baseUrl: string;
  public constructor(baseUrl: string, private readonly limits: HostClientLimits, private readonly fetcher: FetchLike = fetch) {
    this.baseUrl = validateBaseUrl(baseUrl);
    if ([limits.maxCommandBytes, limits.maxEventBytes, limits.maxEvents, limits.followDeadlineMs]
      .some((value) => !Number.isSafeInteger(value) || value <= 0)) throw new HostClientError("invalid_configuration");
  }
  public async createSession(commandId: string, definitionId: string): Promise<CreateSessionResponse> {
    return validateSessionResponse(await this.post("/v1/sessions", commandId, { agent_definition_id: definitionId }));
  }
  public async startTurn(commandId: string, sessionId: string, text: string): Promise<TurnCommandResponse> {
    if (!sessionId) throw new HostClientError("invalid_command");
    const result = validateTurnResponse(await this.post(
      `/v1/sessions/${encodeURIComponent(sessionId)}/turns`, commandId, { text },
    ));
    if (result.session_id !== sessionId) throw new HostClientError("invalid_event");
    return result;
  }
  public async cancelTurn(
    commandId: string, sessionId: string, turnId: string, requestedThroughPosition: number,
  ): Promise<TurnCommandResponse> {
    if (!sessionId || !turnId || !position(requestedThroughPosition)) throw new HostClientError("invalid_command");
    return validateOwnedTurnResponse(await this.post(
      `/v1/turns/${encodeURIComponent(turnId)}:cancel`, commandId,
      { session_id: sessionId, requested_through_position: requestedThroughPosition },
    ), sessionId, turnId);
  }
  public async continueTurn(
    commandId: string, sessionId: string, turnId: string, suspensionId: string,
    expectedSessionVersion: number, input: string,
  ): Promise<TurnCommandResponse> {
    if (!sessionId || !turnId || !suspensionId || !position(expectedSessionVersion) || !input) {
      throw new HostClientError("invalid_command");
    }
    return validateOwnedTurnResponse(await this.post(
      `/v1/turns/${encodeURIComponent(turnId)}:continue`, commandId,
      { session_id: sessionId, suspension_id: suspensionId,
        expected_session_version: expectedSessionVersion, input },
    ), sessionId, turnId);
  }
  public async continueTurnInput(
    commandId: string, sessionId: string, turnId: string, suspensionId: string,
    expectedSessionVersion: number, input: string | boolean,
  ): Promise<TurnCommandResponse> {
    if (!sessionId || !turnId || !suspensionId || !position(expectedSessionVersion) || input === "") {
      throw new HostClientError("invalid_command");
    }
    const body: Record<string, string | number | boolean> = {
      session_id: sessionId, suspension_id: suspensionId,
      expected_session_version: expectedSessionVersion,
    };
    if (typeof input === "boolean") body.input_json = input; else body.input = input;
    return validateOwnedTurnResponse(await this.post(
      `/v1/turns/${encodeURIComponent(turnId)}:continue`, commandId, body,
    ), sessionId, turnId);
  }
  public readDefinitions(): Promise<HostReadDocument> {
    return this.get("/v1/agent-definitions");
  }
  public readSessions(limit = 64): Promise<HostReadDocument> {
    if (!Number.isSafeInteger(limit) || limit <= 0) throw new HostClientError("invalid_command");
    return this.get(`/v1/sessions?limit=${limit}`);
  }
  public readTimeline(sessionId: string, afterPosition = 0, limit = 128): Promise<HostReadDocument> {
    if (!sessionId || !Number.isSafeInteger(afterPosition) || afterPosition < 0 ||
        !Number.isSafeInteger(limit) || limit <= 0) throw new HostClientError("invalid_command");
    return this.get(`/v1/sessions/${encodeURIComponent(sessionId)}/timeline?after_position=${afterPosition}&limit=${limit}`);
  }
  public async *followEvents(sessionId: string, afterPosition = 0, signal?: AbortSignal): AsyncIterable<HostEvent> {
    if (!sessionId || !Number.isSafeInteger(afterPosition) || afterPosition < 0) throw new HostClientError("invalid_command");
    const response = await this.requestEventStream(sessionId, afterPosition, signal);
    const reader = response.body!.getReader(); const decoder = new TextDecoder("utf-8", { fatal: true });
    let pending = ""; let count = 0;
    try {
      while (!signal?.aborted) {
        const chunk = await reader.read();
        if (chunk.done) throw new HostClientError("transport_failure");
        pending += decoder.decode(chunk.value, { stream: true });
        let boundary: number;
        while ((boundary = pending.indexOf("\n\n")) >= 0) {
          const block = pending.slice(0, boundary).replaceAll("\r", ""); pending = pending.slice(boundary + 2);
          const data = block.split("\n").filter((line) => line.startsWith("data: "))
            .map((line) => line.slice(6)).join("\n");
          if (!data) continue;
          if (new TextEncoder().encode(data).length > this.limits.maxEventBytes || ++count > this.limits.maxEvents) {
            throw new HostClientError("event_limit_exceeded");
          }
          let event: unknown;
          try { event = JSON.parse(data); } catch { throw new HostClientError("invalid_event"); }
          validateEvent(event, sessionId); yield event;
        }
      }
    } finally { await reader.cancel().catch(() => undefined); }
  }
  public async followUntilTerminal(sessionId: string, afterPosition = 0): Promise<HostView> {
    if (!sessionId || !Number.isSafeInteger(afterPosition) || afterPosition < 0) throw new HostClientError("invalid_command");
    const controller = new AbortController();
    const timeout = globalThis.setTimeout(() => controller.abort(), this.limits.followDeadlineMs);
    let view: HostView = { cursor: afterPosition, text: "", unknownEvents: [], fingerprints: {} };
    try {
      const response = await this.fetcher(
        `${this.baseUrl}/v1/sessions/${encodeURIComponent(sessionId)}/events?after_position=${afterPosition}`,
        { method: "GET", redirect: "error", signal: controller.signal },
      );
      if (response.redirected) throw new HostClientError("transport_failure");
      if (!response.ok) await throwHostFailure(response);
      if (!response.headers.get("content-type")?.toLowerCase().startsWith("text/event-stream") || !response.body) {
        throw new HostClientError("transport_failure");
      }
      const reader = response.body.getReader(); const decoder = new TextDecoder("utf-8", { fatal: true });
      let pending = ""; let count = 0;
      while (true) {
        const chunk = await reader.read();
        if (chunk.done) throw new HostClientError("transport_failure");
        pending += decoder.decode(chunk.value, { stream: true });
        let boundary: number;
        while ((boundary = pending.indexOf("\n\n")) >= 0) {
          const block = pending.slice(0, boundary).replaceAll("\r", ""); pending = pending.slice(boundary + 2);
          const data = block.split("\n").filter((line) => line.startsWith("data: "))
            .map((line) => line.slice(6)).join("\n");
          if (!data) continue;
          if (new TextEncoder().encode(data).length > this.limits.maxEventBytes) throw new HostClientError("event_limit_exceeded");
          if (++count > this.limits.maxEvents) throw new HostClientError("event_limit_exceeded");
          let event: HostEvent;
          try { event = JSON.parse(data) as HostEvent; } catch { throw new HostClientError("invalid_event"); }
          view = reduceHostEvents(sessionId, [event], view, this.limits.maxEvents);
          if (view.terminal !== undefined) { await reader.cancel(); return view; }
        }
      }
    } catch (error) {
      if (error instanceof HostClientError) throw error;
      if (controller.signal.aborted) throw new HostClientError("follow_deadline");
      throw new HostClientError("transport_failure");
    } finally { globalThis.clearTimeout(timeout); }
  }
  private async post(path: string, commandId: string, body: Record<string, string | number | boolean>): Promise<unknown> {
    if (!validCommandId(commandId) || Object.values(body).some((value) => !value)) throw new HostClientError("invalid_command");
    const encoded = JSON.stringify(body);
    if (new TextEncoder().encode(encoded).length > this.limits.maxCommandBytes) throw new HostClientError("invalid_command");
    let response: Response;
    try {
      response = await this.fetcher(`${this.baseUrl}${path}`, {
        method: "POST", redirect: "error",
        headers: { "Content-Type": "application/json", "Idempotency-Key": commandId }, body: encoded,
      });
    } catch { throw new HostClientError("transport_failure"); }
    if (response.redirected) throw new HostClientError("transport_failure");
    if (!response.ok) await throwHostFailure(response);
    const raw = await response.text();
    if (new TextEncoder().encode(raw).length > this.limits.maxEventBytes) throw new HostClientError("invalid_event");
    try { return JSON.parse(raw) as unknown; } catch { throw new HostClientError("invalid_event"); }
  }
  private async get(path: string): Promise<HostReadDocument> {
    let response: Response;
    try { response = await this.fetcher(`${this.baseUrl}${path}`, { method: "GET", redirect: "error" }); }
    catch { throw new HostClientError("transport_failure"); }
    if (response.redirected) throw new HostClientError("transport_failure");
    if (!response.ok) await throwHostFailure(response);
    const raw = await response.text();
    if (new TextEncoder().encode(raw).length > this.limits.maxEventBytes * this.limits.maxEvents) {
      throw new HostClientError("event_limit_exceeded");
    }
    let value: unknown; try { value = JSON.parse(raw); } catch { throw new HostClientError("invalid_event"); }
    if (!isRecord(value)) throw new HostClientError("invalid_event"); return value;
  }
  private async requestEventStream(sessionId: string, afterPosition: number, signal?: AbortSignal): Promise<Response> {
    let response: Response;
    try { response = await this.fetcher(
      `${this.baseUrl}/v1/sessions/${encodeURIComponent(sessionId)}/events?after_position=${afterPosition}`,
      { method: "GET", redirect: "error", signal },
    ); } catch { throw new HostClientError("transport_failure"); }
    if (response.redirected) throw new HostClientError("transport_failure");
    if (!response.ok) await throwHostFailure(response);
    if (!response.headers.get("content-type")?.toLowerCase().startsWith("text/event-stream") || !response.body) {
      throw new HostClientError("transport_failure");
    }
    return response;
  }
}

function validateBaseUrl(value: string): string {
  let url: URL; try { url = new URL(value); } catch { throw new HostClientError("invalid_configuration"); }
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) ||
      url.username || url.password || url.search || url.hash || url.pathname !== "/") {
    throw new HostClientError("invalid_configuration");
  }
  return url.origin;
}
function validCommandId(value: string): boolean {
  return value.length > 0 && value.length <= 128 && [...value].every((character) => {
    const code = character.charCodeAt(0); return code >= 0x21 && code <= 0x7e;
  });
}
function validateSessionResponse(value: unknown): CreateSessionResponse {
  if (!isRecord(value) || !text(value.session_id) || !text(value.agent_instance_id) || !position(value.committed_position)) {
    throw new HostClientError("invalid_event");
  }
  return value as unknown as CreateSessionResponse;
}
function validateTurnResponse(value: unknown): TurnCommandResponse {
  if (!isRecord(value) || !text(value.session_id) || !text(value.turn_id) || !text(value.execution_id) ||
      !position(value.committed_position)) throw new HostClientError("invalid_event");
  return value as unknown as TurnCommandResponse;
}
function validateOwnedTurnResponse(value: unknown, sessionId: string, turnId: string): TurnCommandResponse {
  const response = validateTurnResponse(value);
  if (response.session_id !== sessionId || response.turn_id !== turnId) throw new HostClientError("invalid_event");
  return response;
}
function validateEvent(value: unknown, sessionId: string): asserts value is HostEvent {
  if (!isRecord(value) || value.api_version !== "v1" || value.session_id !== sessionId ||
      !position(value.position) || !text(value.event) || typeof value.turn_id !== "string" ||
      typeof value.execution_id !== "string" || typeof value.text !== "string") {
    throw new HostClientError("invalid_event");
  }
}
async function throwHostFailure(response: Response): Promise<never> {
  let code: unknown;
  try { code = (JSON.parse(await response.text()) as Record<string, unknown>).code; } catch { /* safe fallback */ }
  if (typeof code === "string" && KNOWN_CLIENT_ERRORS.has(code as HostClientFailure)) {
    throw new HostClientError(code as HostClientFailure, response.status);
  }
  if (typeof code === "string" && KNOWN_HOST_ERRORS.has(code)) throw new HostClientError("host_failure", response.status);
  throw new HostClientError("unknown_host_error", response.status);
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function text(value: unknown): value is string { return typeof value === "string" && value.length > 0; }
function position(value: unknown): value is number { return Number.isSafeInteger(value) && Number(value) > 0; }
