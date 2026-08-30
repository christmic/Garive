# TUI Linux source-level compatibility evidence

> Recorded: 2026-08-30. This closes the Linux source/check gate only. It is
> not evidence of native linking, execution, xterm PTY, tmux, or `TERM=dumb`
> behavior.

## Candidate

| Field | Value |
|---|---|
| Garive code revision | `9631bde6a05ea050ed6b9bfe0e31d09d1154ad5a` |
| Rust toolchain | `1.98.0-aarch64-apple-darwin` |
| Rust target | `x86_64-unknown-linux-gnu` |
| Host | macOS 26.6.1 arm64 |
| Cross C toolchain | Zig 0.15.2, glibc 2.17 target |

The installed Rust target supplied the Linux standard library. A temporary
wrapper removed the duplicate Cargo target argument before invoking
`zig cc -target x86_64-linux-gnu.2.17`; `zig ar` archived native dependency
objects. Cargo performed metadata/source checking and did not link or execute a
Linux artifact.

## Passing gates

The complete TUI package passed all-target checking with `test-hooks`, including
the library, shipping binary, examples, benchmarks, unit-test targets, and
integration-test targets:

```text
PATH="$toolchain_bin:$PATH" RUSTC="$toolchain_bin/rustc" \
CARGO_TARGET_DIR=/tmp/garive-linux-check-target \
CC_x86_64_unknown_linux_gnu=/tmp/garive-zig-linux-cc \
AR_x86_64_unknown_linux_gnu=/tmp/garive-zig-ar \
"$toolchain_bin/cargo" check -p garive-tui --all-targets \
  --features test-hooks --target x86_64-unknown-linux-gnu
```

Result: `Finished dev profile`; exit `0`; elapsed `30.33s`.

The same full target set passed strict Clippy:

```text
PATH="$toolchain_bin:$PATH" RUSTC="$toolchain_bin/rustc" \
CARGO_TARGET_DIR=/tmp/garive-linux-check-target \
CC_x86_64_unknown_linux_gnu=/tmp/garive-zig-linux-cc \
AR_x86_64_unknown_linux_gnu=/tmp/garive-zig-ar \
"$toolchain_bin/cargo" clippy -p garive-tui --all-targets \
  --features test-hooks --target x86_64-unknown-linux-gnu -- -D warnings
```

Result: `Finished dev profile`; exit `0`; elapsed `7.21s`.

## What remains open

- build and link the shipping executable in a native Linux environment;
- run the complete test suite, including real Runtime/SQLite behavior;
- execute the shipping binary under an xterm-compatible PTY and tmux;
- prove resize, paste/focus modes, panic/signal restoration, screen-reader
  output, `TERM=dumb`, and private-mode filesystem behavior;
- retain candidate-bound native logs and screenshots.

Until those gates pass, documentation may say “Linux source-level all-target
check passes,” but must not say “Linux is natively supported.”
