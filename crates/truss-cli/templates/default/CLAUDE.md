# CLAUDE.md

## Toolchain

- Rust `stable`, edition `2024`.
- Verification: `cargo nextest run`, `cargo clippy --all-features -- -D warnings`.

## Build Commands

```bash
cargo check --all-features
cargo clippy --all-features -- -D warnings
cargo nextest run --workspace --no-fail-fast
```
