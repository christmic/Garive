# TUI product Spec set

> This index freezes the evidence-backed contract and delivery order for the
> complete Garive terminal client. It covers architecture through terminal
> details, UI through state model, Host communication through local crash
> recovery, and competitive product-quality evidence.

## Audience

Maintainers implementing or reviewing `tui/`, `clients/host-rs/`, H1/H2/H3
dependencies, and release evidence.

## Why

Garive now has a resident, multi-Session, multi-turn terminal implementation.
This Spec set remains its conformance contract while the remaining competitive,
platform, crash-recovery, and evidence gates are closed. A single large
document would mix ownership, interaction, persistence, and evidence; this set
keeps each normative concern bounded while this index owns their DAG.

## Evidence basis

[`../../docs/tui-source-audit.md`](../../docs/tui-source-audit.md) records the
exact inspected Garive, Grok Build, Codex, and Claude Code artifacts, source
paths, licenses, confirmed patterns, and rejected transfers. The two official
Apache sources inform structure and testing. Claude Code material is
corroboration only and contributes no copied code or distinctive text.

Garive Host and Ledger Specs remain authoritative where a reference product
uses a different ownership model.

## Normative documents

| Spec | Owns | Does not own |
|---|---|---|
| [`tui-application-architecture.md`](tui-application-architecture.md) | modules, state/effect model, concurrency, terminal lifecycle, failures | visual detail or file schema |
| [`tui-interaction-and-rendering.md`](tui-interaction-and-rendering.md) | IA, responsive layout, editor, keymap, commands, Markdown, scroll, accessibility | HTTP or disk algorithms |
| [`tui-visual-system.md`](tui-visual-system.md) | visual tokens, reusable components, state variants, responsive degradation, visual conformance | product commands or Host semantics |
| [`tui-communication-and-persistence.md`](tui-communication-and-persistence.md) | Host port, snapshot/follow, backpressure, retry, files, crash matrix, privacy | widget composition |
| [`tui-quality-and-verification.md`](tui-quality-and-verification.md) | competitive matrix, fixtures, snapshots, PTY, Runtime E2E, performance, compatibility, completion | product semantics already frozen elsewhere |

Existing dependency contracts remain single sources of truth:

| Dependency | Contract |
|---|---|
| durable commands and SSE | [`host-api-v1.md`](host-api-v1.md) |
| Rust client reduction | [`live-host-clients.md`](live-host-clients.md) |
| Agent/Session/timeline/suspension reads | [`host-read-model-v1.md`](host-read-model-v1.md) |
| redacted activity | [`host-agent-activity-v1.md`](host-agent-activity-v1.md) |
| shared product semantics | [`client-product-experience.md`](client-product-experience.md) |

If this set conflicts with a Host wire field or durable invariant, the Host
Spec wins and this set must be corrected before code.

## Product definition of complete

The delivered TUI must:

- launch through the shipping binary and restore terminal state on every exit;
- remain responsive while bounded Host reads, mutations, and SSE follow run;
- discover Agents, navigate/create/reopen Sessions, and submit multiple Turns;
- render durable conversation, typed public activity, suspension, and terminal
  state without exposing internal facts;
- provide a Unicode-safe multiline editor, prompt history, command palette,
  contextual help, responsive layout, themes, mouse option, and accessible
  linear screen-reader mode;
- reconnect from durable watermarks and preserve exact mutation identity across
  timeout, process crash, and restart;
- persist only bounded local preferences, drafts, prompt history, diagnostics,
  and pending retry envelopes with owner-only crash-safe files;
- pass real Runtime HTTP/SSE and PTY workflows, not only fake transports;
- publish measured performance baseline evidence and close every applicable row
  in the competitive matrix.

The goal is not feature-name equality with another provider's Agent. It is
equivalent product discipline for capabilities Garive actually admits: fast
interaction, durable truth, transparent recovery, terminal safety, rich native
presentation, accessibility, and executable evidence.

## Dependency DAG

```text
source audit + TUI Spec set
        |
        +--> H1-F exact version + typed continuation + real Host E2E
        |
        +--> H2/H3 wire -> Runtime projections -> Rust client mappings
        |                                      |
        +--> pure TUI model/editor/view -------+
        |                                      |
        +--> local persistence/retry -----------+
                                               v
                                      live effect runner
                                               |
                                  Runtime E2E + PTY + baseline
                                               |
                                      competitive closeout
```

