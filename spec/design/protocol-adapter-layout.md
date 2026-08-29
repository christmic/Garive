# Protocol adapter source layout and wire vocabulary

> Normative file ownership and constant policy for every Rust and Kotlin wire
> adapter. Protocol modules may differ in their official type catalogue, but
> they must not invent a different source architecture.

## Required layout

```text
adapters/<protocol>/
  src/
    lib.rs          public facade and exports only
    config.rs       constructor configuration and HTTP descriptor
    error.rs        protocol adapter failure vocabulary
    request.rs      typed request values, validation, and JSON encoding
    response.rs     typed ordinary response/error decoding
    events.rs       typed event catalogue when the protocol needs one
    sse.rs          transport-level incremental SSE framing only
    stream.rs       protocol event lifecycle and assembly
    wire.rs         internal repeated wire constants
  tests/            black-box native conformance tests

experiments/engine-kt/adapter-<protocol>/
  src/main/kotlin/<package>/
    *Protocol.kt    configuration, HTTP descriptor, and adapter facade
    *Request.kt     typed request values and encoding
    *Response.kt    typed ordinary response/error decoding
    *Events.kt      event and delta catalogue
    Sse.kt          transport-level incremental SSE framing only
    *Stream.kt      protocol lifecycle and assembly
    ProtocolFailure.kt
    Wire.kt         internal repeated wire constants
  src/test/kotlin/<package>/
                    black-box native conformance tests
```

An adapter may omit `events.rs` only when event parsing is small enough to stay
cohesive in `stream.rs`. It must not merge request, response, SSE framing, and
lifecycle state into one codec file.

## Wire vocabulary rules

1. Public protocol alternatives use typed enums or sealed unions. Their wire
   conversion is the single source of truth for that catalogue.
2. Repeated runtime literals live in `wire.rs` or `Wire.kt`, grouped as HTTP,
   JSON fields, discriminators, and top-level collision sets.
3. Handwritten encoders, decoders, and lifecycle checks reference that internal
   vocabulary; they do not repeat media types, reserved headers, or the same
   discriminator in multiple files.
4. A literal required inside a Rust serde attribute is permitted because
   procedural-macro attributes cannot consume a runtime constant. The matching
   runtime branch must still use `wire` when it exists.
5. Event enums remain the canonical event vocabulary. `wire` must not duplicate
   a complete event catalogue merely to replace the enum's `as_str`/`wireName`.
6. One-off JSON member names stay at the point of use. Constants are for shared
   protocol vocabulary, not for obscuring every string literal.
7. Tests assert official literal values and fixtures directly; they do not
   import internal constants, so a wrong constant cannot make both production
   code and its assertion wrong together.
8. Provider endpoints, credential names, model catalogues, retry classes, and
   vendor capability defaults never enter `wire`.

## Change discipline

- Update the pinned official-SDK evidence and adapter Spec before adding a
  portable discriminator or field.
- Change Rust and Kotlin vocabulary in the same delivery slice.
- Add positive, negative, extension-preservation, and byte-boundary evidence as
  appropriate to the changed wire surface.
- Keep the adapter boundary gate green; it rejects duplicated HTTP/media
  literals outside the canonical vocabulary files.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
