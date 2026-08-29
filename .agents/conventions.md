# Conventions

## Language

**Default: English** for all technical writing — code, comments,
commit messages, documentation, variable names, tool descriptions,
commit messages, log strings, error messages.

When updating existing content, follow the document's existing
language.

User-facing strings that ship to a non-engineering audience may be
localised, but the **keys themselves** stay English.

## Per-language Naming

### Rust (`engine/`, `runtime/replica/`, `cli/`, `tui/`, `bench/`,
`desktop/` backend)

| Element | Convention |
|---------|------------|
| Crates | `garive-<kebab-slug>` (e.g. `garive-core`, `garive-llm`). |
| Modules | `snake_case`. |
| Types / traits / enums | `PascalCase`. |
| Functions / methods / variables | `snake_case`. |
| Constants / statics | `SCREAMING_SNAKE_CASE`. |
| Error variants | `PascalCase`; thiserror enums end in `Error` (e.g. `AgentLoopError`). |
| Feature flags | `kebab-case` (`agent-loop`, `multi-agent`). |

### Kotlin (`experiments/engine-kt/`, `mobile/`)

| Element | Convention |
|---------|------------|
| Packages | `lowercase.dotted` (e.g. `com.garive.core`). |
| Classes / objects / enums | `PascalCase`. |
| Functions / properties / locals | `camelCase`. |
| Constants | `SCREAMING_SNAKE_CASE` in `companion object`. |
| Sealed classes | `<Domain>State`, `<Domain>Event`. |

### TypeScript / React (`desktop/` frontend)

| Element | Convention |
|---------|------------|
| Files | `kebab-case.ts` / `kebab-case.tsx`. |
| Components | `PascalCase` (`.tsx` filename matches component name). |
| Functions / variables | `camelCase`. |
| Types / interfaces | `PascalCase`. |
| Constants | `UPPER_SNAKE_CASE` for module-level. |
| Hooks | `use*` prefix. |

### Go (`runtime/gateway/`)

Standard Go style — `gofmt` + `go vet`. Short receiver names
(1–2 chars). Package names are short, lowercase, single-word.

### Swift (`desktop/` macOS, `mobile/` iOS)

| Element | Convention |
|---------|------------|
| Types / protocols | `PascalCase`. |
| Functions / properties / variables | `camelCase`. |
| Constants | `lowerCamelCase` for instance; `UPPER_SNAKE_CASE` for static. |
| Files | one top-level type per file, filename matches type. |

## Comments

- **Why, not what.** Comments explain intent, invariants,
  non-obvious trade-offs. They do not narrate what the next line
  does.
- **No editor commentary** (`// TODO later`, `// I know this is
  ugly`) — open a TODO issue instead and reference its ID.
- **Module / file-level docs:** every non-trivial module carries
  a top-of-file doc comment that names the module's role and
  any non-obvious constraints.
- **Public API docs:** every `pub` (Rust) / `public` / `export`
  symbol carries a doc comment. Undocumented public API is not
  merge-ready.

## Errors

- Error messages are **specific and actionable**. "I/O failed" is
  not acceptable; "failed to read `~/.config/garive/agent.toml`:
  permission denied" is.
- Never swallow an error silently. Either propagate, log with
  context, or convert to a domain error.
- User-facing errors carry an error code (e.g. `E_AGENT_LOOP`)
  suitable for stable documentation and i18n.

## Log Strings

- English, one short sentence, present tense.
- Include the **subject** (what is being logged) and a **value**
  (the relevant datum). Example: `loaded 42 fixtures from
  spec/fixtures/`, not `load complete`.
- No emoji in log strings. No exclamation marks.

## Commit Messages

- English, verb-first, ≤50 chars, no trailing punctuation.
- Subject line alone. Body optional; if used, wrap at 72 chars
  and explain **why**, not what.
- Refer to the spec or design doc by relative path when relevant
  (e.g. `engine: add loop driver (see spec/proto/garive/v1)`).

## Writing Style

- Concrete, not abstract. "Failed to parse JSON in fixture `ping.json`"
  beats "Parse error".
- No filler ("obviously", "simply", "just", "of course").
- Banned phrases — see `.agents/engineering-rules.md`. The list
  applies to all prose in the repo, including PR descriptions
  and progress notes.

## File Operations (Hard Rule)

**Applies only within git repositories.**

| Operation | Method |
|-----------|--------|
| Move `.md` / `.txt` files | `mv` |
| Move other files | `mv` first, then delete-and-rewrite using the built-in Read/Write tools |
| Rename files | delete first, then Write/Edit to create the new file (not `git mv`) |
| Bulk content changes | built-in Edit/Write tools |
| Shell scripts | ask user for confirmation before executing |

**Write = Permission Granted** when:

- The file has been read in this session, **or**
- The user provided the content explicitly, **or**
- The file is being written new (never existed).

Read the file before writing it. Other directories are forbidden by
default unless explicitly referenced.

## Security

- No API keys, tokens, credentials, or private endpoints in any
  tracked file. Shipping code receives configuration explicitly from
  the Runtime and secrets through an injected OS credential resolver.
  A git-ignored local file may support developer tooling, but Engine,
  protocol adapter, Provider mapping, and SDK modules must not read
  process environment variables for configuration discovery.
- No customer data, internal hostnames, or proprietary URLs in
  examples unless they are obviously placeholder values.
