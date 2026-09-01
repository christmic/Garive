# V0-A — macOS native process VM configuration

> This Spec defines the first executable slice of the macOS-native F0 process
> backend: an explicit Virtualization framework configuration and its private
> executor-binding digest. It does not admit dispatch or recovery claims.

## Audience

Engineers implementing `GariveProcessIsolation` and reviewers checking F0
configuration identity before V0-B protocol work begins.

## Scope

V0-A owns only immutable macOS VM configuration construction. Runtime remains
the authority owner. The Swift adapter does not open the Ledger, resolve an
Agent Definition, choose a Tool, read environment configuration, start a VM,
or produce an effect receipt.

The input is `MacOSProcessVMPlanV1`:

```text
kernel_url, kernel_digest
initial_ramdisk_url, initial_ramdisk_digest
root_disk_url, root_disk_digest
workspace_url, workspace_mode
guest_protocol_revision
cpu_count, memory_size_bytes
control_timeout_milliseconds
```

URLs are Runtime-private file URLs. Digests are lowercase 64-character SHA-256
hex strings. The guest protocol revision is non-empty printable ASCII. CPU,
memory, and timeout are positive and within Virtualization framework plus V0-A
bounds. Unknown or omitted values fail construction; there are no defaults.

## Configuration projection

One plan constructs exactly:

| Property | Required value |
|---|---|
| boot loader | `VZLinuxBootLoader` with the exact kernel and initial ramdisk |
| root storage | one read-only Virtio block device over the exact root disk |
| workspace | one `VZVirtioFileSystemDeviceConfiguration` tagged `garive-workspace` |
| workspace mode | exact `VZSharedDirectory.isReadOnly` projection |
| control | one `VZVirtioSocketDeviceConfiguration` |
| entropy | one `VZVirtioEntropyDeviceConfiguration` |
| CPU and memory | exact admitted values |
| network | empty |
| other devices | no graphics, audio, keyboard, pointing, USB, or extra shares |

The fixed kernel command line is a versioned implementation constant. It may
name only the read-only root device, Virtio console, panic behavior, workspace
tag, and guest protocol revision. Changing it advances the configuration
format version and executor revision.

## Binding digest

`binding_digest` is lowercase SHA-256 over this length-delimited sequence:

```text
garive.macos-process-vm-config.v1
kernel URL bytes, kernel digest bytes
initial ramdisk URL bytes, initial ramdisk digest bytes
root disk URL bytes, root disk digest bytes
workspace URL bytes, workspace mode byte
guest protocol revision bytes
cpu count big-endian u64
memory size big-endian u64
control timeout big-endian u64
fixed kernel command-line bytes
```

Every variable field is prefixed by an unsigned big-endian 64-bit byte length.
The digest is Runtime-private and contains no secret. Logs and public errors
must not expose any input URL.

## Failures

The constructor returns one closed safe category:

| Failure | Meaning |
|---|---|
| `invalid_url` | a value is not an absolute file URL |
| `invalid_digest` | a digest is not canonical SHA-256 hex |
| `invalid_protocol_revision` | the revision is empty, non-ASCII, or unbounded |
| `invalid_cpu_count` | the CPU count is outside framework bounds |
| `invalid_memory_size` | memory is outside framework bounds or not MiB-aligned |
| `invalid_control_timeout` | timeout is zero or above 60 seconds |

Errors contain only the safe category. They never include paths, URLs, digest
values, boot arguments, or framework diagnostics.

## Acceptance

Swift tests must prove:

1. identical inputs produce the same digest and one exact SDK configuration;
2. every input field changes the digest;
3. read and write plans map only to the share read-only flag;
4. network and all unlisted devices remain absent;
5. invalid URLs, digests, revisions, CPU, memory, and timeout fail closed; and
6. source scans find no `ProcessInfo.environment`, `getenv`, `sandbox-exec`,
   `sandbox_init`, shell launch, or VM start call in the V0-A target.

V0-A is complete only after `swift test --package-path desktop/macos-native`
passes on the admitted stable Swift/Xcode toolchain. F0, T1, and the native
backend remain partial until V0-B through V0-D pass their own evidence.

## See also

- [`../../docs/architecture/macos-native-process-isolation.md`](../../docs/architecture/macos-native-process-isolation.md)
- [`sandbox-safety.md`](sandbox-safety.md)
- [`basic-tools.md`](basic-tools.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
