# C5b — Deterministic governed effect batches

> This Spec admits bounded parallel execution only for independently proven
> read-only Prepared Calls. Resource declarations, conflict planning, durable
> start order, timeout, cancellation, and observation order remain deterministic.

## Audience

Engine Tools, Runtime dispatcher, executor-adapter, and Kotlin experiment
engineers extending the accepted sequential C4/C5/C6 effect path.

## Why

C5 intentionally executes model intents one at a time. The effect design now
explores parallel batches, resource conflicts, streaming output, adaptive
limits, speculative dispatch, workspace snapshots, and caching. Admitting all
of those together would weaken authorization and recovery. C5b extracts the
smallest measurable concurrency slice: exact resource claims and parallel
read-only execution with deterministic durable publication.

## Compatibility

- C4 preparation, C5 authority/interaction/receipt semantics, and C6 Runtime
  ownership remain unchanged.
- A Runtime with C5b disabled executes the same intents sequentially.
- A tool without C5b declarations remains valid for sequential C5 but is never
  parallel-eligible.
- Existing `ReplayClass` is the effect/recovery declaration. C5b does not add a
  second `effect_class` value with overlapping meaning.
- Model intent order is authoritative for start, terminal, observation, and
  continuation order. Executor completion timing is never model-visible truth.

## Contract versioning

C5b does not silently change the shipped C4 digest. C4 v1 Prepared Calls retain
their exact preimage and are always sequential. A C5b-capable definition opts
into `prepared_contract_version = 2`; the v2 preimage extends v1 with
`access_policy_revision`, `access_resolver_revision`, and the exact normalized
`invocation_accesses` plus `max_result_bytes`. Version is itself inside the
preimage.

Runtime values and the `effect.prepared.v2` fact carry the Prepared Call's
contract version. Later grants/facts bind its digest, and recovery resolves the
version from that exact prepared fact. Unknown versions fail before
authorization. A definition revision cannot change contract version, policy,
resolver meaning, or result bound; doing so requires a new Tool revision and
effective Agent snapshot.

## Two-level resource declaration

One declaration cannot safely describe both a tool's possible access surface
and a concrete invocation's exact targets.

| Level | Owner | Meaning |
|---|---|---|
| `ToolAccessPolicy` | Frozen Tool definition | Maximum namespaces, modes, patterns, and resolver revision this tool may claim. |
| `InvocationAccessSet` | Trusted C4 resource resolver | Canonical exact resources derived from validated arguments for one Prepared Call. |

The untrusted model cannot submit either declaration. The resolver is part of
the admitted Tool implementation and runs during preparation after schema
validation. Its revision is frozen in the Tool definition.

```text
AccessNamespace = filesystem | process | network | runtime
AccessMode      = read | write | exclusive
ResourceAccess  = { namespace, resource_key, mode }
InvocationAccessSet = non-empty ordered unique ResourceAccess values

ToolAccessPolicyV1 {
  policy_revision
  filesystem_roots: [{ root, allowed_modes[] }]
  process_lanes: [{ lane, allowed_modes[] }]
  network_origins: [{ origin, allowed_modes[] }]
  runtime_lanes: [{ lane, allowed_modes[] }]
  max_accesses, max_result_bytes
}
```

`resource_key` is an opaque canonical UTF-8 identity interpreted only by the
matching Runtime capability. Filesystem keys are workspace-relative normalized
paths with no traversal or symlink resolution ambiguity. Network keys are
Runtime-normalized origin identities, never credentials or full URLs. Process
keys identify an admitted executor lane, not arbitrary command text. Unknown
namespaces and empty keys fail preparation.

The Prepared Call digest includes policy revision, resolver revision, ordered
exact access set, and non-zero `max_result_bytes`. Grants bind that digest, so
claims and buffer charge cannot change after authorization.

Policy entry lists sort by raw UTF-8 root/origin/lane; each non-empty mode set
sorts `read, write, exclusive`. Access sets sort by namespace enum order
`filesystem, process, network, runtime`, then raw UTF-8 key, then that same mode
order. Duplicate policy entries, modes, or accesses are invalid rather than
deduplicated. Policy roots/origins/lanes are authority ceilings; only exact
`ResourceAccess` values participate in conflicts.

A filesystem root covers an equal key or a descendant beginning with
`root + "/"`; raw string prefix alone never covers a sibling. Network origins
and process/runtime lanes cover only exact equality. In every namespace the
requested mode must occur in the matched policy entry's `allowed_modes`.

Filesystem canonicalization is lexical and workspace-relative before any I/O:
UTF-8 `/` separators, no empty/`.`/`..` segment, no leading slash, no NUL, and
no Unicode normalization or case folding. Runtime later resolves the exact
UTF-8 components beneath its workspace capability without following an
escaping symlink and rejects unsupported platform aliases. Network origins are
`http`/`https` plus lowercase ASCII DNS name or RFC 5952 IP literal and explicit
effective decimal port, with no user-info, path, query, fragment, or trailing
DNS dot. Process/runtime lanes are admitted opaque ASCII identities.

