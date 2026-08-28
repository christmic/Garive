# runtime/AGENTS.md

> Runtime owns session lifecycle, durable storage, execution, recovery, and
> composition around the Agent kernel.

This file applies under `runtime/` and refines the root rules.

@AGENTS.md

## Ownership

- `replica/` is the intended first Rust host and composition root. It may
  depend on `engine/`; `engine/` must not depend on it.
- Runtime owns model/tool invocation lifecycle, external-effect receipts,
  cancellation, resume, channels, and persistence adapters.
- Runtime does not decide Agent intent or move domain policy into transport
  handlers.
- `gateway/` is the planned Go service edge for auth, admission, routing, load
  balancing, and edge observability. It is not active until its first slice
  lands and must not absorb Agent policy or Runtime recovery state.

## Boundaries

Add a wire schema in `spec/` only when Runtime has a real out-of-process
consumer. In-process ports remain Rust interfaces. Transport-generated types
are mapped at the boundary and do not become the domain model.

Every external effect uses a durable lifecycle and stable invocation identity.
After a crash, classify the invocation as safe to replay, receipt-recoverable,
or uncertain; never infer success from a missing result and never blindly
repeat an uncertain effect.

## Verification

For each implemented Runtime slice:

- run workspace formatting, clippy, tests, and docs;
- test durable state transitions and crash boundaries with the real storage
  adapter where recovery behavior is claimed;
- add wire, fuzz, or end-to-end checks only for the parsers and processes that
  actually exist;
- keep numeric performance gates provisional until a reproducible baseline is
  recorded.
