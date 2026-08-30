# docs/

The accepted product architecture includes
[`architecture/mobile-remote-work.md`](architecture/mobile-remote-work.md) for
the native iOS/Android remote Agent control experience.

`docs/` contains Garive's human-edited thinking. These are personal working
documents: they may be rough, combine alternatives, or change in place. They
are not archived merely because implementation has not started.

## Start here

- [`manual/mobile-user-guide.md`](manual/mobile-user-guide.md): complete Chinese
  setup, pairing, remote-control, recovery and screenshot guide for iOS and
  Android.
- [`manual/tui-user-guide.md`](manual/tui-user-guide.md): complete operator and
  end-user guide for the resident terminal client.
- [`architecture/README.md`](architecture/README.md): current architecture
  index and document status.
- [`architecture/system.md`](architecture/system.md): product boundaries and
  ownership map.
- [`architecture/core/`](architecture/core/): detailed mechanism discussions.

## Document states

- **working**: an idea under active discussion;
- **active**: the current direction used to guide a slice;
- **superseded**: retained only when its history remains useful, with a pointer
  to the replacement;
- **implemented**: behavior has executable evidence and any normative contract
  lives beside the boundary that enforces it.

A design note can be edited in place as it converges. Add status, open
questions, and evidence where useful; do not force every personal note into a
ceremonial template.

## Boundary with `spec/`

Move only the smallest enforceable contract to `spec/` when a real process,
storage, or language boundary is being implemented. Internal domain types and
exploratory mechanisms remain here or beside their owning code. Protobuf is a
wire format, not the default domain model.

## Style

- Technical writing is English; concise Chinese context is fine in personal
  notes when it preserves the original reasoning.
- Prefer explicit owners, facts, alternatives, and unresolved questions over
  invented certainty or numeric gates without measurements.
- Cross-link the owning component and executable evidence when they exist.
