# F0 — Sandbox enforcement and safety governance

## Status

Accepted implementation contract. F0 specializes the C4/C5 execution boundary
without moving authorization or concrete execution into Core.

## Scope and ownership

F0 defines portable enforcement requirements, exact safety-decision bindings,
Runtime sandbox preflight and terminal proof. Existing values remain
authoritative:

- C4 `ExecutionRequirements` declares filesystem/process/network capability
  classes and duration/output ceilings;
- C5b `InvocationAccessSet` names exact resources;
- C5 `InvocationGrant` is the only authority for one invocation;
- C5 receipts and uncertainty rules own post-start recovery.

`engine/tools` owns pure F0 values and coverage checks. Runtime owns actor
policy, workspace identity, concrete sandbox selection, operating-system
controls, clocks, credentials, execution and durable facts. Kotlin implements
only the pure values and coverage semantics.

## Portable enforcement profile

```text
SandboxRequirementsV1 {
  controls: canonical non-empty set<SandboxControl>
  max_processes: non-zero u32 when Process is requested, otherwise absent
  max_open_files: non-zero u32
}

SandboxControl =
  FilesystemScope
  | SymlinkContainment
  | ProcessContainment
  | StructuredArguments
  | EnvironmentAllowlist
  | NetworkOriginScope
  | RedirectRevalidation
  | ResourceLimits
```

Controls describe observable enforcement, not a technology. They do not name
containers, namespaces, seccomp, Seatbelt, App Sandbox or a particular
operating system.

Cross-field rules:

- any filesystem capability requires `FilesystemScope`,
  `SymlinkContainment` and `ResourceLimits`;
- `Process` requires `ProcessContainment`, `StructuredArguments`,
  `EnvironmentAllowlist`, `ResourceLimits` and `max_processes`;
- `Network` requires `NetworkOriginScope`, `RedirectRevalidation` and
  `ResourceLimits`;
- `StructuredArguments` means an argv vector is passed directly; it does not
  make an explicitly named shell safe;
- limits narrow C4 ceilings. Zero, duplicate/unknown control, process count
  without Process, or missing required control fails catalogue admission.

The profile is part of Tool Definition revision identity and the Prepared Call
digest amendment. Adding or removing a control therefore creates a different
tool revision/digest and requires new authorization.

F0 definitions use Prepared Call v3. V3 retains every v2 field and adds the
complete `sandbox_requirements` canonical value plus its independently
verified `sandbox_requirements_digest` to the v3 digest preimage. A v3
definition cannot be prepared through a v1/v2 entry point; unknown or missing
profiles fail before authorization.

The durable `effect.prepared.v3` payload additionally binds canonical tool
arguments through the standard `ContentBinding`. Runtime may use bounded
`inline_utf8` or a verified opaque content reference; Host/UI projections must
never expose either. This is recovery state, not audit display data: after a
crash Runtime resolves it, re-prepares against the exact installed definition
and resolver revisions, and accepts it only when the Prepared digest and exact
access set are unchanged.

## Safety request and decision

Runtime constructs this request from authenticated and committed state:

```text
SafetyRequestV1 {
  request_id
  invocation_id
  prepared_digest
  tool_name, tool_revision
  actor_authority_reference
  goal_reference?
  plan_reference?
  exact_access_digest
  sandbox_requirements_digest
  effective_policy_revision
}

SafetyDecisionV1 =
  Allow { decision_id, constraints_digest, reason_codes }
  | Deny { decision_id, safe_code }
  | InteractionRequired { decision_id, interaction }
```

All identities and digests are non-empty and typed. `reason_codes` is a
canonical set from Runtime's admitted public catalogue; it is audit metadata,
not authority. Private rule identifiers/evidence stay behind a Runtime content
reference.

An Allow decision binds every request field and may only narrow access,
controls, duration, output, process count or open-file count. Runtime converts
it into the existing C5 grant only after committing `safety.decided`. Deny maps
to the governed rejected observation when policy permits correction.
Interaction uses the existing C5 typed suspension; a response causes a new
safety/authorization evaluation and grants nothing by itself.

## Sandbox binding and proof

After authorization Runtime selects one immutable binding:

```text
SandboxBindingV1 {
  binding_id
  workspace_capability_id
  executor_id, executor_revision
  policy_revision
  supported_controls
  filesystem_scope_digest?
  process_lane_digest?
  network_scope_digest?
  environment_allowlist_digest?
  effective_limits_digest
}
```

The workspace capability is Runtime-owned and opaque outside Runtime. Absolute
paths, bookmarks, descriptors, tokens and credentials are never portable
values or model-visible fields.

Preflight verifies:

1. invocation, Prepared Call, grant, safety decision and binding identities;
2. every granted control is supported by the exact executor revision;
3. every exact C5b access is inside the bound scope;
4. effective limits are equal to or stricter than Prepared/granted limits;
5. the workspace capability and policy revision are current;
6. the executor supports the declared replay class with proof, not metadata.

Runtime commits `sandbox.bound` and successful `sandbox.preflighted` before
`effect.started`. A failed preflight commits no Started fact and returns
`requirement_unsupported`, `sandbox_binding_stale` or
`sandbox_scope_mismatch` as appropriate.

The durable chain is exact and invocation-scoped:

```text
effect.prepared.v3 -> safety.decided(Allow) -> effect.authorized.v2
  -> sandbox.bound -> sandbox.preflighted -> effect.started
```

