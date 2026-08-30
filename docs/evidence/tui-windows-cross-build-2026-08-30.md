# TUI Windows source-level compatibility evidence

> Recorded: 2026-08-30. This closes the Windows source/check gate only. It is
> not evidence of native linking, execution, ACL behavior, Windows Terminal,
> or ConPTY restoration.

## Candidate

| Field | Value |
|---|---|
| Garive code revision | `f0c74deb` |
| Rust toolchain | `1.98.0-aarch64-apple-darwin` |
| Rust target | `x86_64-pc-windows-msvc` |
| Host | macOS arm64 |
| Windows bindings | `windows-sys 0.61.2` |

The installed Rust target supplied the Windows standard library. The workspace
depends on `ring`, whose C build requires Windows-compatible headers and a COFF
archiver before Cargo can reach Garive source. A temporary Zig wrapper supplied
GNU Windows headers and emitted x86_64 COFF objects for that build-script-only
step. Cargo did not link or execute a Windows artifact.

## Passing gates

The complete TUI package—not an extracted module—passed all-target source
checking with `test-hooks`, including the library, shipping binary, examples,
benchmarks, unit-test targets, and integration-test targets:

```text
RUSTC="$(rustup which rustc)" \
CC_x86_64_pc_windows_msvc=/tmp/garive-zig-windows-cc \
AR_x86_64_pc_windows_msvc=/tmp/garive-zig-windows-lib \
cargo check -p garive-tui --all-targets --features test-hooks \
  --target x86_64-pc-windows-msvc
```

Result: `Finished dev profile`; exit `0`.

The same full target set then passed strict Clippy with one consistent Rustup
toolchain and a fresh target directory:

```text
PATH="$toolchain_bin:$PATH" RUSTC="$toolchain_bin/rustc" \
CARGO_TARGET_DIR=/tmp/garive-windows-clippy-target-3 \
CC_x86_64_pc_windows_msvc=/tmp/garive-zig-windows-cc \
AR_x86_64_pc_windows_msvc=/tmp/garive-zig-windows-lib \
"$toolchain_bin/cargo" clippy -p garive-tui --all-targets \
  --features test-hooks --target x86_64-pc-windows-msvc -- -D warnings
```

Result: `Finished dev profile`; exit `0`; elapsed `35.04s`.

On the native macOS reference, `cargo test -p garive-tui` also passed after
the platform split. The production HTTP/SQLite/PTY scenario passed in `93.85s`;
all persistence, terminal, snapshot, performance, and document tests passed.

## What remains open

- build and link the MSVC shipping executable with a supported Windows SDK;
- execute the persistence suite on NTFS and prove exact current-user ACLs,
  hostile-ACL rejection, reparse-point rejection, locking, and replacement;
- run the shipping executable under Windows Terminal ConPTY, including resize,
  screen-reader mode, panic/setup failure, Ctrl+C, and terminal restoration;
- retain candidate-bound logs and screenshots from that native run.

Until those gates pass, documentation may say “Windows source-level all-target
check passes,” but must not say “Windows is natively supported.”
