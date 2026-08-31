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

The macOS arm64 release first-frame, idle CPU, and resident-memory metrics are
now Gates. Pinned evidence at Garive
`d0cfc1c01da30d9389907fbb1bb4b61db1eee34b` runs the shipping binary against
production `LiveHost` plus file SQLite in a real PTY. Three independent
20-sample first-frame runs observed p95 `24.971 ms`, `26.098 ms`, and
`25.113 ms`; the unsmoothed distributions retain the `527.282 ms` first-run
maximum. Three 60-second online-idle runs recorded `<10 ms` CPU time each at
the measurement backend's resolution, proving `<0.017%` of one logical core.
Three isolated release children containing exactly 10 Sessions and 5,000
loaded production timeline cells peaked at `4,128,768`, `4,210,688`, and
`4,145,152` bytes. Raw reports are stored in
`docs/evidence/tui-release-process-2026-08-30.json` and
`docs/evidence/tui-release-memory-2026-08-30.json`. These acceptances are
specific to the named reference environment.

Stress tests also prove cancellation latency under a saturated Host channel.
The candidate-bound release harness proves memory stability during 30 minutes
of bounded event/reconnect churn: Garive `8b077f12` completed 1,426 reconnects
and 143 unique committed Turns in 1,800.080 seconds. TUI RSS peaked and ended at
12,784 KiB; the late five-minute peak exceeded the early peak by 1,104 KiB,
passing the 100 MiB absolute and 20 MiB window-growth gates. The raw report is
stored in `docs/evidence/tui-release-churn-2026-08-31.json`.

## Compatibility matrix

| Platform | Build gate | Native/PTY gate |
|---|---|---|
| macOS arm64 | native release build, full TUI test, and strict Clippy verified | automated xterm-compatible and `TERM=dumb` PTYs plus tmux 3.7c verified; physical Apple Terminal plus one of iTerm2/WezTerm/Kitty and screenshot gallery remain open |
| Linux arm64 | native workspace build/test verified | xterm-compatible automated PTY verified; physical emulator, tmux, and `TERM=dumb` remain open |
| Linux x86_64 | workspace build/test; source-level all-target check and strict Clippy are necessary but not sufficient | xterm-compatible PTY; tmux; `TERM=dumb` refusal/linear fallback |
| Windows x86_64 | MSVC build/test; source-level all-target check and strict Clippy are necessary but not sufficient | Windows Terminal ConPTY; ACL execution; signal-equivalent restore |

The current Windows source-level result is pinned in
[`../../docs/evidence/tui-windows-cross-build-2026-08-30.md`](../../docs/evidence/tui-windows-cross-build-2026-08-30.md).
It compiles and lints every TUI target but does not close native linking,
execution, ACL, or ConPTY rows.

The corresponding Linux source-level result is pinned in
[`../../docs/evidence/tui-linux-cross-build-2026-08-30.md`](../../docs/evidence/tui-linux-cross-build-2026-08-30.md).
It covers every x86_64 TUI target but does not close native x86_64 linking or
execution. Native Linux arm64 linking, full tests, production Runtime/SQLite,
and automated PTY evidence are pinned in
[`../../docs/evidence/tui-linux-native-2026-08-30.md`](../../docs/evidence/tui-linux-native-2026-08-30.md);
physical terminal, tmux, and `TERM=dumb` rows remain open.

The current macOS arm64 componentized candidate rerun is pinned in
[`../../docs/evidence/tui-macos-native-2026-08-31.md`](../../docs/evidence/tui-macos-native-2026-08-31.md).
Merge revision `42104cf2` covers release linking, all 112 listed TUI test cases,
strict Clippy, production Runtime/file-SQLite/PTTY execution, and six repeated
shipping-binary PTY cases. These include content-free semantic terminal titles
shared by full-screen and linear presentation, bounded typed connection and
execution state, Host-page ordinal labels rather than identifiers, unchanged
write suppression, and neutral reset on normal exit, drop, and unwind. Real PTY
coverage asserts live title transitions. The same evidence lineage also covers
SGR-mouse modal
activation/restoration, filtered screen-reader command activation, the same
typed command-availability reason across visual, linear, and activation paths,
and one typed action-overlay contract across controller activation, visual
popups, and linear output. Multiline status details and adaptive popup geometry
are verified alongside reviewed Help and durable-recovery snapshots. Earlier
exact revisions in the same evidence record cover the automatic
`TERM=dumb` accessible fallback and native tmux 3.7c with terminal restoration;
those rows are not relabeled as a later rerun. The locked login session
prevented real-window screenshot admission.

