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
