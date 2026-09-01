# V0-B — macOS native process protocol

> This Spec binds Runtime, the signed XPC service, and the Linux guest agent to
> one strict protocol. It does not package or start a VM.

## Authority and transport

[`../proto/garive/process/v1/process.proto`](../proto/garive/process/v1/process.proto)
is the only wire-shape source. Runtime supplies every authority-bearing value;
neither XPC nor the guest derives an invocation, attempt, executor revision,
Prepared digest, VM configuration digest, workspace mode, executable, argv,
environment, or limit.

XPC admits its caller with the existing exact signature, user, and audit-session
policy, then accepts only one opaque `Data` frame per method call. The guest uses
the same framing over the sole Virtio socket. A frame is a 4-byte unsigned
big-endian payload length followed by one protobuf envelope. Its total payload
is at most 1,114,112 bytes. Truncation, trailing bytes, an unset `oneof`, unknown
fields, unknown enum values, or a mismatched envelope direction fails closed.

The Swift consumer pins Apple SwiftProtobuf `1.38.1`, verified as the current
stable release on 2026-09-01. Runtime and guest use the repository's pinned
`prost` generator. Generated bindings are artifacts; this proto remains SSOT.
No protocol or service configuration comes from environment variables.

## Identity and canonical binding

Every command and result carries the complete `ProcessIdentityV1`. Digests are
exactly 32 bytes. Text identities and revisions are 1--256 printable ASCII bytes
without leading or trailing whitespace. Every response must byte-equal all
identity fields from its request.

`workload_digest` is SHA-256 over this sequence:

```text
garive.macos-process-workload.v1
protocol revision, invocation id, dispatch attempt id, executor revision
prepared digest, VM configuration digest
lane, executable, argv count and ordered argv values
working directory, workspace mode
environment count and ordered key/value pairs
max output bytes, timeout milliseconds, max processes, max open files
```

Text and byte fields are prefixed by a big-endian `u64` byte length. Counts and
numeric values are raw big-endian `u64`; workspace mode is one declared byte.
Environment entries are strictly increasing by UTF-8 key bytes and unique.
Unknown fields are never included because their presence is rejected.

## Bounds

| Value | Bound |
|---|---|
| file URL, executable, working directory | 1--4096 UTF-8 bytes; URL rules remain V0-A |
| lane | 1--128 printable ASCII bytes |
| argv | 1--256 values; each 1--16,384 bytes; aggregate <=262,144 bytes |
| environment | <=128 entries; aggregate key/value bytes <=262,144 |
| environment key | `[A-Za-z_][A-Za-z0-9_]{0,127}` |
| environment value | <=16,384 UTF-8 bytes; no NUL/CR/LF |
| output | stdout plus stderr <=1,048,576 bytes |
| timeout | 1--300,000 milliseconds |
| processes and open files | non-zero |
| challenge | exactly 32 unpredictable bytes from the XPC service |

The guest clears its inherited environment before installing the ordered
entries and a future, separately specified deterministic isolation baseline.
It executes the argv vector directly; no shell, PATH lookup, interpolation, or
string command surface exists. Host validation requires the VM-plan workspace
mode to equal the workload mode. The guest receives only identity and workload;
host file URLs and VM image paths never cross the Virtio socket.

`receipt_digest` is SHA-256 over `garive.macos-process-receipt.v1`, the exact
`workload_digest`, a one-byte exit tag (`0` code, `1` signal, `2` timeout), a
raw big-endian two's-complement `i32` for code or signal, stdout, stderr,
truncated byte, and process-tree-terminated byte. Boolean bytes are only `0` or
`1`; variable bytes use the same `u64` length prefix. The receipt field itself
is excluded.

## State machine

```text
Absent --start--> Starting --guest ready/execute--> Running
Running --terminal + tree absent--> TerminalRetained
Starting/Running --terminate--> Absent
TerminalRetained --ack exact receipt digest--> Absent
```

- `preflight` validates complete bounds, resources, digests, configuration, and
  service availability without creating a VM.
- `start` is legal only from `Absent`. Runtime calls it only after the durable
  Started fact. A lost response is never retried.
- `query`, `terminate`, and `acknowledge` require the exact complete identity.
  A partial or mismatched identity returns `IDENTITY_MISMATCH` and changes no
  state.
- `query` returns only `Absent`, `Starting`, `Running`, or `TerminalRetained`;
  the latter contains the retained terminal receipt.
- `terminate` is idempotent only for the exact identity. It succeeds after the
  complete VM is stopped and absence is proved.
- `acknowledge` removes a retained receipt only when both identity and receipt
  digest match, then proves owned VM and receipt absence.

The guest handshake echoes the exact identity and 32-byte challenge and reports
its explicit agent revision before any workload is sent. A mismatch destroys
the VM and yields state unknown. The terminal receipt is admitted only when the
identity matches, output is bounded, one exit classification is present, and
`process_tree_terminated` is true.

`TerminalRetained` requires a receipt; every other status forbids one. Rust
performs a bounded wire-tag scan before `prost` decoding so unknown fields are
observable and rejected rather than discarded.

## Safe failures and observability

Only the closed `ProcessProtocolFailureV1` categories cross a boundary. Logs may
contain the category and a private correlation digest, but never URLs, argv,
environment, output, challenges, or raw identities. Malformed input never
returns parser or framework diagnostics.

## Acceptance

V0-B requires generated Rust and Swift bindings plus validators proving:

1. the fixed Rust/Swift workload digest vector is identical;
2. every field, order change, duplicate environment key, bound edge, malformed
   frame, unknown field/enum, absent `oneof`, and trailing byte fails correctly;
3. the host and guest direction envelopes cannot be confused;
4. the state reducer rejects replayed start, mismatched query/terminate/ack,
   premature receipt, false tree termination, and wrong receipt digest; and
5. source scans find no environment configuration, shell launch, VM start, raw
   payload logging, or open-ended error strings.

V0-B is complete only after both language suites pass. V0-C packaging and real
VM evidence, and V0-D Runtime composition and recovery, remain unclaimed.

## See also

- [`macos-native-process-isolation.md`](macos-native-process-isolation.md)
- [`sandbox-safety.md`](sandbox-safety.md)
- [`../../docs/architecture/macos-native-process-isolation.md`](../../docs/architecture/macos-native-process-isolation.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: proposed