Merge revision `5e2502d0` covers the centralized status-motion contract and all
116 listed TUI test cases. One pure component maps typed active state to a calm
single-cell pulse, while the normal static renderer is the real
`--reduced-motion` path. Its 160 ms skip-on-miss scheduler is selected only for
Connecting, Reconnecting, or Following; each phase is held for two ticks, and
idle, terminal, screen-reader, reduced-motion, and semantic-title paths do not
animate. Dark, light, and monochrome snapshots review the same component, and
shipping PTY coverage asserts the animated and reduced paths separately. The
exact macOS package, Runtime/file-SQLite/PTTY, Clippy, release, and six-PTY
timings are pinned in the evidence document. Physical Terminal/iTerm2-class
screenshots remain an open native gate.

TUI merge revision `6f5f43c7`, verified in integrated macOS candidate
`9a96a8d5`, covers the shared rich-Markdown presentation component and all 124
listed TUI cases. Exact unit evidence binds nested-style restoration, heading
scope, preserved ordered indices, sanitized bounded link targets, semantic
fenced-code framing/language, tab expansion, grapheme-safe CJK clipping, and
terminal-control rejection. Dark, light, and monochrome style-run snapshots
record both content and semantic Ratatui style, rather than text symbols alone.
The complete package, six shipping PTYs, production Runtime/file-SQLite/PTTY,
strict Clippy, and release timings are pinned in the evidence document.

TUI merge revision `351cca79` covers the prompt-adjacent command-discovery
component and all 151 listed all-feature TUI cases. The typed catalog now
enumerates every admitted parser variant; model prefix/dismissal state, pure
anchored rendering and mouse geometry, and controller key completion have
separate owners. Dark, light, and mono snapshots cover selection and composer
alignment without a modal backdrop. A shipping macOS PTY types `/theme d`,
observes the bounded menu, completes with Tab, executes on the next Enter, and
proves terminal restoration. All eight all-feature shipping PTYs and strict
all-target/all-feature Clippy pass. Screen-reader input remains on the shared
linear `Ctrl+P` catalog. Physical Terminal/iTerm2-class screenshots remain an
open native gate.

Follow-up merge `2c44743a` binds the contextual footer to that same input-owner
state. Three `100x24` theme snapshots show the complete navigation hints, while
the `40x12` monochrome boundary retains completion and dismissal without
clipping. The focused macOS shipping PTY and strict all-target/all-feature
Clippy pass on the containing source.

Merge revision `15d942ae` adds the command-row width and pointer contract.
Reviewed dark, light, and mono `100x24` snapshots plus the monochrome `40x12`
boundary bind the one-cell inner breathing room and explicit detail ellipsis.
Pure tests bind grapheme-safe English/CJK display-width truncation and exclude
padding from hit testing. On native macOS, 36 view tests, 19 snapshot/boundary
tests, and all eight shipping-binary PTYs pass. The new Expect PTY enables SGR
mouse capture, clicks the actually rendered `/theme dark` row, observes
completion without execution, then proves mouse capture and terminal teardown.
Strict all-target/all-feature Clippy passes with warnings denied.

Merge revision `c9b22f76` makes composer selection an explicit component
contract. `view/composer.rs` now owns frame, viewport, styled editor text, and
cursor geometry, while the editor exposes only its validated grapheme-aligned
byte range. Eight editor tests and 37 view tests bind combining/CJK boundaries,
selection visibility, and mono reverse video. Three reviewed semantic style-run
snapshots bind dark, light, and monochrome tokens. All 19 snapshot/boundary
tests and nine shipping-binary macOS PTYs pass; the new Expect case types
`a界b`, sends two real Shift+Left sequences, observes reverse-video `界`, and
proves alternate-screen restoration. Strict all-target/all-feature Clippy,
formatting, and diff checks pass.

Follow-up merge `3513b0f6` binds directional selection collapse. Ten editor
tests prove left/right stop at the corresponding edge without extra movement
and word/vertical moves continue from the correct edge for either anchor
direction. The 37 view and 19 snapshot/boundary tests remain green. All nine
shipping-binary macOS PTYs pass; the selection PTY now collapses a visible CJK
selection with a real Left sequence, inserts `X`, and observes `aX界b` before
clean restoration. Strict all-target/all-feature Clippy, formatting, and diff
checks pass.

Merge revision `7b2c50c0` replaces independent widget-wrap and cursor math with
the composer's shared Unicode layout. Unit contracts cover whitespace-first
wrapping, hard continuation at exact width, explicit newline plus soft wrap,
and selected CJK/combining graphemes across rows. After rebasing onto the then
current `master`, 10 editor tests, 41 view tests, 23 snapshot/boundary tests,
and all nine shipping-binary macOS PTYs passed. Strict all-target/all-feature
Clippy, formatting, and diff checks passed. These results are executable
semantic and PTY evidence; they do not close the physical-window image gate.

