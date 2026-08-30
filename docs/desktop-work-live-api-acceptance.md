# Desktop/Web Work live-API acceptance

Status: verified on 2026-08-31 through the shipping loopback composition.

## Proven path

```text
Garive CLI or Web FetchHostClient
  -> loopback H1/SSE garive-host
  -> durable SQLite Runtime and local Worker
  -> token9 loopback gateway
  -> configured DeepSeek Messages provider
```

No upstream credential was read into React, a command argument, this document,
or captured output. `garive-host serve-stdin` received only a non-secret local
gateway placeholder; token9 retained responsibility for upstream credentials.

## Reviewed acceptance checklist

The first Flash-model draft invented account, semantic-memory, synchronization,
SLA and quota-file claims and was rejected. The following constrained result
was produced by `deepseek-v4-pro`, reviewed against the admitted product
contracts, committed as one durable Turn, and read back after a Host restart.

| Scenario | Operation | Observable pass condition |
| --- | --- | --- |
| Multi-turn durability | Submit multiple Turns in one durable Session | H1/SSE timeline retains every Turn and remains readable after restart |
| Disconnect recovery | Disconnect after a committed Turn, then reconnect | Follow resumes from the last committed cursor without duplicating or losing acknowledged events |
| Approval suspension | Resolve the exact `suspension_id` | Continuation binds the exact `session_version` and resumes only that Session |
| Cross-client isomorphism | Open the same durable Session in Desktop and Web | The shared React/controller surface reads the same ordered Host timeline |
| Capacity truth | Open Capacity without admitted remaining-capacity facts | No remaining balance or percentage is rendered |
| Accessibility | Open Command-K and navigate with the keyboard | Focus remains contained, commands are labelled, and the surface can be operated without a pointer |

## Verification evidence

- CLI exact completion through `deepseek-v4-flash`:
  `GARIVE_RUNTIME_LIVE_OK_20260831`.
- Web transport exact completion through `deepseek-v4-flash`:
  `GARIVE_WEB_LIVE_OK_20260831`.
- Post-rebase Web transport exact completion through `deepseek-v4-pro`:
  `GARIVE_WEB_PRO_LIVE_OK_20260831`.
- The reviewed Pro checklist remained `completed` at durable position 10 after
  stopping and restarting `garive-host` against the same SQLite database.
- token9 usage and rate-limit projections are evidence inputs, not a balance:
  the rate-limit projection was empty, so the UI made no capacity claim.

Native macOS screenshot, menu, picker and VoiceOver evidence remains a separate
gate. Computer Use confirmed the graphical session was locked; unlocking is not
required for headless Runtime or Web transport execution.
