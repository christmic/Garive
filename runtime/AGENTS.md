# runtime/AGENTS.md

> **Service-runtime tier.** Hosts the Core Agent as a service.
> Two sub-tiers:
>
> - `replica/` — Rust service container running an Agent process.
> - `gateway/` — Go high-throughput gateway (auth, rate limit,
>   load balance, observability, routing).
>
> This file applies to everything under `runtime/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Replica (`runtime/replica/`, Rust)

- Workspace member of the root Cargo workspace (added to
  `members` in the root `Cargo.toml`).
- Embeds `engine/` crates via normal Cargo path dependencies.
- Exposes a single inbound interface (`/v1/<endpoint>`) that the
  gateway calls. The wire schema lives in `spec/proto/`; replica
  types are generated bindings, never hand-written.
- Long-running work runs in a Tokio runtime; CPU-bound work is
  dispatched to a blocking pool.

## Gateway (`runtime/gateway/`, Go)

- **Not** a Cargo workspace member. Standalone `go.mod`.
- Talks to replicas over the wire schema in `spec/proto/` using
  the generated Go bindings (`buf generate` with the Go plugin).
- Owns auth, rate limit, load balance, and observability. Does
  **not** run agent logic; agent logic lives in the replica.
- All public endpoints are versioned under `/v1/`.

## Cross-tier Contract

The replica ↔ gateway interface is **defined by `spec/proto/`**
and nothing else. Any change to a request / response shape starts
in `spec/proto/*.proto` and is reflected by `buf generate` (or
`build.rs`) into both tiers. Hand-written request / response
structs are forbidden.

## Observability

- Replica and gateway both emit structured logs (JSON) to stdout.
- Trace IDs propagate from gateway to replica via request
  metadata; both tiers emit the same trace ID on every log line
  that touches the request.