Merge revision `1d1da7af` adds composer mouse placement and drag selection
through the shared layout hit test. Eleven editor tests bind anchor-preserving
grapheme placement, 42 view tests bind whitespace wrapping and CJK double-cell
hit points, and 24 snapshot/boundary tests remain green. All ten
shipping-binary macOS PTYs pass. The added SGR-mouse PTY selects `界b`, observes
mono reverse video, replaces the selection with `X` to produce `aX`, and proves
mouse-capture plus alternate-screen restoration. Strict all-target/all-feature
Clippy, formatting, and diff checks pass.

Merge revision `92f54da7` makes composer frame height consume visual wrapped
rows instead of logical newline count. Contracts cover an empty draft,
whitespace-wrapped prose, and an exact-width continuation cursor. A reviewed
`40x16` monochrome product snapshot binds two visible soft-wrapped rows, while
the existing `40x12` snapshot preserves the tiny-height policy. All 43 view,
25 snapshot/boundary, and ten shipping-binary macOS PTY tests pass, as do
strict all-target/all-feature Clippy, formatting, and diff checks.

Merge revision `e23018f4` routes composer Up/Down through that shared visual
layout at the actual responsive inner width. Contracts cover sticky
terminal-cell columns, shorter-row clamping, CJK double-cell graphemes, and an
exact-width continuation row. After rebasing onto `1ae331e7`, all 11 editor,
45 view, 27 snapshot/boundary, and 11 shipping-binary macOS PTY tests passed.
The new `20x16` Expect case types a single logical line, sends a real Up escape
sequence across its soft wrap, inserts `X` at the first visual row's target,
and proves normal terminal restoration. Strict all-target/all-feature Clippy,
formatting, and diff checks passed.

Merge revision `2dfceab6` closes Home/End against the same component geometry.
Contracts cover whitespace soft wraps, wrapped-row starts and ends,
directional selection edges, and exact-width empty continuation rows. After
rebasing onto `3373e909`, all 11 editor, 46 view, 28 snapshot/boundary, and 12
shipping-binary macOS PTY tests passed. The added `20x16` Expect case moves to
the first visual row, sends a real End sequence, inserts `X` after
`hello wonderful`, and proves it did not jump to the logical document end.
Strict all-target/all-feature Clippy, formatting, and diff checks passed.

Merge revision `98754b25` adds a dedicated transient prompt-history browser at
the shared visual-row boundary. Pure contracts prove bounded older/newer
movement and exact restoration of the original draft plus grapheme cursor.
After rebasing onto `f13b9b4a`, all 35 library, 11 editor, 48 view, 30
snapshot/boundary, and 13 shipping-binary macOS PTY tests passed. The added
`40x16` Expect case writes a real mode-`0600` history file, types `work`, moves
the cursor left twice, traverses `newest -> oldest -> newest -> work`, then
inserts `X` and observes `woXrk`. It exits normally and proves alternate-screen
restoration. Strict all-target/all-feature Clippy, formatting, and diff checks
passed.

Merge revision `ebd6ab0f` adds a pure same-cell multi-click classifier plus
grapheme-safe word/punctuation and logical-line selection. Contracts cover the
500 ms boundary, position/reset cancellation, Unicode alphanumeric/underscore
runs, punctuation runs, whitespace, CJK, and trailing-newline inclusion. After
rebasing onto `f5635e75`, all 37 library, 12 editor, 50 view, 32
snapshot/boundary, and 14 shipping-binary macOS PTY tests passed. The added
`100x24` mono SGR-mouse PTY double-clicks `beta`, observes reverse video and
replaces it with `X`; it then triple-clicks the resulting `alpha X` line,
observes both styled runs, replaces the line with `Y`, and proves mouse-mode
plus alternate-screen restoration. Strict workspace all-target/all-feature
Clippy, formatting, and diff checks passed.

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
- terminal-title presentation without hostile-content canaries, unchanged-write
  suppression, and normal/drop-path neutral reset tests;
- unbounded channel/collection/history/undo/reconnect loops;
- terminal setup without paired teardown and injected-failure tests;
- unsafe code outside the single audited Windows persistence FFI boundary,
  missing public docs, ignored relevant tests, or banned phrases.

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

`just tui` runs every blocking focused gate except the 30-minute release churn
and unavailable native-platform runs. The churn gate is an explicit release
command documented with its pinned result above. Repository completion also
requires:

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
