# Desktop summary and real-API evidence — 2026-09-01

Status: verified locally against installed Codex source, the shared Desktop/Web
client, Desktop Runtime tests and the token9 loopback model gateway.

## Source-led summary design

Installed Codex `26.825.51511`, build `7377`, composes its right thread summary
from keyed `Section`, `Item`, `ItemMeta`, `SectionCount` and `SectionActions`
primitives. Garive applies that contract only to facts admitted by its Runtime:
Runtime identity, attached Workspaces and committed Activity. It does not render
illustrative Git, PR or Source rows when those adapters are absent.

At 1280×720 the shared Web fixture measured a 300 px panel at `(968, 42)` with
a 14 px radius and `0 16px 32px -8px` shadow. Runtime and Activity were native
disclosure buttons; Activity exposed four admitted rows with trailing states.
Closing Activity, reloading and reopening Environment retained its collapsed
state. The same React component and stylesheet ship in Desktop and Web.

## Real model and memory ledger

The opt-in `durable_core_execution` acceptance ran through
`http://127.0.0.1:9527/v1/messages` using the non-secret `token9-loopback`
placeholder. token9 retained the upstream credential.

| Case | Admitted result |
| --- | --- |
| No memory | codename, deploy day and region were all `null`; no evidence ID |
| Current memory | `Amber Heron`, `Tuesday`, `Qingdao`, evidence `client-brief` |
| Current + superseded conflict | same current facts and evidence; stale revision rejected |

The test passed one real-model case in 34.75 seconds. Reported output tokens
were 372, 319 and 1835 respectively; token count is observation, not a quality
or capacity claim.

## Automated gates

- Desktop frontend: 39 files, 167 tests; production build passed.
- Web: 12 tests passed, one transport integration skipped by its normal gate;
  production build passed.
- Desktop Rust: 91 tests passed; `cargo check` passed.
- Real memory-ledger acceptance: one passed, five non-selected tests filtered.

The local DMG digest and bundle audit are recorded in the delivery summary once
the package build completes. This evidence does not claim Developer ID signing,
notarization, Universal architecture or public-release admission.
