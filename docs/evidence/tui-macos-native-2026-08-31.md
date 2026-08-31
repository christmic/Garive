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

Merge revision `c9b22f76` extracts the composer into a bounded presentation
component and makes existing Shift-selection behavior visible. The editor
exports only a validated selected byte range; rendering walks extended
graphemes and never splits CJK or combining sequences. Reviewed style-run
snapshots bind the selected/unselected boundary in dark, light, and mono; mono
uses reverse video and does not depend on color.

On native macOS arm64, eight editor tests, 37 view tests, all 19
snapshot/boundary tests, and all nine shipping-binary PTYs passed. The added
Expect PTY starts the real binary at `100x24` in mono, types `a界b`, sends two
xterm Shift+Left sequences, and observes an emitted reverse-video `界` before
normal quit and alternate-screen restoration. Strict all-target/all-feature
Clippy passed with warnings denied; formatting and diff checks were clean.

Follow-up merge `3513b0f6` corrects selection collapse to the directional
edge. Plain Left/Right stops at the start/end edge without an extra grapheme;
word, vertical, line, and document motion continue from their corresponding
edge. Ten editor tests bind both anchor directions plus word/vertical behavior;
37 view and 19 snapshot/boundary tests remain green. All nine shipping PTYs
pass. The mono Expect case now selects `界b`, sends a real unmodified Left,
inserts `X`, observes `aX界b`, and then proves normal alternate-screen restore.
Strict all-target/all-feature Clippy, formatting, and diff checks passed.

Merge revision `7b2c50c0` gives the composer one Unicode layout for rendered
rows, selection spans, cursor placement, and scroll. Focused contracts cover
`hello world` at eight cells, a cursor exactly after a full row, explicit
newline followed by soft wrap, and selected CJK plus a combining sequence
across wrapped rows. On native macOS arm64 after rebasing to the then-current
`master`, all 10 editor, 41 view, 23 snapshot/boundary, and nine shipping-binary
PTY tests passed. Strict all-target/all-feature Clippy passed with warnings
denied; formatting and diff checks were clean. This is semantic-buffer and real
PTY evidence, not a physical Terminal/iTerm2-class screenshot.

Merge revision `1d1da7af` routes composer mouse placement and selection through
the shared Unicode layout. On native macOS arm64, 11 editor, 42 view, 24
snapshot/boundary, and all ten shipping-binary PTY tests passed. The added
`100x24` mono Expect PTY enables SGR mouse capture, types `a界b`, presses before
`界`, drags through `b`, observes reverse-video selection, types `X`, and
observes `aX`. Normal quit emits mouse disable and alternate-screen restore.
Strict all-target/all-feature Clippy passed with warnings denied; formatting
and diff checks were clean. This remains executable PTY evidence rather than a
physical-window screenshot.

Merge revision `92f54da7` derives composer frame height from visual wrapped
rows. The reviewed `40x16` mono snapshot shows both soft-wrapped rows inside a
four-row frame; the existing `40x12` product snapshot remains compact by
policy. On native macOS arm64, 43 view, 25 snapshot/boundary, and all ten
shipping-binary PTY tests passed. Strict all-target/all-feature Clippy passed
with warnings denied; formatting and diff checks were clean. The snapshot is
semantic buffer evidence, not a physical-window image.

Merge revision `e23018f4` makes Up/Down consume the same visual rows. On native
macOS arm64 after rebasing onto `1ae331e7`, all 11 editor, 45 view, 27
snapshot/boundary, and 11 shipping-binary PTY tests passed. The added `20x16`
Expect PTY types `hello wonderful world` as one logical line, sends a real Up
escape sequence, types `X`, and observes `helloX` on the first visual row; it
then exits normally and proves alternate-screen restoration. Focused unit
contracts additionally bind sticky terminal-cell columns, short rows, CJK
double-cell graphemes, and exact-width continuation. Strict
all-target/all-feature Clippy passed with warnings denied; formatting and diff
checks were clean. This is executable PTY evidence, not a physical-window
Terminal/iTerm2-class screenshot.

