# truss

`truss` scaffolds Rust workspaces from template packs and keeps them in sync.
Use embedded packs or register local packs for `new`, `sync`, and `check` workflows.

## Quick start

```bash
# Create a Rust workspace from the default pack.
cargo run --bin truss -- new my-project

# See embedded and locally registered packs.
cargo run --bin truss -- templates

# Register a local template pack.
cargo run --bin truss -- registry add my-pack --source ./packs/team --kind dir

# Preview and apply template changes.
cargo run --bin truss -- sync --path ./my-project --template my-pack --dry-run
cargo run --bin truss -- sync --path ./my-project --template my-pack

# Check for drift and protect local files during sync.
cargo run --bin truss -- check --path ./my-project --template my-pack
cargo run --bin truss -- sync --path ./my-project --template my-pack --protect AGENTS.local.md
```

### Embedded packs

- `default` — Rust workspace
- `spec-kit`
- `agent-rules`

## Development

```bash
just validate
```

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for contribution guidelines and local hooks.
