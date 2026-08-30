import {
  initialAppViewState, reduceApp, type AppEffect, type AppEffectPayload, type AppErrorKind,
  type AppIntent, type AppViewState, type ControllerLimits,
} from "../state/controller";

/** Executes one application effect without owning product state or policy. */
export interface ProductEffectPort {
  run(
    effect: AppEffect,
    snapshot: AppViewState,
    signal: AbortSignal,
  ): AsyncIterable<AppEffectPayload>;
}

/** A content-free failure admitted at the composition boundary. */
export class ProductPortError extends Error {
  public constructor(public readonly kind: AppErrorKind, public readonly code: string) {
    super("product_effect_failed");
  }
}

/** Owns reducer/effect orchestration while all durable truth remains in Host. */
export class ProductRuntime {
  readonly #port: ProductEffectPort;
  readonly #limits?: ControllerLimits;
  readonly #listeners = new Set<(state: AppViewState) => void>();
  readonly #inFlight = new Map<string, AbortController>();
  #state: AppViewState;
  #disposed = false;

  public constructor(
    port: ProductEffectPort,
    configuration: "configured" | "not_configured" = "configured",
    limits?: ControllerLimits,
  ) {
    this.#port = port; this.#limits = limits;
    this.#state = initialAppViewState(configuration);
  }

  public get state(): AppViewState { return this.#state; }

  public subscribe(listener: (state: AppViewState) => void): () => void {
    this.#listeners.add(listener); listener(this.#state);
    return () => this.#listeners.delete(listener);
  }

  public dispatch(intent: AppIntent): void {
    if (this.#disposed) return;
    const reduction = reduceApp(this.#state, intent, this.#limits);
    this.#state = reduction.state;
    this.#reconcile(); this.#publish();
    for (const effect of reduction.effects) this.#start(effect);
  }

  public dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    for (const controller of this.#inFlight.values()) controller.abort();
    this.#inFlight.clear(); this.#listeners.clear();
  }

  #start(effect: AppEffect): void {
    if (this.#disposed || this.#inFlight.has(effect.effectId)) return;
    const controller = new AbortController(); this.#inFlight.set(effect.effectId, controller);
    const snapshot = this.#state;
    void this.#consume(effect, snapshot, controller).finally(() => {
      if (this.#inFlight.get(effect.effectId) === controller) this.#inFlight.delete(effect.effectId);
    });
  }

  async #consume(effect: AppEffect, snapshot: AppViewState, controller: AbortController): Promise<void> {
    let delivered = false;
    try {
      for await (const payload of this.#port.run(effect, snapshot, controller.signal)) {
        if (controller.signal.aborted || this.#disposed) return;
        delivered = true; this.#deliver(effect, payload);
        if (!this.#state.outstanding.some((item) => item.effectId === effect.effectId)) return;
      }
      if (controller.signal.aborted || this.#disposed ||
          !this.#state.outstanding.some((item) => item.effectId === effect.effectId)) return;
      this.#deliver(effect, effect.kind === "follow_events"
        ? { type: "event_stream_ended" }
        : { type: "failed", error: { kind: "protocol", code: delivered
          ? "effect_result_incomplete" : "effect_result_missing" } });
    } catch (cause) {
      if (controller.signal.aborted || this.#disposed) return;
      const error = safeError(cause);
      this.#deliver(effect, { type: "failed", error });
    }
  }

  #deliver(effect: AppEffect, result: AppEffectPayload): void {
    this.dispatch({ type: "effect_result", effectId: effect.effectId,
      generation: effect.generation, sessionId: effect.sessionId,
      requestDigest: effect.requestDigest, result });
  }

  #reconcile(): void {
    const retained = new Set(this.#state.outstanding.map((effect) => effect.effectId));
    for (const [id, controller] of this.#inFlight) {
      if (!retained.has(id)) { controller.abort(); this.#inFlight.delete(id); }
    }
  }

  #publish(): void { for (const listener of this.#listeners) listener(this.#state); }
}

function safeError(cause: unknown): { readonly kind: AppErrorKind; readonly code: string } {
  if (cause instanceof ProductPortError && SAFE_CODE.test(cause.code)) {
    return { kind: cause.kind, code: cause.code };
  }
  return { kind: "protocol", code: "product_port_failure" };
}

const SAFE_CODE = /^[a-z][a-z0-9_]{0,63}$/;
