# `garive-headless` end-to-end retrospective

Date: 2026-09-02

Verification session against the running `garive-headless` binary
(`/tmp/garive-runtime` → `127.0.0.1:8787`) and the local token9 gateway
(`127.0.0.1:9527`). Captures the actual mistakes made during the live
verification so the next agent does not repeat them.

## What actually worked

The runtime slice delivers what was promised:

- `GET /v1/management/health` returns `{configured:true, configuration_revision:N}`
- `GET /v1/management/setup` returns the row with **no `api_key` field** —
  the redaction contract holds
- `POST /v1/management/setup` (via `garive-host setup-management`) bumps
  `configuration_revision` monotonically and writes a fresh
  `configuration_digest`
- `POST /v1/sessions` returns `{session_id, agent_instance_id, committed_position:1}`
- `GET /v1/sessions/:id` returns `definition_revision:"headless.agent.v1"`
  — proves the headless catalogue is mapping `desktop.agent.v3` correctly
- `POST /v1/sessions/:id/turns` commits the turn and the dispatcher
  invokes the configured model port
- `GET /v1/sessions/:id/events?after_position=N` streams SSE with
  `session.created` → `turn.started` → (`turn.failed` or `turn.completed`)
- Sessions persist in the SQLite ledger across binary restarts; a second
  `POST /v1/turns` against the same `session_id` after a config switch
  succeeds

End-to-end final success on the corrected config: a turn dispatched to
`http://127.0.0.1:9527/v1/messages` returned the model's literal text
`SMOKE_OK` and the event stream recorded `turn.completed`.

## Mistakes made during verification

### 1. Invented model id `claude-haiku-4-5`

The first `setup-management` commit filled in `claude-haiku-4-5` because
it "sounded plausible". Token9 does not proxy any Anthropic models; its
catalogue is `MiniMax-M3`, `deepseek-v4-flash`, `deepseek-v4-pro` only.
The configured row was useless until the model id was replaced.

**Rule:** before committing any `model_id` / `model_target_id`, hit
`GET /admin/models` on the target gateway and pick from its list.

### 2. Picked `openai.responses.v1` profile against an Anthropic-dialect gateway

Token9's `/admin/providers` reports upstream `base_url` of
`https://api.deepseek.com/anthropic` and `https://api.minimaxi.com/anthropic`
with `dialect:"anthropic"`. The first probe against `/v1/responses` returned
404. Only after switching to `anthropic.messages.v1` + `/v1/messages` did
the round-trip succeed.

**Rule:** before picking a `profile_id`, hit `/admin/providers` (or
equivalent) on the gateway and confirm the upstream dialect. If the
provider table is empty / inaccessible, do not commit — pick a different
gateway or stand up a stub first.

### 3. First probe was the wrong path shape

The token9 gateway does not implement OpenAI `/v1/chat/completions`
either. Only `/v1/messages` (Anthropic Messages wire) and the `/admin/*`
endpoints return real responses. Probing `/v1/models` returns the JSON
error body `{"error":{"message":"request body is not valid JSON"}}` even on
GET because token9 is mis-routing into a POST-only handler.

**Rule:** when an LLM gateway returns 200 with an "invalid JSON" error
body on a GET, it means the route exists but only accepts POST. Probe
`/admin/*` first for catalogue, then POST `/v1/messages` (Anthropic) or
`/v1/responses` (OpenAI) depending on the dialect discovered in (2).

### 4. Turn #1 failed silently because token9 returned 404, not because Garive was wrong

`POST /v1/turns` returned 200 with `turn_id` even when the downstream
dispatch failed. Only the SSE event stream revealed `turn.failed` at
position 10. Always read the SSE stream after a turn commit to know
whether the model actually responded.

## Working recipe (final, verified)

```bash
# 1. Discover what token9 actually exposes
curl -s http://127.0.0.1:9527/admin/models
curl -s http://127.0.0.1:9527/admin/providers
# -> model_ids: MiniMax-M3 / deepseek-v4-flash / deepseek-v4-pro
# -> dialect: anthropic

# 2. Probe that the chosen path accepts POST
curl -s -i -X POST http://127.0.0.1:9527/v1/messages \
  -H 'authorization: Bearer token9-loopback' \
  -H 'content-type: application/json' \
  -H 'anthropic-version: 2023-06-01' \
  -d '{"model":"deepseek-v4-flash","max_tokens":64,"messages":[{"role":"user","content":"Reply with exactly: SMOKE_OK and nothing else"}]}'
# -> 200, content has SMOKE_OK

# 3. Commit the row (the api_key here is whatever token9 expects —
#    in this environment it's the literal "token9-loopback")
echo "token9-loopback" | garive-host setup-management /tmp/garive-runtime \
    anthropic.messages.v1 http://127.0.0.1:9527/v1/messages \
    deepseek-v4-flash deepseek-v4-flash \
    desktop.agent.v3 tok9-flash runtime-token9-$(date +%s)

# 4. Restart the binary (restart_required: true is the contract)
pkill -f 'garive-headless.*8787'
/path/to/garive-headless /tmp/garive-runtime 127.0.0.1:8787 > /tmp/garive-runtime.log 2>&1 &

# 5. Drive H1
SESSION=$(curl -s -X POST http://127.0.0.1:8787/v1/sessions \
  -H 'Idempotency-Key: r1' -H 'Content-Type: application/json' \
  -d '{"agent_definition_id":"desktop.agent.v3"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['session_id'])")
curl -s -X POST "http://127.0.0.1:8787/v1/sessions/$SESSION/turns" \
  -H 'Idempotency-Key: r2' -H 'Content-Type: application/json' \
  -d '{"text":"Reply with exactly: SMOKE_OK"}'

# 6. Verify the model actually responded (do not trust the turn_id alone)
curl -s "http://127.0.0.1:8787/v1/sessions/$SESSION/events?after_position=0" \
  | grep '"event":"turn.completed"' | grep SMOKE_OK
```

## Concretely rejected paths

These do not work against the current `127.0.0.1:9527` token9:

| Path | Method | Result | Why |
|---|---|---|---|
| `/v1/responses` | POST | 404 | token9 doesn't speak OpenAI Responses |
| `/v1/chat/completions` | POST | 404 | token9 doesn't speak OpenAI Chat |
| `/v1/models` | GET | "invalid JSON" | mis-routed into a POST handler |
| `https://api.anthropic.com/v1/messages` | POST | unreachable | profile pointed outside the loopback gateway |
| `claude-haiku-4-5` as model_id | any | not in catalogue | token9 doesn't proxy Anthropic models |

## What the slice proves, regardless of token9 state

- Runtime reads the singleton SQLite row on startup (`read_with_credential`)
- `ManagementConfigStateWithCredential` never reaches H1 wire (verified
  via field-set diff against `/v1/management/setup`)
- Profile / endpoint / model / deployment all flow from the row into the
  model port construction
- Sessions persist in the ledger across binary restarts and config switches
- Turn commit returns 200 with `turn_id` even when dispatch fails —
  the SSE event stream is the source of truth for dispatch outcome
- The dispatcher loop (`drive_pending` + `DrivePendingOutcome`) actually
  pulls queued turns and invokes the model port