# Core Agent development plan

## Responsibility

Turn the active Core Agent design into vertical, testable Rust slices without
pulling Runtime persistence, provider HTTP, or client state into the kernel.

## Dependency direction

```text
runtime/replica -> garive-core -> garive-llm
                    |          -> garive-tools
                    `----------> garive-ledger ports/vocabulary
```

Arrows point from consumer to dependency. Runtime composes all ports; no engine
crate depends on Runtime or an App.

## Slice order

| Slice | Output | Depends on | Exit evidence |
|---|---|---|---|
| C0 Turn control | typed turn identity, phase, limits, legal transitions | none | transition unit tests and exhaustive terminal behavior |
| C1 Model contract | provider-neutral request, streamed items, usage, nine normalized outcomes | C0 vocabulary only where necessary | fake adapter contract tests; no HTTP/provider names in Core |
| C2 Context surface | ledger read port, purpose projection, budget, deterministic derive result | C0 + ledger vocabulary | fixture/property tests for ordering, masking, and budget |
| C3 Model-only turn | bounded derive → assemble → invoke loop | C0-C2 | fake-model integration: answer, overflow, cancel, unavailable |
| C4 Tool preparation | registry lookup, schema validation, immutable prepared call + digest | tools vocabulary | invalid intent never reaches authorization; digest stability tests |
| C5 Governed effects | authorization/execution ports and model-visible outcomes | C3-C4 | approve, deny, rewrite, ask-user, multi-iteration tests |
| C6 Durable host | Runtime facts, resume derivation, effect lifecycle and receipts | C0-C5 | process-restart integration tests with real storage adapter |
| C7 Compression | summary request and masking policy under context pressure | C2-C6 evidence | quality/cost baseline before numeric release gates |

## Work rules

- Each slice gets one focused spec under `spec/design/` before behavior lands.
- Internal domain types remain Rust types. Proto is introduced only for a real
  public, persistence, or cross-process boundary.
- Core uses injected ports and deterministic clocks/limits; concrete storage,
  credentials, sandboxes, and provider clients remain outside it.
- A suspended turn retains its `turn_id`. Runtime rebuilds Core input from
  durable facts; Core state is not a second checkpoint format.
- A missing external-effect result is never interpreted as permission to
  execute again.

## Immediate backlog

1. Implement C0 from [`core-turn-control.md`](core-turn-control.md).
2. Promote the normalized model outcomes into a C1 spec, removing policy
   actions that belong to Runtime.
3. Define the smallest ledger read port needed by C2 rather than implementing
   the full ledger research document.
4. Use fakes to deliver C3 before SQLite, provider SDKs, or Apps are connected.

## Out of scope for the first milestone

- provider-specific retry tables and billing reconciliation;
- SQLite schema/index tuning;
- adaptive compression coefficients;
- Kotlin parity, Gateway, Desktop, or Mobile integration;
- benchmark thresholds without a runnable Agent baseline.

## Acceptance

The first milestone is complete when a deterministic Rust test runs one
model-only turn through a fake context source and fake model, returns a typed
terminal outcome, and cannot exceed its iteration limit.
