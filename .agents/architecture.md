# Architecture

## High-level System

Garive's runtime is split across four cooperating tiers:

```
┌─────────────────────────────────────────────────────────────┐
│                  Agent Apps (clients)                       │
│      Swift (macOS) · TypeScript (web) · other surfaces      │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                  Agent Gateway (Go)                         │
│     Auth · rate limit · load balance · observability        │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            Core Agent (Rust · Kotlin mirror)                │
│           loop · tools · safety · memory · knowledge        │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                  Multi-channel Surfaces                     │
│              CLI · IDE · chat · IM · voice (TBD)            │
└─────────────────────────────────────────────────────────────┘
```

## Core Agent — Cross-language Isomorphic Design

The Core Agent ships in **two source-of-truth mirrors**:

- **Rust** — primary, ship-to-production implementation.
  Performance-sensitive paths live here.
- **Kotlin** — synchronized mirror. Same wire protocol, same
  semantics, shareable trait / test surface. Used by JVM-side
  services and Android-adjacent surfaces.

Both languages track each other; a protocol-level change updates
both in the same change set, verified by a shared conformance
suite.

## Research Fronts

Beyond the core loop, Garive explores:

- **Self-drive** — the agent initiates work without explicit
  prompts when it detects signal worth acting on.
- **Value discovery** — the agent surfaces opportunities the user
  hasn't yet articulated.
- **Feedback loops** — every action returns a signal that the
  agent integrates into its next decision.

## Sub-docs

Per-feature / per-subsystem designs accumulate under
`docs/architecture/` once specific slices are scoped. Update this
index as sub-docs land.