# TUI visual system

> Normative visual and interaction-component contract for every Garive TUI
> screen. Product semantics remain in the interaction and Host Specs.

## Principles

1. Hierarchy precedes decoration: workspace, active task, and next action must be recognizable from shape and text before color is applied.
2. Durable truth is visually distinct from local intent and transient notice. One component owns one interaction pattern.
3. Screens compose components; they do not invent local colors, borders, key-hint syntax, or selection.
4. Every state survives monochrome, reduced motion, resize, and replay.

## Tokens

`Palette` is the only source of terminal styles. Components consume semantic
tokens: `normal`, `muted`, `accent`, `title`, `border`, `focus`, `selection`,
`surface`, `success`, `warning`, `danger`, `notice`, and `keycap`. Dark, light,
system, and mono map these tokens; renderers must not select raw colors.

Meaning requires text or glyph plus style. Mono selection and keycaps use
terminal reverse video. RGB backgrounds are enhancement only. Contrast must
not depend on the terminal default being light or dark.

## Primitive components

| Primitive | Contract |
|---|---|
| `StatusChip` | padded icon/text span; semantic state style; never color-only |
| `KeyHint` | visually distinct keycap plus verb; ordered by current action priority |
| `SelectionRow` | full-row highlight plus stable cursor/marker; reverse video in mono |
| `FocusFrame` | double composer border or accent region border; never moves focus on Host events |
| `CenteredColumn` | caps readable main content at 114 cells without changing model state |
| `ModalFrame` | dims retained workspace, clears popup bounds, rounded focus border, safe padding |

Implementations live in `tui/src/view/primitives.rs` and `style.rs`.
Higher-level renderers must reuse these primitives for equivalent behavior.

## Composite components

| Component | Required variants |
|---|---|
| Header | compact/full; connection chip; execution chip; safe identity |
| Session rail | empty/populated; selected; terminal/running/action/failed; overflow |
| Conversation | empty/live/scrolled/newer updates; user/Agent/activity/notice cells |
| Composer | idle/focused/frozen/action response; placeholder/draft/over-limit |
| Context footer | idle/running/notice/recovery; tiny/full width collapse |
| Picker/palette | empty/filtered/selected/disabled; keyboard-owned selection |
| Confirmation | safe consequence, primary action, escape action; no implicit acceptance |

Session and activity state glyphs are closed semantic vocabulary: `✓`
completed, `●` running, `!` action required, `×` failed, `■` stopped, and `○`
unknown/new. Unknown public codes use neutral wording and never borrow success.

## Layout and degradation

At 100 cells and above, the Session rail spans the workspace and the main
column owns conversation, composer, and footer. At 160 and above, the main
column is centered and capped. Below 100, navigation becomes an overlay.
Below 60, footer hints collapse to the highest-priority action and help.
Below `20x8`, render only the safe minimum-size message.

Degradation order is ambient context, secondary hints, decoration, then
nonessential metadata. Never remove the active action, semantic state,
composer cursor, recovery consequence, or selected-row marker to make space.

## Interaction consistency

Only the focused component owns editing/navigation input. Modal ownership
outranks page focus. Selection persists by stable model identity, not screen
row. `Enter` activates the visible primary action; `Esc` closes, defers, or
requests cancellation only as admitted by the current state. Footer hints are
derived from that same routing decision.

## Conformance

Every new component or variant requires: a semantic buffer test, dark/light/mono snapshot coverage at its responsive boundary, keyboard ownership tests,
and a real macOS PTY review when it changes terminal behavior. Reviews reject
raw colors outside the palette, duplicated key-hint formatting, color-only
state, content-dependent layout identity, and screenshots without executable
snapshot or PTY evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
