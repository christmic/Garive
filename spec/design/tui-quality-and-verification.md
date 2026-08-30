# TUI quality and verification

> This Spec defines the competitive feature matrix, executable test layers,
> PTY scenarios, performance program, terminal compatibility evidence, privacy
> scans, and completion criteria for the Garive TUI product.

## Audience

TUI, Host client, Runtime, release, and test engineers deciding whether the
terminal client is product-complete rather than a runnable demonstration.

## Why

Unit tests can prove a reducer while the shipping binary corrupts the terminal,
blocks input during network traffic, loses a pending command, or cannot reopen
a Session. A successful launch is necessary but insufficient. Completion needs
observable evidence across the real entry path and the boundaries that mature
terminal agents exercise.

## Competitive comparison basis

The comparison is scoped to terminal-product capabilities, not provider or
Agent feature count. Evidence revisions and legal constraints are recorded in
[`../../docs/tui-source-audit.md`](../../docs/tui-source-audit.md).

| Capability | Grok Build source | Codex source | Garive required evidence |
|---|---|---|---|
| Typed action/effect/result pipeline | `xai-grok-pager/src/app/actions.rs` | `tui/src/app_event.rs`, `app.rs` | pure reducer matrices and supervised effect integration |
| Nonblocking multiplexed event loop | `app/event_loop.rs` | `tui/src/tui.rs`, `app.rs` | input/cancel under saturated Host event channel |
| Panic-safe terminal restore | `xai-grok-pager/src/app/mod.rs` | `tui/src/tui.rs` | setup-step faults, panic, signal, double restore, PTY shell state |
| Fullscreen responsive presentation | pager app/render crates | Ratatui TUI view modules | width/height/theme snapshots and live resize PTY |
| Mature multiline input | Ratatui textarea fork and input modules | composer/paste state machines | Unicode editor properties, paste, selection, undo/redo, history |
| Command discovery | command palette and slash registry | slash command popup | one registry drives parser, palette, help, availability |
| Session picker/resume | session actions/effects/storage boundary | app-server list/read/resume | H2 list/select/page/reopen Runtime E2E |
| Progress/activity | ACP activity routing | app-server events/history cells | H3 redacted snapshot+follow reduction |
| Approval/external input | ACP interaction views | approval/request-user-input panes | H2 schema-driven suspension + exact H1 continuation |
| Disconnect/reconnect | ACP reconnect tests | app-server session recovery | bounded cursor reconnect and no inferred terminal |
| Persistent convenience state | shell persistence separate from pager | core/app-server truth plus TUI state | bounded preferences/history/drafts/pending command |
| Visual regression | snapshots and PTY harness | `insta` snapshots and frames | semantic buffer snapshots and PTY golden assertions |
| Accessible fallback | minimal/fullscreen behavior | terminal capability-aware UI | monochrome, reduced motion, keyboard-only, screen-reader linear mode |

Garive passes a row only when the required evidence is executable. Presence of
a module, screenshot, fixture transport, or test-only fake does not pass it.

## Test layers

| Layer | Boundary | Required claims |
|---|---|---|
| Unit | editor, parser, text sanitizer, layout helpers, persistence codec | normal, boundary, hostile input, panic freedom |
| Property | reducer, editor, cursor/activity reduction, layout, canonical digest | declared invariants over bounded generated values |
| Snapshot | rendered Ratatui buffers and screen-reader lines | semantic visual output across state/size/theme/capability matrix |
| Contract | `garive-host-client` HTTP/SSE/H2/H3 | exact wire validation, errors, bounds, reconnect inputs |
| Integration | application reducer + effect runner + memory ports | correlation, concurrency, backpressure, retry, shutdown |
| Persistence | real filesystem with fault injection | atomicity, permissions, locks, quarantine, crash matrix |
| Runtime E2E | shipping TUI + real H1/H2/H3 + file SQLite | create, multi-turn, cancel, suspend/continue, restart/reopen |
| PTY E2E | shipping binary under a real pseudo-terminal | keyboard/paste/resize/render/exit and terminal restoration |
| Performance | reducer/editor/render/event loop under pinned corpus | baseline distribution and accepted regression margin |
| Compatibility | supported OS/terminal matrix | build plus representative PTY/native run |

Test-only transports may disprove local behavior but cannot satisfy Runtime E2E.
Runtime E2E uses a temporary file-backed SQLite database and the production
loopback router/composition. Model/tool ports may be deterministic injected
ports when the test targets TUI semantics; the Host, dispatcher, Ledger,
projection, HTTP, SSE, and client are real.

## Shared fixture set

`spec/fixtures/tui/tui-product-v1.json` is strict schema version 1 with:

