# Garive TUI user guide

Garive TUI is the resident terminal client for a local Garive Runtime Host. It
keeps the terminal responsive while durable Sessions and Turns continue in the
Host, and it rebuilds conversation state from Host read models after restart.

## Support status

The verified native environments are macOS arm64 and Linux arm64 with an
xterm-compatible pseudo-terminal. Linux arm64 evidence includes the shipping
binary and production Runtime/SQLite round trip. macOS also has a verified
tmux 3.7c production Host/SQLite/CJK/terminal-restoration run. The full-screen client
requires interactive stdin and stderr. Windows passes source-level all-target
check and strict Clippy, but native linking, ACL execution, and ConPTY remain
open. Linux x86_64 also passes those source-level gates; its native execution,
physical-terminal, tmux, and `TERM=dumb` gates remain unverified.

The TUI does not start a Runtime, select provider credentials, or read the
Runtime database. A configured Runtime Host must already be listening on an
explicit loopback HTTP URL.

## Build and launch

Build the optimized executable from the repository root:

```text
cargo build --release -p garive-tui
```

Launch it against the local Host:

```text
target/release/garive-tui --host http://127.0.0.1:4317/
```

The Host URL is required. It must be credential-free, use `http`, resolve to a
loopback address, and contain no query or fragment. Garive never discovers the
endpoint, credentials, model, provider, or Runtime database from environment
variables.

Use `garive-tui --help` to inspect the executable's current CLI contract.

## Launch options

| Option | Purpose |
|---|---|
| `--host <URL>` | Connect to the required loopback Host root. |
| `--session <ID>` | Select a durable Session after bootstrap. |
| `--definition <ID>` | Choose the installed Agent definition used for new Sessions. |
| `--state-dir <ABSOLUTE_PATH>` | Override the private local presentation-state directory. |
| `--theme system\|dark\|light\|mono` | Override the saved color preference for this process. |
| `--mouse auto\|on\|off` | Override the saved mouse-capture preference. |
| `--screen-reader` | Use ordered linear output without alternate-screen addressing. |
| `--reduced-motion` | Replace active status pulses with stable glyphs and stop their redraw timer. |
| `--no-prompt-history` | Disable prompt-history reads and writes without deleting the file. |
| `--ephemeral` | Disable all local state and diagnostics writes. |

`TERM=dumb` automatically selects screen-reader mode and disables mouse
capture. `NO_COLOR` selects the monochrome theme unless `--theme` explicitly
overrides it.

Ephemeral mode cannot preserve an unknown mutation response across process
exit. The TUI therefore asks for one in-product confirmation before it permits
Host mutations in that mode.

## Interface tour

At standard and wide sizes the screen has four regions:

1. The header shows the selected Agent definition and Session, connection
   state, and Turn state.
2. The Session rail lists durable Sessions and their latest public Turn state.
3. The conversation presents bounded public timeline cells from the Host.
4. The composer and footer provide multiline editing, byte usage, and relevant
   shortcuts.

The Session rail spans the workspace height, while the conversation, composer,
and contextual shortcut footer stay aligned in one main column. The selected
Session has a `▌` marker; state remains readable without color through `✓`
completed, `●` running, `!` action required, `×` failed, and `■` stopped.

At widths below 100 columns the Session rail is hidden and the conversation
uses the available width. At very wide sizes the main column is centered and
capped to a readable measure. A focused composer has a double border. Opening a
picker or palette visibly dims the workspace and highlights the selected row.
Below 20 columns or 8 rows the client displays `Need 20×8`; the draft and
viewport state remain intact while the terminal is resized back.

The header connection states are connecting, online, disconnected,
reconnecting, and unavailable. Turn states are ready, running, action required,
and failed. A disconnected stream never implies that a running Turn completed.
The terminal tab title mirrors the safe workspace/Session ordinal, connection,
and Turn state so multiple Garive tabs remain distinguishable. It never includes
prompt text, Agent output, definition names, error codes, or opaque IDs, and is
reset to `Garive` when the client restores the terminal.

