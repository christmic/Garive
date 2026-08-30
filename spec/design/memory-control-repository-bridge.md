# M2-C2 — Runtime Memory repository bridge

> This Spec closes the production gap between committed M0/M1 Memory facts and
> the M2 control repository. Export, recall and import must observe one current
> Memory state; a Desktop workflow may not initialize a parallel test store.

## Audience

Runtime, Ledger, Memory and Desktop engineers implementing M2-D over real
durable Agent Memory.

## Problem

The verified M2-C adapter can initialize, export and atomically import a
`memory_control_current` projection. Production M0/M1 writes currently commit
only Session Ledger facts, while the initializer is called only by M2 tests.
Therefore M2-D cannot honestly advertise Memory control until Runtime maintains
the control projection from the same committed source records.

## Ownership

- Session Ledger facts and M2 journal events are append-only source records.
- Runtime owns one namespace-scoped `MemoryRepositoryProjectionV1`.
- Recall, maintenance, M2 export and M2 import read that same projection.
- Desktop receives only bounded M2-D views and never initializes or repairs it.

## Projection row

```text
MemoryRepositoryProjectionV1 {
  namespace_id, repository_revision
  records: [MemoryRepositoryRecordV1]
}
MemoryRepositoryRecordV1 {
  record_id, active_revision_id
  authority, memory_type, memory_role
  scope, scope_owner_id, lifecycle, sensitivity
  content_binding, content_digest, document_digest
  provenance_fact_refs[]
}
```

Every field required by an M2 document must come from an admitted committed
fact or a frozen policy binding. Missing authority/type/lifecycle is corruption,
not a default. Referenced or restricted content remains non-visible unless the
exact read grant is present.

## Transaction contract

1. A Runtime command plans its normal M0/M1 fact batch and the corresponding
   projection transition from the same pre-state.
2. SQLite validates the fact batch, projection preconditions and namespace
   revision under one immediate transaction.
3. Facts commit before the derived current row becomes externally observable;
   both become durable in the same transaction.
4. A changed current row advances `repository_revision` exactly once per
   transaction. A no-op does not advance it.
5. M2 import commits its journal event and applies the same projection
   transition path; it does not bypass M0/M1 invariants.

Direct writes, supersession, lifecycle maintenance, promotion, tombstone and
erasure each have an explicit transition. Organisation-published state cannot
be created or changed by M2 import. Learned content edited through M2 becomes a
new user-declared revision with the learned revision retained as provenance.

## Reconstruction and migration

On an existing database without the projection marker, Runtime reconstructs a
candidate projection from authorized fixed Session prefixes, independently
replays M1 authority/type/lifecycle facts, verifies every referenced identity,
then commits one canonical bootstrap transaction. It never guesses missing M1
metadata. An incomplete or contradictory history fails with
`memory_repository_corrupt`; Desktop reports Memory unavailable while ordinary
Agent Sessions remain readable.

After the marker exists, startup verifies namespace revision, current rows and
their source-fact bindings. It does not silently rebuild a partially populated
projection. A separate explicit repair tool is outside M2-C2.

## Public Runtime boundary

```text
open_memory_repository(context) -> Ready {namespace_id, revision}
                                 | Unavailable | Corrupt
read_memory_repository(grant, limits) -> bounded projection
prepare_memory_repository_import(source, grant, limits) -> exact M2 plan
commit_memory_repository_import(command, confirmation) -> M2 receipt
get_memory_control_command(command_id) -> Unknown | Committed {receipt}
```

`context` is constructed by backend configuration from exact namespace,
authorized prefixes, actor authority and scope grants. No environment lookup,
filesystem path or frontend value can create it.

## Stable failures

| Code | Meaning |
|---|---|
| `memory_repository_unavailable` | No admitted Memory control context is installed. |
| `memory_repository_corrupt` | Source facts and current projection cannot be reconciled. |
| `memory_repository_stale` | A fixed prefix or repository revision changed before commit. |
| `memory_repository_unauthorized` | Namespace, scope, content or action grant is absent. |

Existing M2 validation, bound, stale-plan, command-conflict and persistence
codes remain unchanged after this boundary succeeds.

## Acceptance evidence

- shared fixture covers every M0/M1-to-projection transition and corruption;
- temporary SQLite tests prove fact batch and projection are atomic at every
  injected boundary and survive restart;
- reconstruction from multiple ordered fixed prefixes equals live projection;
- M2 export after real Memory writes contains the exact active revisions;
- M2 import changes the same state consumed by recall and survives restart;
- source scans prove Desktop cannot initialize the repository or supply paths,
  authority, namespace, prefixes or hidden content.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
