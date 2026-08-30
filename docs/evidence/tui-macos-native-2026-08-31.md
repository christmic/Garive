# TUI macOS native candidate evidence

> Recorded: 2026-08-31. This is native, candidate-bound macOS arm64 evidence.
> It does not substitute for the still-open visual screenshot gallery or for
> native validation on other operating systems.

## Candidate and environment

| Field | Value |
|---|---|
| Garive revision | `c68a1d61d15bfc60a932e224cc91107c1f1cf242` |
| macOS | 26.6.1 (25G76) |
| Kernel | Darwin 25.6.0, arm64 |
| Rust | `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| Rust host | `aarch64-apple-darwin` |
| PTY driver | Expect 5.45.4, shipping binary |
| Multiplexer | tmux 3.7c, isolated socket, 120×28 pane |

## Passing gates

The current candidate linked in release mode:

```text
cargo build --release -p garive-tui --bin garive-tui \
  --example visual_demo_host
```

Result: exit `0`; the Runtime, ledger, Memory, TUI, and demo Host compiled in
12.99 seconds.

The complete native TUI package passed:

```text
cargo test -p garive-tui
```

Result: exit `0`; 19 test binaries; 75 passed, 0 failed, 0 ignored. The five
shipping-binary PTY cases passed in 21.51 seconds. The 97.37-second production
Runtime/file-SQLite/PTTY case covered completion, CJK editing and persistence,
suspension/continuation, cancellation, background completion, session
switching, clean exit, restart, and screen-reader replay.

Strict native linting passed:

```text
cargo clippy -p garive-tui --all-targets -- \
  -D warnings -D clippy::undocumented_unsafe_blocks