## Declaration validation

Preparation fails closed when:

- an access falls outside the Tool policy;
- filesystem arguments cannot be resolved without I/O or contain ambiguous
  lexical aliases, traversal, or absolute paths;
- `ReplayClass::ReadOnly` declares any `write` or `exclusive` access;
- a non-read-only invocation has no `write`/`exclusive` claim despite a write or
  process capability;
- the same namespace/key appears with conflicting duplicate modes;
- the resolver is unavailable, non-deterministic, performs I/O, or exceeds its
  declared input/output bound.

Runtime authorization independently narrows workspace, origin, process lane,
and duration/output bounds. Resource claims never grant authority.

## Conflict relation

Two accesses conflict when their namespace/key are equal and at least one mode
is `write`, or when their namespaces are equal and at least one mode is
`exclusive`. Thus `exclusive` conservatively closes its complete namespace for
that step. Distinct filesystem descendants are independent only when the
resolver proves exact non-overlapping paths; ancestor/wildcard policies are
admission bounds, not invocation keys.

Two invocations conflict if any access pair conflicts. Graph construction uses
original intent indexes as node identities and evaluates all pairs in ascending
index order. The resulting adjacency matrix and digest are deterministic.

## Batch planner v1

`plan_effect_batch(prepared_intents, limits)` is pure. It returns ordered
`SequentialStep` or `ParallelReadGroup` steps.

```text
EffectBatchLimitsV1 {
  max_intents, max_accesses_per_intent, max_total_accesses
  max_parallel_reads, max_buffered_result_bytes
}
EffectBatchPlanV1 {
  schema_version: 1
  prepared_contract_version: 2
  ordered_prepared_digests[]
  conflict_graph_digest
  steps: (SequentialStep { intent_index } |
          ParallelReadGroup { intent_indexes[] })[]
  plan_digest
}
```

All counts and every Prepared Call `max_result_bytes` are non-zero. Intent
indexes are zero-based, unique, cover every input exactly once, and increase
within/across steps. Graph bytes contain one byte (`0x00` or `0x01`) for each
upper-triangle pair in ascending `(left_index, right_index)` order. Its digest
and the plan digest are lowercase SHA-256 over their named canonical bytes; the
plan uses RFC 8785 JSON with `plan_digest` omitted.

1. Walk Prepared Calls in model order.
2. A read-only call with a non-empty exact access set may join the current read
   group when it has no graph edge with any member and group bounds permit.
3. Every other call closes the current group and becomes one sequential step.
4. A suspension-capable interaction boundary also closes the group.
5. Group size, total access count, sum of member `max_result_bytes`, and planner
   work must fit the explicit non-zero Runtime limits.

The planner does not inspect model prose or scheduling hints. The same ordered
Prepared Calls and limits always produce the same graph and plan in Rust and
Kotlin.

V1 never runs writes, processes, unknown tools, or network mutations in
parallel, even when their keys are disjoint. This conservative rule leaves a
measurable path to broader concurrency without changing C5 recovery semantics.

## Durable execution protocol

C5b coordinates two additive L0 schemas:

```text
effect.prepared.v2 {
  prepared_contract_version: 2
  prepared_digest, tool_name, tool_revision, replay_class, model_call_id
  access_policy_revision, access_resolver_revision
  invocation_accesses: ContentBinding
  max_result_bytes
}
execution.effect_batch_planned.v1 {
  plan_digest, conflict_graph_digest, ordered_prepared_digests: ContentBinding
  steps: ContentBinding, max_parallel_reads, max_buffered_result_bytes
}
```

Both ContentBindings contain canonical JSON matching the pure plan. The batch
fact is scoped to the active Turn/Execution and has no Tool Invocation ID. It
commits after every included preparation/authorization and before the first
member `effect.started`. One Prepared Call appears in at most one committed
batch plan. Recovery validates/reuses that fact; it never silently recomputes a
different plan after restart. Sequential v1 calls require no batch fact.

For each plan step Runtime performs:

1. Revalidate exact grants, cancellation, lease, Tool revision, plan digest,
   access policy, and workspace capability.
2. Validate the committed `execution.effect_batch_planned`, then commit
   `effect.started` facts in model order before dispatch. Preparation and
   authorization were already committed before the plan. No executor begins
   before its own `started` commit.
3. Dispatch a parallel group with one frozen `max_parallel_reads` bound. Each
   invocation receives its own non-zero timeout and cancellation token.
