# Shared client visual and interaction system

> Normative for the canonical React UI mounted by Desktop and Web. Native
> mobile clients preserve the semantic contract while using platform-native
> components. Values not backed by Host/account facts are absent, not guessed.

## Purpose

This contract prevents feature teams from inventing local colors, spacing,
status copy or interaction behavior. New surfaces compose the tokens and
patterns below before adding a variant. A mode may change its work canvas; the
application shell, task condition, authority, capacity and evidence patterns
remain shared.

## Principles

1. Outcome before mechanism: lead with the user's deliverable and next action.
2. Truth before reassurance: distinguish committed, pending, unknown and
   unavailable states.
3. One meaning per semantic color: cobalt is interactive/current, green is
   verified/success, amber needs attention, red failed/destructive.
4. Text always accompanies state color and iconography.
5. Progressive disclosure: ordinary work stays quiet; evidence and technical
   identities remain reachable without occupying the primary canvas.
6. Same concept, same component: Desktop and Web share markup, tokens, copy and
   keyboard behavior. Platform integration changes adapters, not semantics.

## Token contract

CSS custom properties are the executable token source. Components consume only
semantic tokens; raw values are permitted solely in the token declaration and
bounded media-query fallbacks.

### Typography

| Token | Value | Use |
|---|---|---|
| `--font-sans` | system UI stack | all product text |
| `--font-mono` | system monospace stack | IDs, versions, code |
| `--text-2xs` | 10 px | bounded metadata only |
| `--text-xs` | 11 px | labels and timestamps |
| `--text-sm` | 12 px | secondary copy |
| `--text-md` | 14 px | body and controls |
| `--text-lg` | 16 px | section title |
| `--text-xl` | 22 px | page title |
| `--text-display` | clamp(28 px, 3 vw, 38 px) | empty-state outcome |

No shipping text is below 10 px. At 200% text zoom, content reflows and no
essential label is clipped or replaced by an icon.

### Space, shape, depth and motion

| Family | Tokens |
|---|---|
| space | `--space-1` 4, `--space-2` 8, `--space-3` 12, `--space-4` 16, `--space-5` 24, `--space-6` 32 px |
| radius | `--radius-control` 8, `--radius-card` 12, `--radius-panel` 16, `--radius-composer` 18, `--radius-pill` 999 px |
| depth | `--shadow-raised`, `--shadow-overlay`; no other shadow families |
| motion | `--motion-fast` 120 ms, `--motion-base` 180 ms, standard enter/exit easing |

Desktop controls are at least 32 px high; primary actions and touch-capable
surfaces provide 44×44 px targets. Reduced-motion makes nonessential animation
instant. Reduced-transparency removes backdrop blur without losing separation.

### Color and surface aliases

Required tokens are `--surface-canvas`, `--surface-raised`, `--surface-sidebar`,
`--surface-subtle`, `--border-subtle`, `--border-strong`, `--text-primary`,
`--text-secondary`, `--text-tertiary`, `--action-primary`,
`--action-primary-hover`, `--state-info`, `--state-success`,
`--state-attention`, `--state-danger`, and matching state surfaces.

Light, dark, increased-contrast and forced-color modes provide all aliases.
Components do not test theme names or encode their own dark palette.

## Layout contract

The wide shell contains a 248 px rail, fluid work canvas and optional 390 px
evidence inspector. At narrower widths the inspector becomes an overlay, then
the rail becomes a navigation sheet. The primary canvas has a 760 px readable
content measure but artifact desks may use the remaining width.

The persistent hierarchy is rail → top bar → work canvas → composer/action
area. Blocking approval appears immediately above the action area. Connection
and capacity warnings never cover a blocking approval.

## Component grammar

| Component | Required states | Invariants |
|---|---|---|
| Button | default, hover, focus, pressed, disabled, busy | one primary action per decision region; icon-only requires accessible name |
| Field | empty, filled, focus, invalid, disabled | label persists; placeholder is never the label |
| Badge | neutral, info, success, attention, danger | short noun/adjective plus semantic color; never action-shaped |
| Task row | ready, active, needs-input, failed, completed, selected | same status copy and order in rail/search/command center |
| Notice | info, success, attention, error | states consequence and next action; alert only when interruption is required |
| Modal | entering, active, exiting | traps focus, Escape closes when safe, returns focus to invoker |
| Artifact card | preparing, ready, unverified, verified, unavailable | name/type/revision/verification visible; authority actions explicit |
| Capacity | unavailable, normal, watch, critical, exhausted | scope, period, attribution, remaining/reset and continuation policy travel together |

## Capacity view contract

`UsageBudgetView` is a presentation value, not a billing calculation:

```text
UsageBudgetView {
  source: included_plan | workspace_credits | provider_api | execution
  state: normal | watch | critical | exhausted
  scope_label
  period_label
  remaining_percent?       // integer 0..100, trusted source only
  resets_at_label?
  attribution: reported | estimated
  model_posture_label?     // qualitative unless price facts are supplied
  active_turn_may_finish
  detail_destination?      // admitted internal route or safe external URL
}
```

An unavailable value is represented by absence of `UsageBudgetView`; the app
does not render a zero balance. Estimated values say “Estimated” next to the
number. A meter has an accessible name and text equivalent. `watch`, `critical`
and `exhausted` come from the trusted source; clients do not infer billing
policy from arbitrary local thresholds.

Capacity is independent of durable execution state. Reaching a plan limit does
not mark a running task failed, stopped or cancelled. If
`active_turn_may_finish` is true, the notice explicitly says current work may
finish and applies restrictions only to starting subsequent work. Recovery
actions can include switching to an efficient model, using credits, opening
usage details, or selecting provider API billing, but only when the backing
product admits those actions.

## Interaction rules

- `Command-K` opens the global command center; `Command-F` opens durable work
  search; `Command-,` opens Settings.
- Enter activates the focused action; Space toggles controls; arrow keys move
  within menus, tabs and segmented controls according to platform convention.
- Background events do not steal focus. Required approval focuses its first
  safe action; destructive confirmation never receives default focus.
- Optimistic styling cannot imply durable success. Busy disables duplicate
  submission while keeping cancellation/reconnection actions reachable.
- Empty, loading, error, offline, partial and permission-denied states use the
  same component geometry to prevent layout jumps.

## Content rules

Use sentence case and verbs for actions. “Usage”, “capacity”, “credits”,
“tokens” and “task budget” are not synonyms. Never display “unlimited” unless
the trusted source also names fair-use and task-budget boundaries. Never claim
an exact task price from prompt length, selected model, or prior averages.

## Acceptance

- Token declarations are the only source of palette, type scale, canonical
  radii, depth and motion values for new shared components.
- Desktop and Web render the same Capacity component from the same view value.
- Unit tests cover normal, critical, exhausted and absent capacity plus
  accessible meter text.
- Visual evidence covers light/dark at 1440 px and a 720 px narrow window.
- Keyboard-only, reduced-motion, increased-contrast and 200% text matrices stay
  usable. Web captures prove shared presentation only; native macOS integration
  retains its separate evidence matrix.
