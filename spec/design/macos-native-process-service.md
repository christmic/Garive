# V0-C — packaged macOS process service

> This Spec decomposes the packaged macOS process backend into three executable
> slices without weakening V0-B identity, framing, or never-replay rules.

## Audience

Engineers implementing the signed XPC service, Linux guest image, and native
release packaging for `garive.process.run@1`.

## Why

V0-B proves protocol semantics but creates no process. A production boundary
must additionally prove that only the signed Garive backend can enter the XPC
service, the VM contains the workload, and terminal evidence survives Runtime
loss. Those are distinct failure domains and cannot be inferred from one Swift
package test.

## Ownership

Runtime remains the only authority owner. The XPC service owns one
`VZVirtualMachine` per exact invocation and dispatch attempt. The guest agent
owns workload supervision inside that VM. Neither native component selects an
Agent, Tool, executable lane, workspace, environment, or resource policy.

The service receives one canonical `Data` frame per XPC call. It never exposes
generated protobuf objects through Objective-C XPC, and it never returns raw
Foundation, Virtualization, Security, or guest diagnostics.

## Delivery

| Slice | Deliverable | Completion evidence |
|---|---|---|
| V0-C1 | Installable XPC bundle and closed host endpoint | bundle validation, exact caller admission, strict frame/error round trip, no VM construction |
| V0-C2 | Digest-pinned guest image and one VM execution | image manifest, no network, exact share mode, direct argv/environment, resource/output bounds |
| V0-C3 | Forced stop and retained receipt store | independent service kill/restart matrix, exact query/terminate/ack, receipt tamper and cleanup tests |

V0-C is complete only when all three slices pass. V0-C1 completion must not
change F0 or T1 from partial.

## V0-C1 service package

The application embeds exactly one bundle at:

```text
Contents/XPCServices/GariveProcessIsolationService.xpc
```

Its `Info.plist` is part of the signed bundle and contains the service bundle
identifier plus one exact backend code-signing requirement. Package tooling
receives both values explicitly; it has no defaults and reads no environment
configuration. The service validates the signed metadata before constructing
its bootstrap value.

The service calls `NSXPCListener.service()`, installs the requirement with
`setConnectionCodeSigningRequirement`, sets its delegate, and activates only
after all bootstrap validation succeeds. Each accepted connection must also
match the service effective user and login audit session. PID is diagnostic
correlation only.

The Objective-C interface contains one method:

```text
exchange(frame: Data, reply: (Data) -> Void)
```

The request is decoded only with `decodeHostRequestFrame`; every reply is a
canonical `ProcessHostResponseV1` frame. Until V0-C2 is installed, a valid
request returns `PROCESS_PROTOCOL_FAILURE_SERVICE_UNAVAILABLE`. Malformed or
oversized input returns the corresponding closed protocol failure. XPC transport
loss has no reply and is never translated into a trustworthy service result.

The C1 service target must not construct or start `VZVirtualMachine`, open image
or workspace paths, access the Ledger, read environment variables, spawn a
process, invoke a shell, or log frame bytes.

## V0-C2 guest execution

The package admits one immutable image manifest binding kernel, initramfs, root
disk, guest-agent binary, build recipe, architecture, and SHA-256 digests. The
release build reproduces every artifact or fails; downloaded mutable images are
not admitted.

One accepted start constructs the V0-A configuration, verifies every artifact
digest before VM creation, performs the V0-B challenge handshake over the sole
Virtio socket, and sends one workload. The guest mounts the root read-only and
the workspace at the fixed tag with the exact read/write mode. It creates no
network interface, clears inherited environment, installs only the specified
baseline and ordered entries, and executes argv directly as a non-root identity
with no capabilities.

The guest enforces timeout, process, open-file, and aggregate output bounds. A
terminal response is valid only after every descendant has terminated.

## V0-C3 recovery

The service persists one canonical terminal receipt under a service-private
recovery root before replying terminal. The record is keyed by the complete
identity and binding digest, written atomically, bounded, and never overwritten.

Query, terminate, and acknowledge implement the V0-B reducer against real VM
and receipt state. Terminate destroys the whole VM and waits for stopped state.
Acknowledge erases only an exact retained receipt after durable Runtime
acknowledgement. Corruption, duplicate ownership, mismatched identity, or an
unprovable VM state returns `STATE_UNKNOWN` without dispatch.

## Failures

| Boundary | Result |
|---|---|
| invalid signed package metadata | service exits before listener activation |
| caller signature, user, or audit mismatch | connection rejected |
| malformed or oversized frame | canonical closed protocol error |
| valid C1 request before C2 | `SERVICE_UNAVAILABLE` |
| XPC invalidation after dispatch | no inferred terminal; Runtime recovery queries exact identity |
| image, VM, socket, or guest mismatch | destroy VM, retain no success, return safe failure or unknown state |
| receipt persistence failure | no terminal reply |

## Acceptance

V0-C1 requires:

1. the shared native XPC admission module is the only implementation of caller
   signature, user, and audit-session policy;
2. an anonymous-listener integration test proves one canonical frame round
   trip and rejects a caller outside the admitted facts;
3. bundle assembly validates exact layout, plist keys, executable identity,
   and code signature before producing an artifact;
4. source policy rejects environment reads, shell/process launch, VM creation,
   raw payload logging, and open-ended XPC DTOs in the C1 target; and
5. `swift test --package-path desktop/macos-native` and the package validation
   command pass on the pinned stable toolchain.

V0-C2 and V0-C3 require native artifact and crash evidence described above;
unit tests or a mock guest cannot close them.

## Official evidence

The design was checked on 2026-09-01 against the installed macOS 26.5 SDK:

- `Foundation.framework/Headers/NSXPCConnection.h` documents
  `serviceListener`, listener activation, and
  `setConnectionCodeSigningRequirement`;
- `Virtualization.swiftinterface` exposes VM start and Virtio socket
  connection APIs; and
- V0-A records the exact Virtualization configuration surface.

## See also

- [`macos-native-process-protocol.md`](macos-native-process-protocol.md)
- [`macos-native-process-isolation.md`](macos-native-process-isolation.md)
- [`../../docs/architecture/macos-native-process-isolation.md`](../../docs/architecture/macos-native-process-isolation.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