Agent responses render safe headings, nested emphasis, block quotes, tables,
task and numbered lists, inline code, and framed fenced code. A fenced language
appears in its `CODE` header. Code that exceeds the conversation width ends in
`…` instead of wrapping into an ambiguous gutter; tabs display as four spaces.
Common recognized fenced languages are syntax-highlighted with Garive's own
dark, light, or monochrome semantic styles. Use a language label such as
`rust` or `json` after the opening fence to enable it. Garive does not guess an
unlabeled language; unknown labels and unusually large blocks remain readable
as plain framed code. Highlighting never changes `/copy last` source text.
Markdown links show both their label and bounded destination as inert terminal
text, so the target remains inspectable even when OSC hyperlinks are disabled.
Tables use a compact aligned grid when the conversation has room. In a narrow
pane, Garive automatically changes each row into labeled `Header: Value`
records, so columns do not disappear off-screen; bold, links, CJK text, and
alignment remain readable. Resizing switches presentation without changing the
response text copied by `/copy last`.

## First Session and first Turn

When exactly one Agent definition is installed, type a prompt and press Enter.
Garive creates a Session, waits for the durable acknowledgement, then submits
the queued prompt. `Ctrl+N` creates an empty Session explicitly.

When several definitions are installed, launch with `--definition <ID>` or run:

```text
/new "definition-id"
```

The definition must be installed according to the Host bootstrap response.
Garive does not invent or persist provider/model choices in the TUI.

## Composer keys

| Key | Result |
|---|---|
| `Enter` | Submit the prompt or selected slash command. |
| `Ctrl+J` or `Shift+Enter` | Insert a newline. |
| Arrow keys | Move by grapheme or visual line. |
| `Alt+Left` / `Alt+Right` | Move by word. |
| `Home` / `End` | Move to the visual line boundary. |
| `Ctrl+Home` / `Ctrl+End` | Move to the document boundary. |
| `Shift` plus a movement key | Extend the selection. |
| `Backspace` / `Delete` | Delete the selection or one grapheme. |
| `Alt+Backspace` / `Alt+Delete` | Delete one word. |
| `Ctrl+Z` / `Ctrl+Y` | Undo or redo. |
| `Alt+C` | Copy exactly the active composer selection through terminal OSC 52. |
| `Ctrl+C` | Clear a selection, then a nonempty draft, then ask to quit on a second empty press. |

Selected composer text is visibly highlighted in dark and light themes and
uses reverse video in monochrome. Selection follows complete displayed
characters, so CJK, emoji families, and combining marks are never split. Left
or Right without Shift collapses the selection to that edge without moving an
extra character; word and line movements continue from their directional edge.

Typing `/` at the first column of a focused, single-line composer opens a
compact command menu above it. Continue typing to narrow by command prefix.
Use Up/Down (or Shift+Tab) to move, Tab or Enter to complete, and Escape to
dismiss without changing the draft. Completion is deliberately separate from
execution: press Enter again to run the completed command. `/new` and
`/sessions` complete with a trailing space for their optional argument.

The menu shows at most five rows, names why an action is unavailable, and does
not move the conversation or composer. A mouse wheel moves its visible
selection and a left click completes the hit row; the border and surrounding
padding do nothing. On a narrow screen, the command remains visible while its
description ends with `…` at a safe Unicode boundary. The menu is hidden for
multiline or argument text, small terminals below `30x12`, modal overlays, and
screen-reader mode. Use `Ctrl+P` for help-text search or complete linear
screen-reader discovery.

The composer accepts bracketed multiline paste as one edit, normalizes CRLF,
expands tabs to spaces, rejects unsafe controls, and enforces the Host's 4,096
UTF-8-byte command limit. Cursor movement follows Unicode grapheme boundaries,
including CJK, emoji, and combining sequences.