Merge revision `2dfceab6` routes Home/End through the shared visual layout. On
native macOS arm64 after rebasing onto `3373e909`, all 11 editor, 46 view, 28
snapshot/boundary, and 12 shipping-binary PTY tests passed. The new `20x16`
Expect PTY types `hello wonderful world`, sends real Up and End sequences,
inserts `X`, and observes `wonderfulX` at the first visible row's end rather
than `worldX` at the document end. It exits normally and proves
alternate-screen restoration. Strict all-target/all-feature Clippy passed with
warnings denied; formatting and diff checks were clean. This remains
executable PTY evidence, not a physical-window screenshot.

Merge revision `98754b25` adds visual-boundary prompt recall. On native macOS
arm64 after rebasing onto `f13b9b4a`, all 35 library, 11 editor, 48 view, 30
snapshot/boundary, and 13 shipping-binary PTY tests passed. The new `40x16`
Expect PTY creates an actual mode-`0600` `prompt-history.v1.jsonl`, types
`work`, moves its grapheme cursor left twice, and sends real Up/Down escape
sequences through `newest`, `oldest`, `newest`, and back to `work`. Inserting
`X` then produces `woXrk`, binding restoration of the original cursor rather
than only the draft text. The latest focused rerun completed in 1.21 seconds,
exited normally,
and proved alternate-screen restoration. Strict all-target/all-feature Clippy
passed with warnings denied; formatting and diff checks were clean. This is
executable shipping-binary evidence, not a physical-window screenshot.

Merge revision `ebd6ab0f` adds same-cell double/triple-click composer
selection. On native macOS arm64 after rebasing onto `f5635e75`, all 37
library, 12 editor, 50 view, 32 snapshot/boundary, and 14 shipping-binary PTY
tests passed. The new `100x24` monochrome Expect PTY sends real SGR down/up
pairs: double-click visibly selects `beta` and typing replaces it with `X`;
triple-click visibly selects both runs of the resulting `alpha X` logical line
and typing replaces it with `Y`. The raw transcript binds reverse-video word
and line spans, mouse disable, and alternate-screen restore sequences. Strict
workspace all-target/all-feature Clippy passed with warnings denied; formatting
and diff checks were clean. This is executable shipping-binary evidence, not a
physical-window screenshot.

Merge revisions `1ee5160c` and `d4bd68db` add explicit composer-selection copy
and responsive modal visual boundaries. A native `100x24` monochrome Expect
PTY types `alpha beta`, extends the selection over `beta` with real
Shift+Left sequences, observes the selection-specific `Alt+C copy` footer,
sends `Esc c`, and captures OSC 52 payload `YmV0YQ==`. The transcript asserts
that the full-draft payload `YWxwaGEgYmV0YQ==` is absent, then proves
alternate-screen restoration. The complete shipping-binary PTY suite passed
15/15 in 41.74 seconds after rebasing onto `ad18b683`.

The same candidate passed 37 library, 12 editor, 6 command, 50 view, and 32
snapshot/boundary tests. Manually reviewed `160x28` palette and `100x24`
Help, recovery, action, and Session-picker snapshots bind one-line unavailable
detail, the complete 17-command catalog and action footer, horizontal modal
separation, intact header/composer frames, and compact selection visibility.
Strict workspace all-target/all-feature Clippy passed before rebase; the exact
rebased TUI passed all-target/all-feature Clippy with warnings denied, format
check, and diff check. A full workspace test run reached an unrelated
creativity-baseline fixture and failed once with `generator_failure`; its exact
test passed when rerun alone. This is executable buffer/PTY evidence, not a
physical Terminal/iTerm2-class screenshot; that gallery remains open.

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

## Private composer kill/yank candidate