Pure editor, reducer, layout, renderer, and persistence codecs may progress
while H2/H3 land because they consume ports and shared fixtures. Live Session
navigation, suspension, activity, and final product E2E wait for the real Host
capabilities.

## Delivery packages

| Order | ID | Deliverable | Acceptance |
|---:|---|---|---|
| 1 | T0 | source audit and complete Spec set | link/banned-phrase/source review; status board synchronized |
| 2 | T1 | H1-F client prerequisite | exact `v1`, canonical JSON continuation, Runtime-backed CLI/TUI H1 flow |
| 3 | T2 | H2/H3 wire and Runtime/client capability | Proto tag audit, fixed-prefix SQLite projection, Rust mappings and fixtures |
| 4 | T3 | TUI library architecture and terminal runtime | pure reducer/effects, supervised loop, idempotent restore, launch/PTTY tests |
| 5 | T4 | editor, commands, responsive renderer | Unicode properties, parser matrix, semantic snapshots, safe Markdown/control filtering |
| 6 | T5 | local persistence and exact retry | atomic/locked files, permission/fault/crash matrices, process-kill replay |
| 7 | T6 | live navigation/conversation/activity/suspension | multi-Session/multi-turn real Runtime flows and reconnect/backpressure |
| 8 | T7 | competitive quality closeout | repeated PTY E2E, measured baseline, compatibility evidence, full repository gates |

Every commit stays within the repository small-batch limit and leaves its
implemented slice buildable and testable. Packages may use multiple commits;
they do not become separate branches because they implement one requirement.

## Dependency selection

The 2026-08-30 crates.io sparse-index audit found these newest stable compatible
pins for the terminal foundation:

| Crate | Pin | Evidence/decision |
|---|---:|---|
| `ratatui` | `0.30.2` | newest non-yanked stable index entry; Rust `1.88`, compatible with workspace `1.98` |
| `crossterm` | `0.29.0` | newest non-yanked stable entry and Ratatui `0.30` backend line |
| `tui-textarea` | excluded | newest `0.7.0` requires Ratatui `0.29`; Garive authors a backend-independent editor |

Additional direct dependencies require the same official registry/release
audit and native gates before admission. Manifest and `Cargo.lock` become the
version SSOT in the implementing commit.

## Fixture catalogue

| Root | Owner |
|---|---|
| `spec/fixtures/host/live-host-client-v1.json` | H1 command/reduction compatibility |
| `spec/fixtures/host/host-read-model-v1.json` | H2 definitions/Sessions/timeline/suspension |
| `spec/fixtures/host/host-agent-activity-v1.json` | H3 public activity projection/reducer |
| `spec/fixtures/tui/tui-product-v1.json` | TUI application, editor, persistence, failure behavior |
| `tui/tests/snapshots/` | semantic terminal buffers and screen-reader lines |
| `tui/tests/pty/` | shipping-binary terminal behavior |
| `tui/benches/` | pinned reducer/editor/render/event-loop baselines |

Fixtures are authoritative only for the Spec that names them. A fixture change
and implementation change in one commit must identify the accepted behavior
that changed.

## Compatibility and migration

The existing positional command form
`garive-tui <host> <definition> <message>` is replaced, not retained as a hidden
mode. The new resident CLI uses explicit options and interactive input. Scripts
should use `garive-cli`; the TUI is a terminal application.

Existing Runtime Session/Ledger data require no migration. TUI local schema v1
is new. Unknown or corrupt local versions reset/quarantine presentation state
without changing Host truth. Host v1 changes are additive under H1/H2/H3.

## Status rule

The Specs are accepted and the resident implementation covers T1 through the
principal T6 flow. T0 documentation synchronization and T7 completion evidence
remain active until every named acceptance gate is executable and verified.
`A-TUI` in `spec/STATUS.md` is the sole status row; this index does not create a
second delivery board.

## See also

- [`agent-product-increment-spec-set.md`](agent-product-increment-spec-set.md) — wider H1/H2/H3 product DAG.
- [`../STATUS.md`](../STATUS.md) — sole delivery board.
- [`../../.agents/testing.md`](../../.agents/testing.md) — evidence maturity and repository gates.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
