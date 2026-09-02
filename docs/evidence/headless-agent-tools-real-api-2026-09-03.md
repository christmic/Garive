# Headless Agent tools — real API evidence

Date: 2026-09-03  
Runtime: `garive-headless` on `127.0.0.1:18790`  
Provider profile: `anthropic.messages.v1`  
Model target: `deepseek-v4-flash` through the configured loopback deployment

## Claim boundary

This run used the compiled Runtime, H1 HTTP API, real configured model service,
SQLite ledger, governed effect path and real filesystem. It was not a mocked
model test.

With an explicit workspace root, the effective Agent snapshot exposes:

```text
garive.workspace.read_text@1
garive.workspace.list@1
garive.workspace.search_text@1
garive.workspace.write_text@1
garive.workspace.apply_patch@1
```

`write_text` creates only and cannot overwrite. `apply_patch` changes existing
files only after binding every target to the `content_digest` observed by
`read_text`. `garive.process.run` is not exposed by this headless composition,
because no explicit isolated process lane was supplied.

## Launch shape

```bash
garive-headless /tmp/garive-tools-real.qrq3Vs \
  127.0.0.1:18790 /tmp/garive-tools-workspace.iIbhG7
```

The third argument is mandatory for tool mode. It is the exact workspace
capability; Runtime does not infer a current directory. Patch recovery uses a
private `0700` directory below the config directory.

## Read, create, and edit in one real Turn

The initial file contained `ORIGINAL_ALPHA` plus a newline. The submitted user
input asked the model to read it, create `notes/result.txt` from the observed
value, and patch the source to `EDITED_BETA`.

The first run proved real read and create effects:

- `read_text` observation returned 15 bytes and content digest
  `e6fa8a2f814012adba39a738b76a5ae405560540f140517e702dc105d8a9a381`;
- `write_text` created `notes/result.txt` with exact content
  `READ=ORIGINAL_ALPHA\n`;
- the ledger committed `effect.prepared`, Safety/authority/sandbox facts,
  `effect.receipt`, `effect.completed`, and `effect.observation` for both.

That first run then exposed a real compatibility defect: the model generated a
normal unified diff, while T1 revision 1 accepted only the internal Garive
patch spelling. The preparation failure surfaced as `port_failure`. The tool
contract was corrected to accept safe existing-file unified diffs while
retaining exact target/digest checks.

The rerun used:

```text
Session  session-7373a8b613d7e01efec5c448740c4d36732c4b69dab3cd09caff386166de7d55
Turn     turn-73b71441f4bae71ceaa7b45c66922d5b409fa5cacb3bfda01293f93be9f867bc
```

Its durable sequence was:

```text
model.completed -> garive.workspace.read_text intent
effect.prepared -> effect.receipt -> effect.completed -> effect.observation
model.completed -> garive.workspace.apply_patch intent
effect.prepared -> effect.receipt -> effect.completed -> effect.observation
model.completed -> execution.completed -> turn.completed
```

The patch observation bound:

```text
before e6fa8a2f814012adba39a738b76a5ae405560540f140517e702dc105d8a9a381
after  01e65933201edf324aedc267999fedc4b119c221c6bb7feb89a8837728c8126a
```

The SSE terminal was `turn.completed`, and the actual file read after the Turn
was `EDITED_BETA\n`.

## List and search in one real Turn

Session
`session-159c5108047da7f9a1a53d02d55195d9727da9b81cbc12f11b69a604d9e7ec1e`
ran Turn
`turn-e585b07e8b77c345cf32ca274a61330df4f3c07e95e6a4c544d2312f7dc07978`.
The ledger records a successful root `list` observation containing
`handoff.txt`, `notes/`, and `source.txt`, followed by a successful
case-sensitive `search_text` observation. Search scanned three files and
returned `handoff.txt`, line 1, column 14, preview
`FROM_AGENT_A=BLUE_CRAB`. SSE committed `turn.completed` at position 32.

## Protocol compatibility found by the real run

Garive's neutral dotted tool identities do not satisfy the portable provider
wire-name grammar. The Provider layer now maps an unsafe neutral name to a
bounded deterministic `garive_<sha256-prefix>` name and restores the exact
neutral identity on normalized output. Provider-specific names do not enter
Core or the ledger.

A second defect was also found: a later provider request contained a tool
result but omitted the assistant tool call it answered. Durable context now
retains neutral `ToolIntent` history. The compatible mapper emits correlated
Responses `function_call/function_call_output` or Messages
`tool_use/tool_result` pairs.

## Multi-Agent, multi-Session shared-workspace run

H1 created two distinct Agent instances from the same installed definition:

```text
Agent A  agent-c387a326d79aedbea5b09ebd3192efcdf4a7b26cbe3208363244705466dbe2ab
Session  session-22a74a06bc434697f90314dca9e3a2736f6ffe36a2fb19c121e7de53e0be4345

Agent B  agent-945ef71ed614413495ac2aeb644a1448fb250911eecf7cfef2d7b1ae7435b797
Session  session-51b86be1c7a390c304ba744e916beadf0a07bdafdf1f3afab246ccd18fc989be
```

Agent A used `write_text` to create `handoff.txt` with exact bytes
`FROM_AGENT_A=BLUE_CRAB\n`. Its Turn completed at position 23 with digest
`a86519fda1a2e36b4a152dd4bc5651ca666d2ae952b5a31cd01599af144eb302`.
Agent B then used `read_text` from its own Session and completed at position 23
with exactly:

```text
AGENT_B_ACK_BLUE_CRAB
```

This proves separate Agent instances, separate durable Sessions and governed
shared-workspace interoperability. It does not claim MA0 parent/child
delegation: MA0's intent, budget, ledger planning and reconstruction exist,
but H1 still has no parent-to-child orchestration command. That is a distinct
remaining composition slice and must not be represented by this evidence.

## Automated gates attached to the behavior

The implementation has focused tests for:

- all six T1 definitions and shared Prepared-v3 fixture;
- create-only write, overwrite refusal and symlink refusal;
- unified-diff parsing, rename/create refusal and old Garive grammar;
- workspace-only composition excluding process execution;
- Responses and Messages tool-call history correlation;
- deterministic provider wire-name mapping and neutral-name restoration;
- durable second-iteration context containing both tool intent and result.