Each arrow is validated by the Ledger transition reducer. The final three
facts repeat the minimum identity/digest bindings needed to reject a mixed
decision, workspace, executor, grant or dispatch attempt after restart. Deny
and InteractionRequired never admit `sandbox.bound`; they continue through the
existing governed observation or typed interaction path.

`effect.prepared.v3` is committed before Runtime calls the Safety broker;
`safety.decided` is a later commit. Broker unavailability therefore leaves the
same invocation at the explicit Safety-pending recovery cut and never erases
or silently reallocates its Prepared identity.

The local governed worker freezes one composition containing public Tool
definitions, a pure `ToolPreparationPort`, Safety broker, Sandbox broker and
executor. Core sees only definitions plus the preparation interface. A v3
intent therefore cannot bypass F0 through the legacy v1 Core catalogue path;
the production loop proves the same nine-fact chain as direct Runtime use.

## Filesystem enforcement

Workspace filesystem keys remain non-empty relative slash-separated values
without embedded `.`, `..`, empty components, NUL, backslash or an absolute
prefix. The exact key `.` is reserved for the workspace root and may be emitted
only by a Tool resolver for an explicitly directory-valued argument.

An enforcing executor resolves from an already opened Runtime capability. It
must reject symlinks/reparse points and verify each component without a
check-then-open escape. Case and Unicode spelling are exact; Runtime does not
silently normalize a caller's resource key. Reads and listings are bounded.
Writes use an admitted journal/temporary plus atomic replacement strategy and
return a receipt binding previous/new content digests. Unsupported filesystem
semantics fail before start.

## Process enforcement

`process.run` receives a non-empty executable lane plus a bounded argv vector.
No argument string is re-parsed by a shell. Runtime resolves the lane to a
configured executable capability and constructs the environment only from an
explicit allowlist. It supplies a fixed working-directory capability, process
count, open-file, duration and output bounds.

Network is denied unless a Network access and origin scope were independently
authorized and the selected executor proves enforcement. Timeout, cancellation
or worker loss after process start requires a trustworthy terminal receipt;
otherwise the effect is uncertain. Exit status alone is not a receipt.

## Network enforcement

Exact origins use the C5b canonical `scheme://host:port` identity. The
executor revalidates every redirect against the granted origins, constrains
resolved destinations according to Runtime policy, strips ungranted
credentials/headers and applies byte/time bounds. Provider/model transport is
not a tool network capability and follows its own H1-T contract.

## Result boundary

Before Core receives a result, Runtime verifies the C5 receipt and applies:

- bounded UTF-8/portable JSON validation;
- secret, absolute-path, environment and private-policy redaction;
- deterministic truncation with an explicit `truncated` flag;
- safe terminal-code mapping;
- optional lossless content-reference storage under separate access control.

Live stdout, filesystem events or policy diagnostics are best-effort telemetry
and cannot substitute for a terminal receipt/result fact.

## Stable failures

| Code | Meaning |
|---|---|
| `sandbox_requirement_invalid` | Portable profile is malformed or incomplete. |
| `safety_denied` | Authenticated policy denied the exact request. |
| `safety_interaction_required` | Existing C5 interaction must resolve. |
| `safety_decision_conflict` | Identity was reused with different bindings. |
| `sandbox_enforcement_unsupported` | Selected executor cannot prove a required control. |
| `sandbox_binding_stale` | Workspace/executor/policy binding is no longer current. |
| `sandbox_scope_mismatch` | Exact resolved access is outside the bound scope. |
| `sandbox_receipt_invalid` | Terminal proof failed identity/content validation. |

Unknown versions, controls, decision variants or binding fields fail closed.
Diagnostic strings are neither compatibility keys nor model-visible content.

## Recovery

| Durable position | Decision |
|---|---|
| safety decision absent | Re-evaluate current policy; do not invent a grant. |
| Allow committed, no binding | Select/preflight under current revisions. |
| binding committed, no Started | Revalidate all revisions; dispatch may proceed with same invocation. |
| Started, no receipt | Apply C5 replay proof; otherwise reconcile. |
| receipt, no result | Reconstruct from verified receipt/content reference. |
| terminal result | Return idempotently. |

Configured recovery reconstructs the same Prepared-v3 invocation and calls the
current Safety and Sandbox brokers. Already durable decision, grant, binding
and preflight facts must equal the newly derived candidates exactly; Runtime
appends only the missing ordered suffix. A changed policy constraint, executor
revision, workspace binding, effective limit or dispatch-attempt identity
fails before `effect.started` and never allocates a replacement invocation.

Policy or binding changes after Started never authorize a new replay under the
old invocation.

## Acceptance evidence

- shared Rust/Kotlin fixture for every control, cross-field rule,
  normalization, digest sensitivity, coverage and stable failure;
- source/dependency gates proving Engine imports no filesystem/process,
  environment, Runtime or sandbox technology;
- real descriptor/capability-rooted filesystem tests for traversal, symlink,
  case, Unicode, concurrent replacement, byte bounds and revoked scope;
- real process tests for argv preservation, environment omission, working
  directory, child/output/time bounds, cancellation and no-network posture;
- fake-policy tests for exact decision binding, narrowing, interaction and
  conflicts;
- fault injection at every durable position in the recovery table.

## See also

- [`prepared-tool-call.md`](prepared-tool-call.md)
- [`governed-effects.md`](governed-effects.md)
- [`deterministic-effect-batches.md`](deterministic-effect-batches.md)
- [`agent-foundation-capability-spec-set.md`](agent-foundation-capability-spec-set.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