Long drafts wrap at readable whitespace when possible and at a complete
displayed character when a word is wider than the composer. Explicit newlines,
wrapped text, selection highlighting, the cursor, and automatic vertical
scroll all use the same terminal-cell layout, so the cursor remains aligned at
line boundaries and while a narrow composer scrolls.
Up and Down move between those visible wrapped rows even when the draft has no
newline. Repeated vertical moves keep the original screen column where the row
has one and otherwise land at its nearest complete-character boundary; CJK and
combining characters are never split.
Home and End move to the current visible wrapped row's start or end; use
Ctrl+Home and Ctrl+End for the whole draft. At a soft-wrap boundary, End stays
on the visible row instead of unexpectedly jumping to the final logical line.
At the first visible row, unmodified Up begins browsing submitted local
prompts; repeated Up goes older. While browsing, Down goes newer only after the
cursor reaches the recalled prompt's last visible row. Down after the newest
restores both the text and cursor position you had before browsing, so editing
continues at the same place. Typing, pasting, submitting, clearing, or switching
the composer ends that browse session. Shift+Up/Down remains text selection,
and `Ctrl+R` remains searchable history rather than sequential recall.
When height permits, the composer grows from three to seven rows according to
what is visibly wrapped, even if the draft contains no explicit newline. On an
extremely short terminal it stays compact and scrolls internally so the
conversation and active footer controls remain available.

While a mutation has an unknown durable result for the active Session, the
draft is frozen to prevent a second conflicting command. Global Help remains
available with `?` during that recovery state.

## Global and navigation keys

| Key | Result |
|---|---|
| `Ctrl+N` | Create a Session. |
| `Ctrl+S` | Open the searchable Session picker. |
| `Ctrl+P` | Open the searchable command palette. |
| `Ctrl+R` | Open local prompt history. |
| `?` | Open Help when the composer is empty or recovery-frozen. |
| `Tab` / `Shift+Tab` | Move focus among visible regions. |
| `Up` / `Down`, `Home` / `End`, `Enter` in the Session rail | Move its visible selection, jump to an edge, or open that Session. |
| `Up` / `Down`, `PageUp` / `PageDown`, `Home` / `End` in the conversation | Scroll precisely, scroll by a page, jump oldest, or return to latest. |
| `Ctrl+Home` / `Ctrl+End` in conversation | Jump to the oldest loaded cell or latest cell. |
| `Esc` | Close a nonblocking overlay, or request cancellation while a Turn runs. |
| `Ctrl+Q` | Open the quit confirmation. |

Picker and palette overlays accept a text filter, Backspace, Up/Down, Enter,
and Escape. Reaching the end of the Session picker requests the next bounded
Host page. Session filtering matches the displayed Agent definition label or
the opaque Session ID. Long result sets scroll with the selection, so the row
that Enter will open remains visible. Prompt history is local convenience state
and is not reconstructed from Host transcript content.
Unavailable commands remain discoverable and name a safe reason. The visual
palette, screen-reader prompt, and Enter activation consume the same condition,
so an unavailable action cannot appear enabled or fail silently.
When mouse capture is enabled, the wheel moves the selection while the pointer
is over a selectable overlay and a left click activates the visibly hit row.
Modal input never scrolls the conversation or opens a Session behind the
overlay.
Click inside composer text to place the cursor. Hold the left button and drag
to select complete displayed characters; releasing leaves the selection ready
for replacement or keyboard movement. Dragging just beyond the text viewport
extends to its nearest visible insertion point. Borders and padding are inert,
and switching terminal focus safely cancels an unfinished drag.
Double-click the same text cell to select its complete Unicode word or
punctuation run; double-clicking whitespace only places the cursor. A third
click selects the complete logical line, ready for replacement. The sequence
must stay on one cell within 500 ms and resets after typing, paste, resize,
focus loss, or a click outside the composer. Multi-click selection never copies
text automatically.
Use `Alt+C` to copy the active selection explicitly. The selection remains
visible and ready for replacement after the request. The equivalent
`/copy selection` action is available from `Ctrl+P` only while text is selected;
typing that command into the composer would replace or clear the prior
selection and therefore cannot act on it.

## Slash commands

The inline menu and `Ctrl+P` palette are generated from the same catalog used
for availability. The catalog lists every admitted theme and mouse variant;
an entry shown as unavailable cannot be completed or activated until its named
condition is true.

