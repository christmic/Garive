# engine/proto/fuzz/

> **Fuzz targets for the Rust wire-types decoder.** Every
> message in `spec/proto/` must have a fuzz target that
> throws random bytes at `Message::decode(&[u8])` and asserts
> that the decoder either succeeds with a sensible struct or
> fails with a typed error — never panics, never overflows,
> never leaks memory.

## Why

Wire types cross every process boundary in the system. A
panic in `AgentIdentity::decode` takes down the replica, the
gateway, the desktop backend, the mobile bridge — anywhere
Rust sits on the wire. Fuzzing the decoder is the cheapest
way to flush out that class of bug.

## Mechanism

```
cd engine/proto
cargo +nightly fuzz run fuzz_agent_identity_decode -- -max_total_time=60
```

Each `fuzz_*.rs` target is one decoder. Targets run on
libFuzzer via `cargo-fuzz`. Run for minutes (not seconds)
per target for a meaningful coverage signal.

## Required Targets (one per `.proto` message)

| Target | Asserts |
|--------|---------|
| `fuzz_agent_identity_decode` | random bytes → `AgentIdentity::decode` → never panics |
| `fuzz_ping_request_decode` | random bytes → `PingRequest::decode` → never panics |
| `fuzz_ping_response_decode` | same |
| `fuzz_*_decode` | one per message in `spec/proto/*.proto` |

Add a target the moment a new message lands in
`spec/proto/*.proto`. The contract: **every wire message has
a fuzz target, full stop.**

## Cadence

- **Per-PR:** the unit suite runs the round-trip contract
  test. Fuzz doesn't run (too slow).
- **Nightly:** fuzz targets run for 5 minutes each on the
  self-hosted runner. Any finding → block-release until a
  regression test in the unit suite reproduces it.
- **Release:** fuzz targets run for 30 minutes each.

## Status

Placeholder. Targets land as messages land in
`spec/proto/*.proto`.