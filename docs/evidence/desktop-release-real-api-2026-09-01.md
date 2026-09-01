# Desktop release real-API evidence — 2026-09-01

Status: verified against the local release binaries and a fresh schema-v5
configuration. This is local ad-hoc evidence, not notarized release admission.

## Proven path

```text
release garive-host
  -> shared Desktop Workspace governance composition
  -> loopback H1/SSE at 127.0.0.1:8787
  -> durable SQLite Runtime and Worker
  -> token9 at 127.0.0.1:9527
  -> deepseek-v4-pro
```

The credential supplied to `configure` and `serve-stdin` was the non-secret
`token9-loopback` placeholder. token9 retained the upstream credential. No
upstream secret was placed in argv, output, a tracked file or React state.

## Defect found and fixed

Fresh setup correctly wrote schema v5 and installed `garive-work` at
`desktop.agent.v3`, including its governed Workspace tool. The package Host
then returned `host_construction_failed`: it still called the ungoverned
`DesktopHost::new`, which correctly rejects every tool-bearing Agent snapshot.

The GUI already used a Workspace-governed factory. `DesktopHost` now owns one
`new_with_workspaces` composition used by both the GUI installation path and
the loopback Host binary. The shared path installs authority, safety, sandbox
and executor ports and carries optional T1 machine configuration. A regression
test proves that the same tool-bearing configuration is rejected ungoverned
and admitted through the shared governed composition.

## Real execution and durability

After rebuilding the release Host:

1. `serve-stdin` printed `Garive Host listening on http://127.0.0.1:8787`.
2. `/v1/agent-definitions` returned exactly one installed definition:
   `garive-work / desktop.agent.v3`.
3. The release CLI submitted an exact-response request through the Host.
4. The real model returned exactly
   `GARIVE_DESKTOP_RELEASE_REAL_OK_20260901`.
5. H1 timeline projected one completed Turn at durable position 14 with that
   exact completion.
6. The Host was stopped and restarted against the same SQLite database.
7. The same Session, position, completed state and exact marker were read back.

The full Tauri bundle was then rebuilt and the `garive-host` executable was run
from `Garive.app/Contents/MacOS`, not from Cargo's loose output. It continued the
same Session through the real model and returned exactly
`GARIVE_DESKTOP_PACKAGE_MULTITURN_OK_20260901`. The two completed Turns were
projected in order at durable position 27. A second package-Host restart read
the same two completions and position back from SQLite.

This proves the release Host is not a static UI fixture and that its configured
Desktop Agent can execute a real provider Turn and recover the committed result.
The already-built GUI process also remained resident instead of exiting during
launch. Fresh GUI interaction evidence is still pending because Computer Use
reported the macOS graphical session locked during this run.