```text
bootstrap_cases
navigation_cases
conversation_cases
command_cases
follow_cases
suspension_cases
activity_cases
editor_cases
persistence_cases
failure_cases
```

Each case has a unique name, complete initial model or file state, ordered
actions/results, and complete expected model/effects or stable failure. Readers
reject unknown fields, duplicate case names, invalid IDs, unbounded values, and
partial expected output.

The fixture uses public values only. It contains redaction canaries for secrets,
terminal escapes, bidi overrides, raw tool/provider values, SQL/file errors,
and private paths. Those canaries must not appear in rendered buffers, logs,
errors, titles, persisted preferences, or diagnostics.

## Reducer and property matrix

At minimum, executable tests cover:

- every application state and every legal/illegal transition;
- exact effect/generation/Session/digest correlation and stale result refusal;
- navigation while another Session runs or requires action;
- snapshot install followed by duplicate, gap, unknown, activity, suspension,
  and terminal events;
- every disconnect/reconnect attempt and explicit retry series;
- pending-command save, exact retry, known rejection, successful replay,
  abandonment, and cross-process conflict;
- bounded maps, drafts, history, activities, timeline cells, undo, and redraw;
- arbitrary terminal sizes without underflow, overflow, or panic;
- arbitrary valid UTF-8 editor operations preserving grapheme boundaries;
- hostile control/ANSI/bidi text producing no terminal control output;
- every slash-command grammar and availability state.

Property failures persist their minimal seed/case beside the owning test when
they reveal a new stable regression class.

## Snapshot matrix

| Axis | Required values |
|---|---|
| Width | `20`, `40`, `60`, `80`, `100`, `120`, `160`, `200` |
| Height | `8`, `12`, `24`, `40`, `60` |
| Theme | dark, light, mono |
| Capability | basic ANSI, 256 color, truecolor, no Unicode border, no OSC 8 |
| State | loading, empty, idle, submitting, running, cancelling, disconnected, suspended, unknown command, completed, stopped, failed |
| Overlay | command, help, Session, history, suspension, error, quit |
| Content | prose, lists, table, quote, code, CJK, emoji, combining, long token, hostile controls |

The complete Cartesian product is unnecessary. Pairwise cases cover every
axis value, while boundary sizes and lifecycle/overlay states receive focused
snapshots. Snapshot approval includes reading every changed `.snap` file.

Snapshots assert semantic cells after Ratatui rendering. Separate sanitizer
tests assert emitted backend bytes contain no untrusted escape sequence.

## PTY scenarios

Each scenario launches `CARGO_BIN_EXE_garive-tui`, not a test-only entry point:

1. boot against real Runtime, observe first frame, create Session, type and
   submit one Turn, observe committed completion;
2. submit a multiline Unicode prompt with bracketed paste and edit it before
   sending;
3. keep typing and open help while Host events arrive at the bounded-channel
   stress rate;
4. resize through tiny, compact, standard, and wide layouts while preserving
   the visible anchor and draft;
5. disconnect the loopback stream, observe unknown state, restart Host, and
   reconnect from the saved cursor;
6. request cancellation and prove UI remains running until a committed
   stopped/completed/failed terminal arrives;
7. render and answer a typed suspension, then continue the same Turn;
8. terminate at each pending-command crash hook, restart, exact-retry, reload
   H2, and submit another Turn;
9. exit normally, by signal, and by injected panic after terminal acquisition;
   assert canonical mode, echo, cursor, paste, focus, and alternate screen are
   restored;
10. run the equivalent core flow in `--screen-reader` mode and assert ordered
    semantic lines without cursor-addressing output.

Scenarios 1, 4, 9, and 10 run twice in the same verification job. Both runs
must pass; a single selected success is not evidence.

## Runtime E2E assertions

The principal product E2E performs:

```text
start file-backed Runtime Host
launch shipping TUI in PTY
load installed definitions
create Session
start Turn 1 -> committed completion
exit and restart TUI
reopen the same Session through H2
verify Turn 1 public timeline
start Turn 2 -> activity -> suspension
continue exact suspension -> committed completion
disconnect/reconnect after a legal position gap
request cancel on Turn 3 -> committed terminal
exit and verify terminal state and SQLite restart replay
```

Assertions bind exact Session/Turn identities, command replay, positions,
public text, activity transitions, suspension schema/digest, and terminal.
No scripted TCP response may stand in for this scenario.

## Performance program

Numeric values begin as Proposed until the first pinned repeated run is stored.
The baseline record names CPU/OS/terminal backend, Rust/toolchain, build profile,
Garive commit, fixture digest, sample count, warmup, distribution, and peak RSS.

