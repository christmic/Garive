# Engineering Rules

## Truth > Speed

- Never claim completion without verification evidence.
- Self-verify before declaring done — run build/test/lint and read the
  output to confirm a real PASS.
- Small batch: ≤15 files or ≤400 lines net changes per commit.
- No secrets: never commit API keys, tokens, credentials, or private
  endpoints.
- Reversible: every change must have a clear rollback path.

## Evidence Before Implementation

- Protocol and API implementations must be derived from verified sources
  (downstream SDK source, official API documentation, captured wire
  traces). Record the source, version, and inspected paths in the
  relevant design doc.
- Do not invent wire fields, event names, error shapes, or token
  semantics from intuition or "compatible" third-party providers.
- Mock fixtures must use official response shapes. A fixture invented
  only to fit an implementation is not evidence.
- Toolchains, SDKs, and dependencies follow
  `.agents/dependency-versions.md`; newest-stable is an evidence-backed
  compatibility decision, not a dynamic version range.

## Verification Gates

- Formatting, lint, and tests must pass before completion is claimed.
- A source scan for known anti-patterns (e.g., local imports outside
  the module section, undocumented `unsafe`, banned phrases) must be
  clean.
- Real-provider / network-dependent tests stay ignored unless
  credentials and endpoints are supplied explicitly. Code must never
  read environment variables for endpoint or authentication discovery
  in test paths.

## Banned Phrases

Do not use these in commit messages, code comments, or progress notes:

- "I fixed it, you try"
- "Should be fine"
- "Probably passes"
- "Theoretically correct"
- "I think it's fixed"

If you catch yourself about to write one, stop and verify instead.
