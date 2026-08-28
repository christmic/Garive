# P1 — OpenAI Responses adapter

## Status

Accepted first protocol slice for Rust `adapters/llm-openai` and Kotlin
`runtime/server-kt/provider-openai`.

## Evidence coordinates

Reviewed 2026-08-29:

- official create reference:
  `https://developers.openai.com/api/reference/resources/responses/methods/create`;
- official streaming guide:
  `https://developers.openai.com/api/docs/guides/streaming-responses`;
- official `openai-python` commit
  `a1eeab58db02de46717ccebaf1eb83e314fa86ff`
  (`v3.0.0-1-ga1eeab58`);
- inspected SDK paths:
  `src/openai/types/responses/response_create_params.py`, `response.py`,
  `response_output_item.py`, `response_stream_event.py`, `response_usage.py`,
  `response_function_tool_call.py`, function-call argument delta/done event
  types, and `src/openai/types/shared/error_object.py`.

The official docs/SDK define wire truth. Sylvander is an implementation
reference only; any disagreement is resolved in favor of the coordinates above.

## Boundary

- endpoint: `POST /v1/responses`;
- authentication: Runtime supplies the secret bearer credential to the HTTP
  adapter; it never enters `ModelRequest`, logs or fixtures;
- request media type: JSON;
- streaming: request `stream=true`, response parsed as SSE semantic events;
- Chat Completions, Realtime, WebSocket mode, background responses and remote
  retrieval/cancellation are outside this first slice.

Base URL, organization/project routing and credential source are Runtime
configuration. Core cannot override them.

## Supported request subset

The adapter renders:

- required `model` from the resolved target;
- ordered `input` messages with `system`, `developer`, `user`, or `assistant`
  roles and text content;
- admitted image/media references only after Runtime resolves them to an
  official `input_image` representation;
- `instructions` only when the frozen request explicitly uses the dedicated
  instruction channel; it is not copied from an arbitrary message;
- optional non-zero `max_output_tokens`;
- function tools with `type=function`, name, description, parameters and
  strictness;
- `parallel_tool_calls`, text format and reasoning configuration when declared
  by the target capability snapshot;
- bounded metadata (official limit: at most 16 entries, key ≤64 characters,
  value ≤512 characters);
- `store=false` by default unless Runtime policy explicitly admits provider
  storage.

Unsupported `ModelInputItem`, tool kind or output mode fails before dispatch.
The adapter never drops it or changes it to text.

## Response items

The response `output` array is authoritative and order-sensitive. The adapter
supports:

- `message` content: `output_text` and `refusal`;
- `reasoning`: visible summary/text and opaque/encrypted content references;
- `function_call`: `call_id`, name and the complete JSON argument string.

Unknown/built-in output item kinds are preserved as bounded sanitized audit
evidence and return `UnsupportedCapability` unless the target snapshot admitted
that exact kind. The adapter does not execute OpenAI built-in tools on behalf of
Core.

## Usage

Official `ResponseUsage` fields map as follows:

| OpenAI | Garive |
|---|---|
| `input_tokens` | known input tokens |
| `output_tokens` | known output tokens |
| `input_tokens_details.cached_tokens` | cache-read breakdown |
| `input_tokens_details.cache_write_tokens` | cache-write breakdown |
| `output_tokens_details.reasoning_tokens` | provider detail/audit, already included in output |
| `total_tokens` | consistency evidence, not recomputed billing truth |

Negative values, overflow, or `total_tokens != input_tokens + output_tokens`
produce an adapter invariant failure with sanitized evidence. Cache/reasoning
breakdowns are not added again to totals.

## SSE sequence

Every decoded event has a non-negative `sequence_number`. Sequence numbers must
strictly increase. The admitted lifecycle is:

```text
response.created
  -> response.in_progress / response.queued (optional lifecycle facts)
  -> output item/content/delta events
  -> response.completed | response.incomplete | response.failed
```

For supported items the adapter handles:

- `response.output_item.added/done`;
- `response.content_part.added/done`;
- `response.output_text.delta/done`;
- `response.refusal.delta/done`;
- reasoning text/summary delta/done events;
- `response.function_call_arguments.delta/done`.

Deltas require a started output item and matching item/index. Done events must
match the fully assembled item. Duplicate terminal, event after terminal,
missing required item completion or mismatched arguments fails closed.

Unknown event types are retained as sanitized audit evidence. They are ignored
only when they cannot change admitted item assembly or terminal meaning;
otherwise the outcome is adapter invariant/unsupported failure.

## Terminal mapping

| Official terminal | Garive fact |
|---|---|
| `response.completed` with status `completed` | `Completed` with ordered items/usage/stop reason |
| `response.incomplete`, reason `max_output_tokens` | `Interrupted(OutputLimit)` with partial items/usage |
| incomplete/failed content-policy evidence | `Rejected(ContentPolicy)` or completed refusal item according to the official response shape |
| cancelled response or observer cancellation | `Interrupted(Cancelled)` |
| transport ends before terminal | `Interrupted(Transport)` |
| verified context-length rejection | `Rejected(ContextOverflow)` |
| verified auth failure | `Rejected(Authentication)` |
| exhausted 429 | `Unavailable(RateLimited, retry_after)` |
| exhausted service/model availability | `Unavailable(ModelUnavailable)` |

Only `response.completed` with a valid completed Response can produce
`Completed`. HTTP 2xx or SSE EOF alone cannot.

## HTTP errors and retry

The adapter parses the official error object (`message`, `type`, optional code
and parameter), sanitizes/bounds evidence, and classifies only verified signals.
Runtime config supplies retry limits. Adapter retry is allowed only before a
response becomes externally ambiguous and uses the same logical request ID;
provider billing idempotency is not assumed without an official guarantee.

`Retry-After` is parsed when present and valid. Raw response bodies, headers,
credentials and user content are never placed in public errors.

## Official wire fixtures

`spec/fixtures/providers/openai/responses/` contains:

- minimal text request/response;
- ordered text + reasoning + function-call response;
- complete SSE stream with argument delta chunking;
- incomplete max-output stream;
- refusal/content-policy case;
- 400 context/auth errors, 429 with retry hint and 5xx availability;
- malformed sequence/index/done/terminal cases;
- unknown event and unsupported output-item cases;
- known/cache/reasoning usage case.

Fixtures record their official source path and reviewed commit. Rust/Kotlin
parsers consume the same bytes and compare normalized facts.

## Acceptance

- request JSON matches the admitted official schema and omits unset fields;
- both parsers pass every official-shape fixture and reject malformed streams;
- stream chunking cannot change terminal ordered items;
- no response completes from HTTP status/EOF alone;
- usage and error classification follow the mappings above;
- adapter modules depend on the LLM contract, never Core/Runtime policy;
- no live API test runs without explicitly supplied credentials/endpoint.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
