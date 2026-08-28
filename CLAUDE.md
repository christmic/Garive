# CLAUDE.md

> Claude Code entry point for the **Garive** repo (next-generation
> Agent project — Core Agent, multi-channel, gateway platform,
> multi-platform Apps). Other tools read `AGENTS.md`; both
> `@`-reference the same rules.

@AGENTS.md

## Working in This Repo

- **Single source of truth for wire types:** `spec/proto/*.proto`.
  Rust / Kotlin / Go bindings are generated. Do **not** hand-edit
  generated code in any tier — change the `.proto`, regenerate
  via `just codegen`.
- **Build orchestration:** every recipe lives in `Justfile`.
  Run `just --list` for the menu. Recipes wire each language's
  native toolchain (Cargo workspace, Gradle, pnpm, Go, buf).
  Workspace members are added as crates are populated — see
  the planned-list comment in the root `Cargo.toml`.
- **Conformance is evidence-driven:** `just conformance` is currently a
  truthful placeholder. When two implementations consume a shipped boundary,
  wire/canonical/semantic checks land with that slice; byte identity is only a
  gate for explicitly canonical encodings.
- **Per-tier AGENTS.md override the root:** `engine/AGENTS.md`,
  `runtime/AGENTS.md`, `spec/AGENTS.md` carry tier-specific rules
  and win over the root `AGENTS.md` where they disagree.
- **Spec vs docs:** `spec/` is normative (落地规范 + 共享 proto);
  `docs/` is deliberative (思考和设计). Don't move content
  between the two lightly.

## Tooling Quick Reference

| Task | Command |
|------|---------|
| List recipes | `just --list` |
| Regenerate proto bindings | `just codegen` |
| Cross-language checks (not wired yet) | `just conformance` |
| Build Rust workspace | `just build` |
| Run Rust tests | `just test` |
| Desktop placeholder | `just desktop` |
| Mobile placeholder | `just mobile` |
| Bench | `just bench` |
| Clean | `just clean` |

## Oraculo-side Memory

- Project status: `~/Oraculo/projects/Garive/PROJECT.md`
- Daily notes: `~/Oraculo/memory/today.md`
- Cross-project overview: `~/Oraculo/memory/projects.md`

When you change Garive in a way that affects project status
(new crates, new parts, deprecated sub-trees, etc.), update
those files.
