# CLAUDE.md

> See `AGENTS.md` for universal repository rules.

## Toolchain

- Rust `stable`, edition `2024`.
- Verification: `cargo nextest run`, `cargo clippy --all-features -- -D warnings`.
- Performance: `ahash`/`rustc-hash` for internal maps, `indexmap` for ordered maps,
  `minijinja` for templating, `rust-embed` for compiled-in template packs.

## Non-Inferable Critical Patterns

### Error Handling

`unwrap`, `expect`, `panic`, `todo`, `unimplemented`, `dbg!`, `Option::unwrap_or`,
and `Option::unwrap_or_default`/`Result::unwrap_or_default` are forbidden in
production code. Return typed errors with `thiserror` in `truss-core` and use
`?`-based propagation. The CLI may use `color-eyre` at the entrypoint.

### Collections

Never use `std::collections::HashMap` or `std::collections::HashSet`;
use `rustc_hash::FxHashMap`, `ahash::HashMap`, or `indexmap`.

### No Unsafe Code

`unsafe_code` is forbidden at the workspace level. Do not introduce `unsafe` blocks.

## Build Commands

```bash
cargo check --all-features
cargo clippy --all-features -- -D warnings
cargo nextest run --workspace --no-fail-fast
```

```bash
cargo run --bin truss -- new --path ./my-hft-workspace
cargo run --bin truss -- sync --path ./my-hft-workspace
cargo run --bin truss -- check --path ./my-hft-workspace
```

## Health Stack

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features -- -D warnings
cargo nextest run --workspace --no-fail-fast
cargo machete
cargo audit --deny warnings
```