The source-audited kill/yank increment is frozen by `e0bac7c3` and implemented
at `750c17f6`. `input/kill_buffer.rs` is a 99-line single-entry component;
`input/editor.rs` remains 478 lines and `runtime/controller.rs` 434 lines, so
the production-module architecture gate passes without a relaxed bound. The
buffer is process-memory only, is independent of undo, and is cleared before a
different Session draft is loaded.

On macOS arm64, the isolated candidate target passes 37 library, 14 editor, 6
command, 50 view, 32 snapshot/boundary, and all 16 shipping PTY tests, plus
strict TUI all-target/all-feature Clippy, formatting, and diff checks. The new
`100x24` monochrome PTY enters `alpha`, a real newline, and `beta`; it proves
`Ctrl+U` kill, `Ctrl+Y` yank, `Ctrl+Z` undo, portable `Alt+Z` redo, clean exit,
and alternate-screen restoration. The screen-reader PTY separately announces
“Control U or Control K” and “Control Y” from the shared Help contract. The
reviewed Help snapshot grows one row and retains all three safety notes without
clipping.

This automated PTY and semantic snapshot evidence does not replace physical
Terminal/iTerm PNGs. That native gallery remains open while the macOS login UI
is locked.

## Typed terminal-key catalog candidate

The catalog is frozen by `fc936859` and implemented by `ca522fc9`.
`input/keymap.rs` owns typed chords, intents, and shared visual/spoken Help
metadata; `input/logical_line.rs` independently owns newline-delimited edge
calculation. The main editor remains 480 physical lines and the controller 440
physical lines; the architecture gate passes without a relaxed bound.

On macOS arm64, isolated targets pass 38 library, 15 editor, 6 command, 51
view, and 33 snapshot/boundary tests. Strict TUI all-target/all-feature Clippy,
formatting, diff, and architecture checks pass. A new `100x24` shipping PTY
drives the real composer through logical-line, grapheme, word, and delete
aliases and observes each intermediate screen state. The first parallel
17-case PTY run exposed two harness timing failures: the mouse case passed
alone, and the screen-reader driver had sent `Esc` immediately before
`Ctrl+Q`, which the terminal correctly combined as `Alt+Ctrl+Q` under exact
modifier matching. Revision `a4a9f525` gives those distinct user actions an
explicit parsing boundary and waits for the quit confirmation. The definitive
full rerun passes all 17 shipping-binary PTYs, with 0 failures or ignored cases,
in 41.65 seconds.

As above, this is executable macOS PTY evidence rather than an admitted
physical-window screenshot. The locked-login gallery gate remains open.

## Safe external-draft candidate

The external-editor implementation is `bd837ada`, the typed command exposure
is `770a3bc2`, and the locally integrated candidate is `a587cda8`.
`runtime/external_editor.rs` owns bounded editor resolution, private temporary
file lifecycle, child execution, result validation, and freshness checks.
`runtime/terminal_events.rs` owns one acknowledged pause/resume reader across
both full-screen and linear presentation. This avoids relying on crossterm
0.29's `EventStream` drop, whose worker is not joined, and clears ratatui's
cursor query while the reader is still paused before input ownership resumes.

On macOS arm64, the candidate passes 44 library, 16 editor, 6 command, 51 view,
33 snapshot/boundary, and all 18 shipping-binary PTY tests. The complete
`cargo test -p garive-tui` run also passes the production Runtime/file-SQLite/
PTY case. Architecture, strict all-target/all-feature Clippy, formatting, and
diff checks pass. The deterministic editor child proves stdin, stdout, and
stderr are TTYs, writes a multiline result, permits one `Ctrl+Z` restoration,
then exits 7 on a second invocation without changing the draft. The PTY also
proves temporary-file removal and complete raw, bracketed-paste, focus, mouse,
alternate-screen, title, and cursor-query restoration.