4. Collect executor terminals into bounded per-index slots. A completion may be
   retained internally but cannot publish ahead of an earlier group member.
   Each slot applies the accepted C5 output limit/truncation before buffering
   and can never exceed its Prepared Call `max_result_bytes` charge.
5. Commit terminal receipt/failure and model-visible observation strictly in
   model order. Then continue to the next plan step.

Runtime may commit terminals in one transaction or ordered transactions, but a
later observation can never exist without every earlier invocation having a
terminal/reconciliation fact. A durability failure stops publication and uses
the accepted C5 recovery query for every invocation whose `started` fact exists.

## Timeout and cancellation

- Each invocation timeout begins only after durable `started` and immediately
  before executor dispatch. Queue time is bounded separately at the step level.
- Timeout requests cooperative cancellation. Runtime waits one bounded grace
  period, then classifies the invocation using executor evidence: proven not
  completed, proven terminal, or uncertain.
- Turn cancellation fans out to active group members. Unstarted later steps are
  never allocated. Each started member still receives exactly one terminal or
  reconciliation state in model order.
- Dropping a future/task is not terminal evidence. Read-only replay remains
  subject to its original invocation identity and C5 retry policy.

## Determinism and observability

For one fixed input, configuration snapshot, grants, and normalized executor
terminals, plan bytes and durable semantic fact order are identical across all
executor completion interleavings. Wall-clock timestamps, latency metrics, and
executor diagnostics are explicitly excluded from semantic equality.

Metrics use bounded labels only: group size, queue duration bucket, execution
duration bucket, timeout classification, conflict count, and parallel-disabled
reason. Resource keys, arguments, content, paths, origins, and command text are
forbidden labels.

## Stable failures

| Code | Meaning |
|---|---|
| `effect_access_invalid` | Policy, resolver, canonical key, or mode declaration is invalid. |
| `effect_access_not_authorized` | Runtime capability does not cover the exact access set. |
| `effect_batch_bound_exceeded` | Planner, group, access, queue, or buffer bound was exceeded. |
| `effect_batch_plan_stale` | Prepared digest, grant, policy, or workspace revision changed. |
| `effect_queue_timeout` | Step could not begin within its queue bound. |
| `effect_execution_timeout` | Executor exceeded its per-invocation bound with proven terminal handling. |
| `effect_batch_uncertain` | At least one started invocation requires reconciliation. |
| `effect_batch_durability_failure` | Ordered terminal/observation publication failed. |

Existing C4/C5 failures retain their meanings and take precedence when they
occur before C5b planning.

Failure precedence inside C5b is: invalid/unknown contract version, declaration
shape, policy coverage, authorization coverage, aggregate bounds, stale plan,
queue timeout, execution timeout, uncertainty, then durability. When several
independent declarations are invalid, report the lowest intent index and then
the lowest canonical access index.

## Explicitly deferred

| Proposal | Reason it is not C5b |
|---|---|
| Speculative dispatch from partial model streams | A streamed proposal is not an authorized immutable Prepared Call. |
| Streaming tool stdout/stderr to durable facts | Requires redaction, backpressure, truncation, and ephemeral/durable event separation. |
| Parallel mutating/process/network effects | Requires executor-specific isolation and stronger reconciliation evidence. |
| AIMD/adaptive concurrency | Runtime tuning needs production measurements and cannot alter semantic order. |
| Read-result cache | Freshness cannot be derived from wall time/mtime alone; needs source revision tokens. |
| Git worktree/branch snapshots | Workspace lifecycle and adoption are a separate capability contract. |
| BDI terminology | Useful explanatory mapping, not a wire type or scheduling invariant. |

## Acceptance evidence

- shared Rust/Kotlin planner fixtures cover exact access normalization, policy
  denial, every conflict mode, bounds, group formation, and graph/plan digests;
- property tests enumerate completion permutations and assert identical semantic
  fact/observation order with exactly one terminal per started invocation;
- fake-clock Runtime tests cover queue timeout, invocation timeout, cancellation,
  grace expiry, partial group start, durability failure, and uncertain recovery;
- real executor tests prove filesystem confinement, symlink/case behavior, and
  that no dispatch precedes the matching committed `started` fact;
- sequential-mode differential tests prove the same normalized observations as
  C5 for accepted read-only batches;
- source scans prove no speculative, write-parallel, environment-discovery, or
  unbounded buffering path is reachable.

## See also

- [`prepared-tool-call.md`](prepared-tool-call.md) — C4 immutable call and digest.
- [`governed-effects.md`](governed-effects.md) — C5 authority, receipt, observation, and recovery.
- [`durable-runtime-turn.md`](durable-runtime-turn.md) — C6 orchestration and leases.
- [`../../docs/architecture/core/effect-layer.md`](../../docs/architecture/core/effect-layer.md) — broader mechanism research.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
