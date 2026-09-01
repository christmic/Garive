# Garive TUI

Resident, restart-safe terminal client for the loopback Garive Host. The
shipping binary provides a responsive Ratatui workspace, durable Session
navigation, multiline Unicode editing, safe Markdown, H1 event follow with
bounded reconnect, H2 timeline restore, H3 activity, real H4 progressive
answers with durable convergence, schema-bound suspension replies, exact
idempotent retry, and private local presentation state.

See the complete [TUI user guide](../docs/manual/tui-user-guide.md) for setup,
workflows, every key and command, recovery, accessibility, privacy, and
troubleshooting.

## Run

Start a configured Runtime Host, then launch:

```text
cargo run -p garive-tui -- --host http://127.0.0.1:4317/
```

Useful launch options:

```text
--session <id>             open a durable Session
--definition <id>          default Agent definition for new Sessions
--theme system|dark|light|mono
--mouse auto|on|off
--screen-reader            linear output without alternate-screen addressing
--reduced-motion
--state-dir <absolute-path>
--no-prompt-history
--ephemeral                disable every local state write
```

`--host` must be an explicit credential-free loopback HTTP URL. Run
`garive-tui --help` for the complete contract.

## Interaction

- `Enter` sends; `Ctrl+J` inserts a newline.
- `Ctrl+A`/`Ctrl+E` move to logical-line edges; `Ctrl+B`/`Ctrl+F` move by
  grapheme; `Alt+B`/`Alt+F` move by word. `Home`/`End` remain visual-row
  boundaries for wrapped text.
- `Ctrl+H`/`Ctrl+D` delete a grapheme; `Ctrl+W`/`Alt+D` delete a word.
- `Ctrl+U`/`Ctrl+K` kill to a logical line edge and `Ctrl+Y` yanks from one
  private in-memory entry; `Ctrl+Z`/`Alt+Z` undo/redo. Session changes clear
  the private entry, and it never touches OSC 52 or persistence.
- `Ctrl+G` or `/edit-prompt` hands an unfrozen draft to `VISUAL`, then `EDITOR`,
  without a shell. Zero exit applies one undoable replacement; failure keeps
  the draft while Garive restores and reacquires the same terminal.
- `Ctrl+N` creates a Session; entering the first prompt also creates one when a
  single Agent definition is installed.
- `Ctrl+S`, `Ctrl+P`, and `Ctrl+R` open Sessions, commands, and prompt history.
- `Esc` requests cancellation while a Turn runs; `Ctrl+Q` confirms exit.
- `PageUp`/`PageDown` scroll by visual cells. Scrolling away detaches the
  viewport; new durable or live updates do not steal it, and `End` resumes
  latest-follow. `/jump [filter]` opens the bounded public Turn navigator.
- `/inspect [activity|recovery|details|close]` opens the safe Inspector. At
  129 columns and wider it shares the surface without narrowing the 96-column
  answer; 120–128 columns retain that answer measure and use an overlay;
  narrower terminals use the same projection in an overlay.
- `/help` lists commands. `/retry` repeats the exact persisted command identity;
  `/copy last`, `/copy selection`, and `/copy session-id` use a bounded OSC 52
  request. `Alt+C` is the direct explicit composer-selection gesture.

These immutable defaults and the visual/screen-reader Help labels are owned by
the typed `input/keymap.rs` catalog. Contextual `Enter` and `Esc` ownership is
kept in the overlay/composer controller and covered by shipping PTYs.

The Host/SQLite ledger is the only durable conversation authority. Local files
contain preferences, bounded drafts, prompt history, and exact pending-command
recovery envelopes only. Unix state directories/files are enforced as
`0700`/`0600`. Windows state uses a protected current-user-only ACL under
`%LOCALAPPDATA%\Garive\tui`. `--ephemeral` requires confirmation before
mutations.

Verified native targets currently include macOS arm64 and Linux arm64 with an
xterm-compatible PTY; the Linux run covers the shipping binary and production
Runtime/SQLite round trip. The complete Windows target passes source-level
all-target check and strict Clippy, including its private-state backend, while
native linking, ACL execution, and ConPTY remain open. Linux x86_64 likewise
passes complete source-level checking and strict Clippy, but its native
execution, physical terminal, tmux, and `TERM=dumb` gates remain open.
The current macOS candidate's full native results are recorded in
[macOS native evidence](../docs/evidence/tui-macos-native-2026-08-31.md),
including a production Host/SQLite/CJK run under tmux 3.7c with exact termios
restoration. Physical Apple Terminal plus an iTerm2-class terminal and the
candidate-bound PNG gallery remain open; automated PTY and semantic snapshots
do not close that visual gate.

## Verify

```text
just tui-unit
just tui-snapshots
just tui-persistence
just tui-runtime-e2e
just tui-pty
just tui
```

The Runtime E2E launches the shipping binary in a real PTY against the
production HTTP Host and SQLite ledger, then covers completion, suspension,
continuation, cancellation, real H4 intermediate output, durable replacement,
exit, restart, and screen-reader replay. Separate H4 recovery coverage proves
preview unavailability, reconnect snapshot replacement, two live frames before
durability, and final Ledger convergence. Detached-live scrolling and
screen-reader no-per-delta evidence remain completion gates.

The release-only `release_churn_baseline` example runs that same production
path for at least 30 minutes with unique committed Turns and reconnect churn.
The pinned macOS arm64 candidate completed 1,426 reconnects and 143 Turns with a
12,784 KiB TUI RSS peak; see the
[machine-readable report](../docs/evidence/tui-release-churn-2026-08-31.json).

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: resident product implementation; see `spec/STATUS.md` for evidence
