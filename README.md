# truss

Rust project scaffolder with template sync and local registries (`truss new` / `sync` / `check`).

## Quick start

```bash
just setup-hooks   # once per clone
cargo run --bin truss -- new my-project
cargo run --bin truss -- templates
cargo run --bin truss -- registry add my-pack --source ./packs/team --kind dir
cargo run --bin truss -- sync --path ./my-project --template my-pack --dry-run
cargo run --bin truss -- sync --path ./my-project --template my-pack --protect AGENTS.local.md
```

## Development

```bash
just validate      # secrets + fmt + check + clippy + nextest
```

Feature specs: `specs/001-registry-cli/`.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for Conventional Commits, no-AI-attribution
policy, PR hygiene, and local hooks.
