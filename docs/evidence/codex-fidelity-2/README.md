# Codex fidelity batch 2 — visual evidence

Captured 2026-09-01 from the shared shipping React tree in a real browser at
1280 × 800 CSS pixels. The supplied Codex references are also 1280 × 800:

- Reference 1 SHA-256:
  `5df11bc57aa1c8c9ac8fbb04ed264618854e5d90a5403d3b0022c81b4daa45d6`
- Reference 2 SHA-256:
  `acdb4fcf68cb911381c8c251b6f07abc9c12d5b7184fc521cc6d55a390cc0e65`

## Admitted captures

| Capture | State | SHA-256 |
| --- | --- | --- |
| `desktop-running-environment.png` | Desktop running Turn, Environment open | `75db162e1860b831c3de4af3b602c8ea5711e85b00caed320b6051e2a4985ce1` |
| `desktop-artifact.png` | Desktop completed Turn, verified Markdown Artifact open | `b672597978d3387fb809fa6dad2e24138b85d70dfe412222ecc2cda0a53f0185` |
| `web-running.png` | Web running Turn using the same work surface | `ddf549098ccc9977e30e088dd62d3f1ddde22eaf9931b1bb1f27c47d0b8e4a98` |

## Geometry audit

- Sidebar boundary: reference ≈ 205 px; admitted capture 205 px.
- Artifact split: reference ≈ 556 px; admitted capture 558 px.
- Artifact document leading edge: reference ≈ 580 px; admitted capture 583 px.
- Environment: 224 px floating panel at 12 px from the right edge; the work
  surface reserves 236 px so conversation and composer do not sit underneath.
- Composer: bounded to 560 px and uses one 36 px progressive rail. Detailed
  admitted Activity remains available in Environment and in accessibility
  semantics.
- macOS reserves 58 px ahead of sidebar controls for the native traffic lights;
  Web intentionally does not reserve this desktop-only zone.

These captures prove presentation and geometry only. Real provider streaming,
durable terminal convergence, memory selection, and restart behavior remain
covered by their Runtime/Desktop integration and explicit token9 acceptance
tests; visual fixtures do not stand in for those claims.