| Command | Result |
|---|---|
| `/new ["definition-id"]` | Create a Session with the selected definition. |
| `/sessions [filter]` | Open and optionally filter the Session picker. |
| `/status` | Show safe connection details. |
| `/retry` | Refresh Host truth and replay the exact persisted command identity. |
| `/reconnect` | Reload the selected Session and resume its event stream. |
| `/cancel` | Request cancellation of the active Turn. |
| `/theme system\|dark\|light\|mono` | Change the current theme. |
| `/mouse on\|off` | Save mouse capture for the next terminal session. |
| `/copy last` | Request an OSC 52 copy of the last visible Agent completion. |
| `/copy selection` | Request an OSC 52 copy of the active composer selection. |
| `/copy session-id` | Request an OSC 52 copy of the selected Session ID. |
| `/help` | Open the keyboard guide. |
| `/quit` | Open the quit confirmation. |

Command arguments may be quoted. Inside quotes, only `\"` and `\\` escapes
are accepted. An invalid command is not sent to the Host; a status overlay says
so and Escape returns to the workspace.

Copy requests are limited to 64 KiB and require terminal OSC 52 support. They
are disabled in screen-reader mode. The TUI cannot read the system clipboard.

## Sessions, background work, and notifications

Selecting a Session loads its verified Host snapshot and timeline, then follows
events after the observed watermark. Switching Sessions does not cancel a Turn.
Up to four background Sessions with running or action-required work may retain
event followers; the UI rings the terminal bell when a background Session
reaches a terminal state, if the saved bell preference is enabled.

The conversation follows latest by default. Scrolling upward establishes a
stable anchor. New cells increment the newer-update indicator instead of
moving the viewport. Jumping to the end resumes latest-follow behavior.

## Cancellation and suspension

`Esc` or `/cancel` sends a durable cancellation request for the active Turn.
The UI remains in the running/cancelling flow until the Host commits a stopped,
completed, or failed terminal. Network EOF and local process exit are never
treated as terminal truth.

When the Host reports a public suspension, Garive opens Action required. Press
Enter to move to the response composer. Text suspensions accept a normal
response; schema-backed suspensions validate the entered public value and send
canonical JSON. The continuation is correlated to the exact Session, Turn,
suspension ID, and expected Session version.

## Disconnect and recovery

For a nonterminal Turn, the TUI reconnects from the last accepted event cursor
with delays of 250 ms, 500 ms, 1 s, 2 s, and 4 s. After five failed attempts it
stays disconnected and offers `/reconnect`. Replayed identical events are safe;
gaps are legal; conflicting or malformed protocol values make the Session
unavailable and require a fresh snapshot.

Every mutation is written to a private pending record before the Host call.
Transport failure, deadline, process kill, or an invalid response retains that
record because the durable result is unknown. On restart, the recovery overlay
is titled **Command result unknown**, shows the complete safe explanation even
when it wraps across lines, and offers:

- Enter: reload Host truth and replay the same command ID and byte-equivalent
  request;
- `A`: abandon only the local recovery record, then reload the Session.

Abandonment does not cancel, roll back, or prove failure. Use it only when you
accept that the durable outcome may remain unknown.
The fullscreen card and screen-reader prompt announce the same two actions,
and those labels come from the same bindings that handle Enter and `A`.

## Local state and privacy

On Unix, state is stored under `$XDG_STATE_HOME/garive/tui`, or
`~/.local/state/garive/tui` when `XDG_STATE_HOME` is absent. On Windows it is
stored under `%LOCALAPPDATA%\Garive\tui`. Use an absolute `--state-dir` for an
operator or test override.

| Path | Stored content |
|---|---|
| `preferences.v1.json` | Theme, motion, mouse, selected Session, bounded drafts, and UI preferences. |
| `pending/*.v1.json` | Exact idempotent recovery envelopes for unknown/in-flight mutations. |
| `prompt-history.v1.jsonl` | Up to 500 submitted local prompt entries within 2 MiB. |
| `diagnostics/garive-tui.log*` | Rotating content-free operational events. |
| `quarantine/` | Invalid local files moved aside with opaque names. |

