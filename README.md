# truss

Rust project scaffolder with template sync and local registries (`truss new` / `sync` / `check`).

## Quick start

```bash
just setup-hooks   # once per clone
cargo run --bin truss -- new my-project
```

## Development

```bash
just validate      # fmt + check + clippy + nextest
```

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for Conventional Commits, no-AI-attribution
policy, PR hygiene, and local hooks.
