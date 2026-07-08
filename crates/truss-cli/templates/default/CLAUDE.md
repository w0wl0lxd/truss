# CLAUDE.md

> See `AGENTS.md` for universal repository rules for `{{ project_name }}`.

## Toolchain

- Rust `stable`, edition `{{ edition }}`.
- Verification: `cargo nextest run`, `cargo clippy --all-features -- -D warnings`.

## Build Commands

```bash
cargo check --all-features
cargo clippy --all-features -- -D warnings
cargo nextest run --workspace --no-fail-fast
```