```

Result: exit `0`; `Finished dev profile` in 33.87 seconds.

## Componentized candidate rerun

After the semantic palette, reusable overlays, shared Session presentation,
focus-aware footer, keyboard/pointer rail, and height-aware selectable-list
windows landed, exact revision `c9d0b459` was rerun natively on the same macOS
arm64 host. `cargo test -p garive-tui -- --list` enumerated 90 test cases. The
complete package passed with no failed or ignored test; its five shipping-binary
PTY cases completed in 41.46 seconds and its production Runtime/file-SQLite/PTTY
case completed in 68.48 seconds. Strict all-target Clippy completed in 7.84
seconds with warnings denied.

The release shipping binary and `visual_demo_host` also linked successfully in
16.23 seconds. This rerun supersedes the older revision only for current-source
build/test admission; the performance distributions and tmux transcript below
remain pinned to their own named revisions. The physical-window and PNG rows
remain open because the login session is still locked.

Revision `882e158c` then added modal-safe pointer routing and shared overlay
geometry. `cargo test -p garive-tui -- --list` enumerated 93 cases. The complete
package passed, including six shipping-binary PTY cases in 41.20 seconds and a
new twice-repeated SGR-mouse workflow that clicked the visibly rendered
`/help` command, opened Help, exited cleanly, and proved mouse-capture restore.
The production Runtime/file-SQLite/PTTY flow passed in 69.34 seconds. Strict
all-target Clippy passed in 8.64 seconds and the release binary plus demo Host
linked in 9.13 seconds. This exact rerun still does not substitute for a
physical Terminal/iTerm-class window or admitted PNGs.

Revision `effe08f0` then extracted the append-only screen-reader overlays into
the shared view component layer. Command, Session, and prompt-history prompts
now consume the same filtered ordering, selection-following window, and
activation index as their visual overlays; Help names keyboard-only, newline,
no-color/no-mouse, and OSC 52 fallbacks. `cargo test -p garive-tui -- --list`
enumerated 94 cases. The complete package passed, including all six
shipping-binary PTY cases and the filtered screen-reader command-to-Help flow;
the production Runtime/file-SQLite/PTTY case completed in 69.44 seconds.
Strict all-target Clippy completed in 11.71 seconds and the release binary plus
demo Host linked in 13.82 seconds. Merge revision `f5d64c50` then passed the
focused library, visual-model, and snapshot suites on `master` (11 + 19 + 7
tests). Physical Terminal/iTerm-class validation and admitted PNGs remain open.

Merge revision `8aa1db9f` then replaced the remaining local command-availability
predicates with one typed command catalog. Visual command rows, linear
screen-reader announcements, and Enter activation now consume the same context
and safe unavailable reason; `/copy last` is no longer absent from the visual
catalog. The exact merged revision enumerated 96 test cases and passed the
complete package. Its six shipping-binary PTY cases completed in 41.35 seconds;
the production Runtime/file-SQLite/PTTY case completed in 69.64 seconds and the
screen-reader PTY proved the shared unavailable reason before opening Help.
Strict all-target Clippy completed in 13.22 seconds, and the release shipping
binary plus `visual_demo_host` linked in 1 minute 2 seconds. The physical-window
and admitted-PNG rows remain open.

Merge revision `0fa1dda8` then unified non-list action overlays behind one
typed application contract. Controller activation, visual popups, and linear
screen-reader output now share the same action bindings and safe copy for
unknown command results, status details, ephemeral confirmation, and quit.
Explicitly multiline status details retain Host, Session, and Cursor rows, and
popup geometry reserves space for every action after wrapping. The exact merged
revision enumerated 103 test cases and passed the complete package in 135.56
seconds. Its six shipping-binary PTY cases completed in 41.38 seconds, and its
production Runtime/file-SQLite/PTTY case completed in 69.21 seconds. Strict
all-target Clippy completed in 5.16 seconds; the release shipping binary plus
`visual_demo_host` linked in 16.68 seconds. Reviewed Help and recovery snapshots
cover the shared action copy, but remain semantic buffer evidence rather than
real terminal PNGs. The physical-window and admitted-PNG rows remain open.

Merge revision `42104cf2` added the content-free semantic terminal-title
component. Full-screen and linear presentation now share the grammar
`Garive · <Workspace|Session N|Session active> · <connection> ·
<execution>`; `Session N` is the currently admitted Host-page ordinal, never a
Session identifier. Connection and execution values come from bounded typed
state, so definition names, user or Agent content, internal identifiers, and
error codes cannot enter the title. The terminal guard suppresses unchanged
writes and restores the neutral `Garive` title on normal exit, drop, and unwind.

The exact merged revision enumerated 112 test cases and passed the complete TUI
package in 138.54 seconds. Its six shipping-binary PTY cases completed in 41.68
seconds; the production Runtime/file-SQLite/PTTY case completed in 69.28
seconds and asserted connecting, running, action-required, restarted-ready, and
neutral-reset title transitions. Strict all-target Clippy completed in 4.31
seconds, and the release shipping binary plus `visual_demo_host` linked in
15.27 seconds. The physical-window and admitted-PNG rows remain open.

Merge revision `5e2502d0` closed the empty reduced-motion switch with one pure
shared status-motion component. Typed Connecting, Reconnecting, and Following
states select a calm single-cell pulse; a 160 ms Tokio interval uses
`MissedTickBehavior::Skip`, holds every phase for two ticks, and is polled only
while the current status has a visible animated variant. `--reduced-motion`
uses the stable production renderer (`○ connecting` and plain `running`) and
does not schedule motion ticks. Idle, suspended, failed, and disconnected
states, linear screen-reader output, and semantic terminal titles remain
nonanimated.

The implementation was derived from direct local source inspection, not UI
guesswork: Codex centralizes motion primitives and reduced-motion fallback in
`codex-rs/tui/src/motion.rs`, and coalesces frame scheduling in
`codex-rs/tui/src/tui/frame_requester.rs`; Grok Build bounds and deduplicates
title animation in
`crates/codegen/xai-grok-pager/src/notifications/title.rs`. Garive keeps its
own frames, state semantics, and scheduler contract.

The exact merged revision enumerated 116 test cases. On macOS, all six
shipping-binary PTY cases passed in 41.50 seconds, the production
Runtime/file-SQLite/PTTY case passed in 69.67 seconds, and the complete TUI
package passed in 147.63 seconds. Strict all-target Clippy completed in 6.41
seconds; the release shipping binary plus `visual_demo_host` linked in 18.83
seconds. Dark, light, and monochrome reviewed snapshots cover the active frame;
shipping PTY assertions distinguish animated `· connecting` from stable
reduced-motion `○ connecting`, while the production transcript observes the
pulse phases. These are executable semantic/PTY evidence, not physical-window
screenshots. The physical Terminal/iTerm2-class and admitted-PNG rows remain
open.

TUI merge revision `6f5f43c7` then replaced the mutable Markdown style flag
with one bounded presentation component. Nested strong/emphasis/strike/link
styles now compose and restore their enclosing style; headings restore normal
body style; ordered lists retain their declared start; explicit links show a
sanitized destination capped at 120 characters without OSC 8; and fenced code
uses a semantic frame, bounded language label, four-cell tab expansion, and
grapheme/display-width clipping with an explicit `…`. Copy source remains the
unmodified Host text.

The design follows direct source inspection rather than product observation.
Codex `codex-rs/tui/src/markdown_render.rs:430-567` owns an inline-style stack,
link state, code language, and structured tables, with nested-style/link tests
in `markdown_render_tests.rs:717-768`. Grok Build independently separates
style, parsing, hyperlinks, code metadata, streaming, and syntax across
`xai-grok-markdown/src/`. Garive keeps its own smaller Host-safe renderer and
does not copy either implementation or activate terminal hyperlinks.

The integrated macOS candidate `9a96a8d5` contains that merge with no later TUI
change. It enumerated 124 test cases and passed the complete package in 142.15
seconds. Its six shipping-binary PTYs passed in 41.47 seconds and its production
Runtime/file-SQLite/PTTY case in 68.55 seconds. Strict all-target Clippy passed
in 6.46 seconds, and release `garive-tui` plus `visual_demo_host` linked in
16.47 seconds. Unit assertions cover style restoration, visible links, ordered
lists, code framing, tab expansion, CJK clipping, and terminal safety; reviewed
dark/light/mono semantic style-run snapshots cover the rich component. These
remain executable buffer evidence, not physical-window PNGs.

TUI merge revision `3b2b0f2c` replaces delimiter-only Markdown tables with the
bounded `markdown_table` component. It parses header, row, cell, alignment, and
styled-span state; caps input at 12 columns, 64 body rows, and 4,096 characters
per cell; allocates content-aware Unicode display widths for a semantic grid;
and transposes undersized tables into labeled records without mutating copied
Host source. Merge `741add34` freezes the six dark/light/mono wide-grid and
narrow-record style-run snapshots plus the source audit, visual rules,
interaction contract, and user guide.

The exact merged macOS candidate `741add34` enumerated 128 test cases. All six
shipping-binary PTYs passed in 41.46 seconds, the production Runtime/file-
SQLite/PTTY flow in 77.19 seconds, and the complete package in 145.60 seconds.
Strict all-target Clippy later passed on the containing current master
`5fb523d5` in 8.91 seconds; release `garive-tui` plus `visual_demo_host` linked
at the exact TUI candidate in 15.23 seconds. Unit and style-run assertions prove
grid/record switching, declared alignment, bold-cell preservation, CJK display
width, explicit label overflow, and maximum line width. The later Runtime and
Desktop commits did not change TUI sources, and the repository was clean at
`5fb523d5`. Physical Terminal/iTerm screenshots remain open and are not
substituted with ANSI captures.

TUI merge revision `351cca79` adds the componentized composer-adjacent command
menu. Direct source inspection at the pinned Codex, Grok Build, and Pi
revisions established the separate registry/state/view/controller ownership,
bounded visible window, prefix synchronization, explicit dismissal, and
geometry-derived pointer patterns. Garive keeps its own smaller catalog and a
separate `Ctrl+P` search surface. The catalog now exposes every admitted theme
and mouse parser variant, and a test requires every catalog row to parse.

On native macOS arm64, the exact merged source listed 151 all-feature TUI test
cases and the complete all-feature package passed. Eight shipping-binary PTY
cases passed, including an Expect-driven `100x24` flow that typed `/theme d`,
observed the anchored `Use dark theme` result, completed `/theme dark` with
Tab, executed it with the subsequent Enter, and restored the alternate screen.
Strict all-target/all-feature Clippy passed with warnings denied. Pure tests
bind prefix/dismissal state and geometry-derived row hit testing; reviewed
dark, light, and mono snapshots bind the five-row menu, selected marker,
semantic styles, composer alignment, and no modal backdrop. Screen-reader PTY
coverage retained its linear `Ctrl+P` flow and the controller explicitly
refuses to give it invisible suggestion-key ownership.

These are executable buffer and macOS PTY results, not physical-window images.
The login session remains locked, so Apple Terminal/iTerm2-class review and the
admitted PNG gallery remain open.

Follow-up merge `2c44743a` removes the last routing/presentation mismatch: the
context footer now derives the active suggestion state and shows `↑/↓ select`,
`Tab complete`, and `Esc close` instead of the inactive composer hints. A
reviewed monochrome `40x12` snapshot proves the compact fallback keeps only
`Tab complete` and `Esc close`; the three `100x24` theme snapshots prove the
full hint set. The focused shipping PTY and strict all-target/all-feature
Clippy passed again.

Merge revision `15d942ae` closes the compact row and real pointer path. The
command component now reserves one horizontal cell inside each border, keeps
the canonical command visible, and truncates only secondary detail with an
explicit `…` at grapheme-safe Unicode display width. English and CJK unit
assertions bind the boundary; geometry tests prove that border/padding cells
cannot activate a row. Updated dark, light, and mono `100x24` snapshots and the
monochrome `40x12` boundary were reviewed.

On native macOS arm64, 36 view tests, 19 snapshot/boundary tests, and all eight
shipping-binary PTY cases passed on the containing source. A new `100x24`
Expect PTY starts the shipping binary with `--mouse on`, types `/theme d`,
observes `Use dark theme`, sends an SGR left click to the rendered suggestion
row, and observes `/theme dark` without premature execution. It then executes,
quits normally, and the transcript contains both `?1000h` and `?1000l`, proving
mouse capture restoration. Strict all-target/all-feature Clippy passed with
warnings denied; formatting and diff checks were clean.

## Terminal behavior checked during this run

Launching the release shipping binary in a macOS PTY whose actual environment
reported `TERM=dumb` selected the linear screen-reader presentation without an
explicit flag. It reached `Connection online`, accepted the normal confirmed
quit path, printed `Terminal restored.`, and emitted the bracketed-paste,
focus, and cursor restoration sequences. A separate xterm-compatible Expect
PTY answered the cursor-position query and rendered the full 120×28 workspace.

The system login session remained locked: an OS screen capture was entirely
black, and Computer Use was not permitted to control Terminal or Codex. No PNG
from that state is admitted as product evidence. The comprehensive real visual
gallery therefore remains open rather than being replaced with ANSI output or
synthetic images.

## Native tmux acceptance

The release shipping binary was launched inside a detached tmux 3.7c session
with an isolated socket and a fixed 120×28 pane. It connected to production
`LiveHost` backed by `/tmp` file SQLite, rendered the complete workspace, and
submitted the exact CJK prompt `tmux native 你好`. The durable projection showed
one completed Turn and the unique response `Churn event 0 committed.`.

`Ctrl+P` opened the complete command palette above the live conversation.
`Ctrl+Q` opened the durable-Session quit confirmation, and Enter exited the
shipping binary with status 0. A shell wrapper recorded `stty -g` immediately
before acquisition and after exit; the complete snapshots were identical:

```text
gfmt1:cflag=4b00:iflag=6b02:lflag=5cb:oflag=3:discard=f:dsusp=19:
eof=4:eol=ff:eol2=ff:erase=7f:intr=3:kill=15:lnext=16:min=1:quit=1c:
reprint=12:start=11:status=14:stop=13:susp=1a:time=0:werase=17:
ispeed=9600:ospeed=9600
```

This closes the native macOS tmux row for revision `d907ea63`. It does not
close physical Terminal/iTerm2-class window behavior or screenshots.

## Related performance gate

Revision `d907ea63`, which includes this evidence plus concurrent integration,
was rerun through the release outer-process and bounded-memory gates. Three
first-frame p95 values were 29.907, 27.146, and 29.520 ms; three 60-second idle
runs recorded no CPU-time increase at the measurement resolution; three loaded
10-Session/5,000-cell processes peaked at 4,177,920, 4,227,072, and 4,227,072
bytes. Raw reports are stored beside this document as
`tui-release-process-2026-08-31.json`,
`tui-release-memory-2026-08-31.json`, and
`tui-release-in-process-2026-08-31.json`.

The same source candidate lineage has a passing 30-minute production
reconnect/Turn churn report in
[`tui-release-churn-2026-08-31.json`](tui-release-churn-2026-08-31.json). That
gate is pinned to its exact pre-documentation revision `8b077f12`; this native
suite rerun proves the later merged candidate remains green after concurrent
Runtime, Desktop, and Mobile integration.
