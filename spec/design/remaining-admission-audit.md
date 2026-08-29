# Remaining design admission audit

## Status

Accepted admission decision. This document does not admit behavior.

## Decision rule

An implemented harness proves that evidence can be collected; it is not the
evidence itself. A draft becomes an implementation Spec only after the named
workload, measurement and ownership decision exist in versioned repository
evidence. Proposed numeric thresholds are not copied into accepted contracts.

## Current decisions

| Slice | Decision | Missing evidence before a focused Spec |
|---|---|---|
| C7 measured compression | gated | C7-A supplies the corpus/runner, P2-VX-ATC the exact vendor exchange, and verified C7-B the secret-safe normal-provider composition. C7-C is admitted to add the missing bounded HTTPS and credential-reference composition. Remaining after C7-C: a credentialed publication run and measured trigger/retention trade-off. |
| Creativity | gated | Neutral task taxonomy and bounded alternative-generation hypothesis; reproducible baseline runs using E0/B0 infrastructure; deterministic outcome rubric separating diversity from correctness; authority and budget ownership. Harness unit tests are not a baseline. |
| P2-VX hosted capability | planned per capability | One concrete capability request; provider-neutral semantics and extension values; exact unsupported/failure behavior; protocol fixtures; proof that ordinary Tool/Knowledge semantics cannot represent it. No generic extension allowlist is admitted. |
| P2-VX-ATC token count | delivered | Exact non-generating count semantics, official SDK evidence, explicit profiles, strict decoding and shared Rust/Kotlin fixtures are verified. A credentialed publication run remains separate C7-A evidence. |
| G0 Go Gateway | gated | A live H1 edge workflow requiring independent scaling; load/failure measurements; authentication/routing ownership not already held by Host; deployment and recovery boundary. Language preference alone is insufficient. |
| A-MOBILE Android evidence | active external gate | Android SDK 36, real APK assembly and device/emulator evidence. This does not require a new architecture Spec. |

## Consequences

- `engine/creativity` remains a documented empty crate and carries no delivery
  claim; implementation must not begin from its name alone.
- `docs/architecture/core/compression.md` and numeric targets in
  `derive-testing.md` remain research hypotheses.
- B0 and E0 stay completed and reusable as evidence infrastructure without
  being cited as representative Creativity or compression results.
- No empty Go or hosted-vendor implementation shells are created while their
  admission dependency is unmet.

## Re-audit trigger

Re-open only when a change set links the concrete evidence named in the table.
That change set must first add a focused design/Spec, public API and shared or
native verification plan, then update `spec/STATUS.md`; behavior follows in a
later small batch.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