The reviewed `100x24` Help snapshot retains every binding and all three safety
notes after `Ctrl+G` is added by packing rows with actual Unicode display
width. The wide palette snapshot exposes `/edit-prompt`. These semantic
snapshots and ANSI PTY transcripts still do not satisfy physical Terminal or
iTerm image admission; that gallery remains open behind the locked login UI.

Revision `f62da6a1` closes the editor-ownership routing boundary. One exact
`composer_is_frozen` projection now feeds the visual palette, linear palette,
inline discovery, and activation reason; the central editor request repeats
the same fail-closed check. `Ctrl+G` reaches that request only after Composer
focus and freeze guards. The complete 18-case shipping PTY rerun passes in
146.11 seconds; the editor case first moves focus to Conversation and proves
`Ctrl+G` does not spawn the child, then returns to Composer and proves the full
successful/failing terminal handoff. The 44 library, 6 command, 51 view, 33
snapshot/boundary, architecture, formatting, diff, and strict Clippy gates also
pass.

## Superseded conversation position rail and preview

This section records historical evidence only. The accepted conversation-first
contract in `spec/design/tui-visual-system.md` removes the permanent rail and
hover preview; none of the evidence below admits the current TUI v2 design.

The source-backed contract is frozen by `cd17b5f4` and the componentized
implementation is merged locally at `1b93b115`. `view/position_rail.rs` owns
the bounded stable-cell metric, theme/monochrome glyphs, painting, and pointer
mapping. The conversation renderer and mouse controller consume that same
geometry; modal presentation suppresses both, and drag outside the one-cell
track cannot fall through to Composer or Session activation.

On native macOS arm64, the containing implementation passes 47 library, 54
view, and 35 snapshot/boundary tests. The reviewed `100x24` dark, light, and
monochrome snapshots show the track in existing right padding without message
reflow; compact snapshots bind the same rule. Architecture, strict TUI
all-target/all-feature Clippy, formatting, and diff checks pass after rebasing
onto the then-current `master`.

The complete 19-case shipping-binary PTY suite passed with 0 failures or
ignored cases in 146.47 seconds before that unrelated rebase. The new macOS
Expect case then passed on the merged tree: it loads 40 public timeline cells,
observes `#40`, presses the first track row to reach `#1`, drags to the middle
to reach `#22`, and presses the last row to return to `#40`. It also proves SGR
mouse capture and alternate-screen restoration. A final repeated run exposed
an `EAGAIN` race in the test-only nonblocking HTTP listener, not the product;
accepted sockets now explicitly use blocking reads, and two consecutive
focused PTY runs passed afterward.

The hover-preview contract is frozen by `61e2dbef` from the pinned Grok
timeline renderer and mouse-routing sources; implementation `16bee8a7` is
merged locally on top of `5f76a8ca`. The existing `view/position_rail.rs`
component now owns hover emphasis and the bounded two-line card as well as the
shared render/hit metric. Its transient model contains only stable-cell index
and screen row. Public role, ordinal, and sanitized excerpt are projected at
render time; no opaque identity or hover state is persisted.

On native macOS arm64, the final tree passes 47 library, 54 view, 35
snapshot/boundary, and the focused reducer lifecycle test. The definitive
19-case shipping-binary PTY suite passes serially with 0 failures or ignored
cases in 147.45 seconds. The rail case observes `Cell 22 · Garive` without
moving the `#40` viewport, moves off-track and proves the exact rail cell is
repainted, then proves `#40 -> #1 -> #22 -> #40`, SGR mouse-mode restoration,
and alternate-screen restoration. The run caught and closed two false redraw
assumptions: hover must not request a hard terminal clear/cursor query, and an
unchanged per-frame size report must not be treated as a resize. Architecture,
strict all-target/all-feature Clippy, formatting, and diff checks pass after the
final rebase.

These reviewed semantic buffers and real ANSI PTY transcripts still do not
satisfy physical Apple Terminal or iTerm2-class PNG admission. That gallery
remains open while the macOS login UI is locked.
