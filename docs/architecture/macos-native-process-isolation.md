# macOS native process isolation

> This design selects Apple's Virtualization framework as the macOS-native
> F0 process boundary. It guides Runtime and macOS integration engineers away
> from deprecated sandbox profiles and toward a VM-owned execution receipt.

## Audience

Runtime, Desktop, release, and security engineers implementing or reviewing
`garive.process.run@1` on macOS.

## Why

F0 requires exact workspace scope, no network by default, process-tree
containment, resource limits, explicit environment, and trustworthy cleanup
after Runtime loss. A child process or process group cannot prove that whole
boundary.

The macOS 26.5 SDK header `usr/include/sandbox.h` marks `sandbox_init` as
deprecated and no longer supported. The installed `sandbox-exec(1)` manual
does the same. They are not an admitted product dependency.

Apple App Sandbox and XPC remain appropriate for signed application helpers
and privilege separation. Their rights are entitlement-based, however, and
Apple's supported helper workflow expects embedded, signed executables that
inherit a static application sandbox. That does not represent Garive's
explicit executable-lane catalogue plus one dynamic workspace authority.

Virtualization framework exposes the required host controls directly:

- VM configurations have no network devices by default;
- `VZSharedDirectory` selects one host directory and an enforced read-only
  flag;
- Virtio socket devices provide a private guest-agent control channel; and
- `VZVirtualMachine.stop()` destructively stops the complete VM.

## Decision

The macOS-native backend is one same-architecture Linux micro-VM per
invocation and dispatch attempt. It is an alternative to Podman, not a
fallback inside the Podman backend.

The VM contains a digest-pinned, read-only root image and Garive guest agent.
It receives no network device, graphics, audio, USB, keyboard, pointing, or
host clipboard device. The only host filesystem exposure is one VirtioFS
workspace share. A read grant creates a read-only share; a write grant creates
a read-write share. The only control channel is one Virtio socket.

The guest agent runs as the trusted supervisor. Workload processes run as a
non-root identity with no capabilities. The agent applies guest resource
limits, clears the environment, installs only the admitted baseline plus lane
values, executes the argv vector directly, bounds output, terminates remaining
workload processes, and returns one attempt-bound terminal receipt.

The macOS XPC service owns `VZVirtualMachine`. Runtime owns authority,
invocation identity, durable facts, and receipt acknowledgement. XPC is the
control-plane bridge; it is not cited as workload containment.

## Recovery boundary

The service retains a bounded receipt in its Runtime-private recovery root
until Runtime durably acknowledges it. A restart queries the exact invocation
and dispatch attempt:

| Service state | Runtime action |
|---|---|
| no owned VM and no receipt | prove absent, then publish uncertainty for a previously Started effect |
| running or paused VM | destructive stop, wait for stopped state, then publish uncertainty |
| retained terminal receipt | verify and recover the receipt without dispatch |
| mismatched identity or configuration digest | fail closed as state unknown |

No Started process is replayed. A pooled or reusable VM is not admitted until
separate measurements and cross-invocation erasure evidence exist.

## Delivery slices

| Slice | Deliverable | Evidence |
|---|---|---|
| V0-A | Explicit immutable VM configuration and executor-binding digest | Swift structure tests over official SDK types; no environment reads |
| V0-B | Versioned host/XPC and guest-agent protocol | strict decoder, identity, bounds, malformed-input, and code-signing admission tests |
| V0-C | Packaged VM service and guest image | no-network, read/write scope, argv/environment/resource, forced-stop, and receipt tests |
| V0-D | Runtime `ProcessIsolationBackend` composition | real SQLite Started/receipt/acknowledgement and independent-process recovery matrix |

V0-A does not claim process execution or close F0. Each later slice consumes
the preceding exact configuration identity.

## Official evidence

- [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)
- [Embedding a command-line tool in a sandboxed app](https://developer.apple.com/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app)
- [VZVirtualMachineConfiguration](https://developer.apple.com/documentation/virtualization/vzvirtualmachineconfiguration)
- [VZVirtioFileSystemDeviceConfiguration](https://developer.apple.com/documentation/virtualization/vzvirtiofilesystemdeviceconfiguration)
- [VZVirtualMachine.stop](https://developer.apple.com/documentation/virtualization/vzvirtualmachine/stop%28completionhandler%3A%29)

Evidence was reviewed against macOS 26.6.1, Xcode 26.6, Swift 6.3, and the
macOS 26.5 SDK on 2026-09-01.

## See also

- [`../../spec/design/macos-native-process-isolation.md`](../../spec/design/macos-native-process-isolation.md)
- [`../../spec/design/sandbox-safety.md`](../../spec/design/sandbox-safety.md)
- [`../../spec/design/basic-tools.md`](../../spec/design/basic-tools.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