Unix directories are required to be owner-only `0700`; files are `0600`.
Windows directories and files use a protected DACL granting full control only
to the current process-token SID. Existing objects must have that owner and
exact ACL; reparse points in the private path are rejected. Unsafe permissions
make local state unavailable instead of weakening privacy. Writes use advisory
locks, same-directory temporary files, file flush, atomic replacement, and
directory sync or write-through metadata moves where the platform supports it.

The Runtime/Ledger remains the only durable owner of Sessions, Turns,
conversation, activity, suspension, and terminal outcomes. Diagnostics exclude
prompt/completion text, IDs, Host URLs, file paths, credentials, provider/model
values, raw responses, and terminal bytes.

## Screen-reader mode

Launch with `--screen-reader`, or use `TERM=dumb`. The client writes ordered
semantic lines such as connection changes, `You:`, `Garive:`, activity, and
overlay guidance. It never enters the alternate screen or emits cursor
addressing. The same command, Session, Turn, suspension, recovery, and quit
semantics apply.

Because output is append-only, a long-running screen-reader session relies on
the terminal's own scrollback. Clipboard escape requests and mouse capture are
disabled.
Filtered command, Session, and history prompts announce the selected numbered
row from the same bounded result window that Enter activates. Help also states
portable newline, keyboard-only, no-color, no-mouse, and OSC 52 fallback
behavior.

## Safe exit and terminal restoration

Use `Ctrl+Q`, `/quit`, or press `Ctrl+C` twice with an empty composer, then
confirm with Enter. A normal exit stops local follower tasks, persists allowed
presentation state, disables mouse/focus/paste modes, shows the cursor, leaves
the alternate screen, disables raw mode, and flushes stderr.

SIGINT and SIGTERM use the same restoration path and return `128 + signal` on
Unix. Garive also owns an idempotent drop guard so partial terminal setup and
panic unwinding restore every mode already acquired.

Exit codes are:

- `0`: confirmed clean exit;
- `1`: Host/product/local-state failure after launch;
- `2`: argument, URL, TTY, or terminal setup error;
- `128 + signal`: restored signal exit on Unix.

## Troubleshooting

**`an interactive terminal is required`**: run directly in a terminal with
interactive stdin and stderr. Redirected or piped full-screen execution is
rejected; use screen-reader mode only from an interactive terminal.

**`invalid arguments; use --help`**: check the loopback URL, enum spelling,
absolute state path, duplicate options, and unexpected positional arguments.
Supplied values are deliberately omitted from the error.

**`local state is unavailable or unsafe`**: on Unix, verify ownership and
`0700`/`0600` permissions. On Windows, verify that the path is not a symlink or
junction and that its protected ACL grants only the current account full
control. Remove no files while another process holds the state locks, and
inspect only the content-free diagnostics. Corrupt records are quarantined
automatically when safe atomic rename succeeds.

**The Turn appears stuck after disconnect**: this is intentional uncertainty,
not inferred failure. Wait for bounded automatic reconnect or run `/reconnect`.
If a pending recovery overlay is present, use exact retry after reading the
recovery guidance.

**Mouse is unchanged after `/mouse`**: the command updates the preference for
the next terminal session so the current terminal's capture lifecycle remains
coherent.

## Verification for maintainers

Run the focused product gates from the repository root:

```text
just tui-unit
just tui-snapshots
just tui-persistence
just tui-contract
just tui-runtime-e2e
just tui-pty
just tui-bench
just tui-boundaries
```

`tui-runtime-e2e` launches the shipping binary in a real PTY against the
production HTTP Host and file-backed SQLite Runtime. `tui-pty` checks real
terminal input, resize, screen-reader output, signal exit, and restoration.
`tui-bench` runs both the debug smoke gate and the three-run optimized candidate
baseline.

The normative design set begins at
[`../../spec/design/tui-product-spec-set.md`](../../spec/design/tui-product-spec-set.md).
The pinned competitive source audit is
[`../tui-source-audit.md`](../tui-source-audit.md).