| Metric | Corpus | Proposed target before baseline review |
|---|---|---|
| first interactive frame | empty local Runtime, release build | p95 `<150 ms` after process start |
| key-to-model update | 4 KiB Unicode draft | p99 `<2 ms` |
| key-to-render completion | `120x40`, 200 loaded cells | p99 `<16 ms` |
| resize reflow | `200x60`, 1,000 loaded cells | p95 `<33 ms` |
| Host event reduction | 10,000 bounded H3 events | p95 throughput `>100,000 events/s` |
| snapshot render | full state matrix | no allocation or time growth with unloaded timeline pages |
| idle CPU | online idle frame for 60 s | `<0.5%` one logical core average |
| resident memory | 10 Sessions, 5,000 bounded cells | peak `<100 MiB` release build |

After three repeated runs, the Spec is updated with observed distribution and
an accepted regression margin. Completion requires those values to be Baseline
or Gate, not Proposed. Correctness gates remain blocking regardless of speed.

The macOS arm64 release first-frame metric is now a Gate at p95 `<150 ms`.
Pinned outer-process evidence at Garive `54ae160b697147a00e7e1fc128cb3accdc19a18c`
runs the shipping binary against production `LiveHost` plus file SQLite in a
real PTY. Three independent 20-sample runs observed p95 `28.883 ms`,
`26.503 ms`, and `27.068 ms`; the unsmoothed distributions, including the
`356.375 ms` first-run maximum, are stored in
`docs/evidence/tui-release-first-frame-2026-08-30.json`. This acceptance is
specific to the named reference environment. Idle CPU and the exact resident
memory workload remain Proposed and keep the complete performance program open.

Stress tests also prove cancellation latency under a saturated Host channel and
memory stability during 30 minutes of bounded event/reconnect churn. Duration
is a scheduled/release gate after the shorter deterministic harness passes.

## Compatibility matrix

| Platform | Build gate | Native/PTY gate |
|---|---|---|
| macOS arm64 | workspace build/test | Apple Terminal plus one of iTerm2/WezTerm/Kitty; tmux |
| Linux x86_64 | workspace build/test | xterm-compatible PTY; tmux; `TERM=dumb` refusal/linear fallback |
| Windows x86_64 | MSVC build/test | Windows Terminal ConPTY; signal-equivalent restore |

For unavailable local platforms, checked CI evidence may close the build gate;
native interaction remains explicitly unverified until its named run exists.
SSH/mosh, screen, non-UTF-8 locale, and legacy Windows Console stay outside the
supported claim unless separately verified.

## Static and privacy gates

Executable scans fail on:

- imports from Engine, Runtime implementation, SQLite, Provider, adapter,
  credential, or generated Proto modules in `tui/`;
- shipping fixture/fake transport use;
- environment lookup for Host, endpoint, credential, model, or database;
- `println!`/`eprintln!` after terminal acquisition outside the controlled
  screen-reader/restore boundary;
- raw Host/user content in tracing fields, errors, `Debug`, terminal title, or
  preference diagnostics;
- unbounded channel/collection/history/undo/reconnect loops;
- terminal setup without paired teardown and injected-failure tests;
- unsafe code, missing public docs, ignored relevant tests, or banned phrases.

## Verification commands

Focused commands are added to `Justfile` with their implementation:

```text
just tui-unit
just tui-snapshots
just tui-contract
just tui-persistence
just tui-runtime-e2e
just tui-pty
just tui-bench
just tui-boundaries
just tui
```

`just tui` runs every blocking focused gate except scheduled stress and
unavailable native-platform runs. Repository completion also requires:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
just architecture
```

Commands are not reported as passing unless their executable harness performs
the named work and the output has been read.

## Completion rule

The TUI is complete only when:

1. every Garive-required row in the competitive matrix has executable evidence;
2. all three focused Specs and the Host dependencies agree with code;
3. the shipping binary passes the repeated Runtime and PTY scenarios;
4. performance numbers are promoted from Proposed with stored baseline evidence;
5. local platform compatibility is verified and other claims remain scoped;
6. focused and repository gates pass from a clean worktree;
7. `spec/STATUS.md`, TUI README, project memory, and task registry state the same
   verified support and remaining external platform evidence.

## See also

- [`tui-application-architecture.md`](tui-application-architecture.md) — application and terminal invariants.
- [`tui-interaction-and-rendering.md`](tui-interaction-and-rendering.md) — UI and editor contract.
- [`tui-communication-and-persistence.md`](tui-communication-and-persistence.md) — Host/recovery/file contract.
- [`../../.agents/testing.md`](../../.agents/testing.md) — repository evidence maturity rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
