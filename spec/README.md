# spec/

`spec/` contains small normative contracts for boundaries that are being
implemented. It is not a catalog of the future system.

## Layout

- `STATUS.md`: authoritative design/Spec/API/code/test delivery board.
- `proto/`: protobuf schemas for real wire or independently persisted
  contracts.
- `fixtures/`: shared inputs for executable conformance checks.
- `design/`: concise normative decisions that name their producer, consumer,
  compatibility promise, and enforcement test.

Empty directories are reservations, not proof that a contract exists.

## Admission rule

A contract belongs here only when all of the following are known:

1. the owning component and consumers;
2. the boundary being crossed;
3. compatibility and failure behavior;
4. the implementation slice that will enforce it;
5. an executable verification path.

Otherwise keep the reasoning in `docs/` and edit it in place.

## Type ownership

Internal Rust domain types stay in their owning module. Add protobuf only for a
wire/persistence boundary; generated bindings are transport types and may be
mapped to domain types. Do not require every language to mirror every internal
concept.

## Conformance

Choose the minimum level the boundary requires: wire compatibility, canonical
encoding, semantic equivalence, or explicit capability reporting. `just
conformance` is the executable semantic gate for admitted C0-C3 behavior: Rust
and Kotlin both consume every case in the shared Agent fixtures.

See [`AGENTS.md`](AGENTS.md) for schema and verification rules.
See [`STATUS.md`](STATUS.md) for current delivery evidence and next slices.
See [`design/agent-core-spec-set.md`](design/agent-core-spec-set.md) for the
accepted D0/C4/C5/C6 implementation contract set.
The complete active increment is indexed by
[`design/agent-product-increment-spec-set.md`](design/agent-product-increment-spec-set.md);
its repository dependency order is also maintained in
[`design/core-agent-plan.md`](design/core-agent-plan.md#next-accepted-increments).
The macOS product composition and work-quality bar are defined by
[`design/desktop-work-product.md`](design/desktop-work-product.md).
The native physical-device remote-work contract is
[`design/mobile-remote-work-client.md`](design/mobile-remote-work-client.md),
with its authenticated edge contract in
[`design/mobile-gateway-v1.md`](design/mobile-gateway-v1.md).
The complete terminal product is indexed by
[`design/tui-product-spec-set.md`](design/tui-product-spec-set.md), including
its source audit, architecture, interaction, communication/persistence, and
competitive verification contracts.
