# Shared UX remaster evidence

> Captured 2026-08-31 from shared UI revision `2cdcef16`. These are design
> review fixtures, not notarized Desktop release-candidate evidence and not
> entries in the M01–M85 release matrix.

## Reference binding

- installed ChatGPT/Codex: `26.825.51511` (`7377`), bundle
  `com.openai.codex`
- installed `app.asar` SHA-256:
  `f56ac8d5254a10fc4a04e7417fa787d135c3bbca49bad7d668d4ae65833d40c7`
- official visual reference:
  <https://learn.chatgpt.com/docs/features>
- design study: `docs/desktop-web-codex-fidelity-study.md`
- normative contract: `spec/design/shared-client-visual-system.md`

## Captures

| Capture | Viewport | SHA-256 |
|---|---:|---|
| `shared-artifact-wide-light-2026-08-31.png` | 1440 × 900 | `6c1b85c8d6c495d727853c30ecce828336622efb0112d856d3584fbbe7ac7208` |
| `shared-artifact-wide-dark-2026-08-31.png` | 1440 × 900 | `47edc61f45ac36fc852fd14ded17714a793ef45029eb4da369b972552019092f` |
| `shared-artifact-720-dark-2026-08-31.png` | 720 × 900 | `a44081cf85c48359b3fb5b9b778c0fbd666fd65feb6bb5b4afdb8e262d00ad77` |

All three use the deterministic `visual-test=artifact` fixture. The fixture
contains no private account data and no uncommitted provider output. Desktop
and Web import the same `App`, token and style sources; these captures prove
the shared presentation only. Native macOS window chrome and a real Web H1
journey retain their separate admission gates.

## Review result

- Gate 1: neutral hierarchy, 252/360 shell geometry, 56 px title bar, readable
  metadata, compact composer and sentence-case navigation are present.
- Gate 2: explicit durable status, committed artifacts, revision, export
  authority and optional evidence panel remain visible without entering the
  primary reading column.
- 720 px: document width equals viewport width; the rail collapses to 72 px and
  the evidence panel becomes a 360 px overlay.

The captures do not yet prove 200% text zoom, native VoiceOver, real provider
execution, signing or notarization.
