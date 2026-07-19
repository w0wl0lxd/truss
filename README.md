<!--
  GENERATED FILE - DO NOT EDIT DIRECTLY.
  Source: docs/README.template.md
  Regenerate with: just docs
-->

# truss

<!-- doc-gen: truss_description -->
Rust project scaffolder with template sync and local registries
<!-- /doc-gen: truss_description -->

<!-- doc-gen: truss_version -->
0.1.0
<!-- /doc-gen: truss_version -->
| Rust edition: <!-- doc-gen: truss_edition -->2024<!-- /doc-gen: truss_edition -->
| Minimum rustc: <!-- doc-gen: truss_rust_version -->1.85.0<!-- /doc-gen: truss_rust_version -->
| License: <!-- doc-gen: truss_license -->MIT<!-- /doc-gen: truss_license -->

`truss` is a small, opinionated CLI for bootstrapping and maintaining Rust
workspaces from reusable template packs.  It ships with a few embedded packs,
supports a local template registry, and can diff a generated project against its
template to detect drift.

## Features

- **Scaffold** new workspaces with `truss new`.
- **Sync** an existing project back to its template with `truss sync`.
- **Check** for drift between a project and its template with `truss check`.
- **Protect** local files from being overwritten during sync.
- **Template registry** for sharing custom packs inside a team or organization.
- **Path-safety guards** against absolute paths and `..` traversal in templates.
- **Offline** rendering using [MiniJinja](https://github.com/mitsuhiko/minijinja).

## Quick start

Install from source:

```bash
cargo install --path crates/truss-cli
```

Create a workspace from the default pack:

```bash
truss new my-project
cd my-project
cargo check
```

List available packs and check for template drift:

```bash
truss templates
truss check --path my-project --template default
```

## Workspace crates

<!-- doc-gen: crates_table -->
| Crate | Description |
|-------|-------------|
| `truss-core` | Core scaffolding library for truss |
| `truss-cli` | CLI for truss |
<!-- /doc-gen: crates_table -->

## Embedded template packs

<!-- doc-gen: embedded_packs -->
| Pack | Kind | Source |
|------|------|--------|
| `agent-rules` | embedded | (built-in) |
| `default` | embedded | (built-in) |
| `monorepo` | embedded | (built-in) |
| `spec-kit` | embedded | (built-in) |
<!-- /doc-gen: embedded_packs -->

## CLI reference

<!-- doc-gen: cli_reference -->
## Available commands

| Command | Description |
|---------|-------------|
| [`truss new`](docs/CLI.md#truss-new) | Create a new project from a template |
| [`truss sync`](docs/CLI.md#truss-sync) | Sync a project to a template |
| [`truss check`](docs/CLI.md#truss-check) | Check for drift against a template |
| [`truss update`](docs/CLI.md#truss-update) | Apply upstream template changes with a 3-way merge |
| [`truss extract`](docs/CLI.md#truss-extract) | Reverse-scaffold an existing project into a reusable pack |
| [`truss define`](docs/CLI.md#truss-define) | List variables expected by a template pack |
| [`truss templates`](docs/CLI.md#truss-templates) | List embedded and registry templates |
| [`truss types`](docs/CLI.md#truss-types) | List and inspect project-type presets |
| [`truss registry`](docs/CLI.md#truss-registry) | Manage the local template registry |
| [`truss member`](docs/CLI.md#truss-member) | Manage workspace members |

See [docs/CLI.md](docs/CLI.md) for the complete command reference.
<!-- /doc-gen: cli_reference -->

## Documentation

- [Architecture and data flow](docs/ARCHITECTURE.md)
- [Writing template packs](docs/TEMPLATES.md)
- [Using the template registry](docs/REGISTRY.md)
- [Full CLI reference](docs/CLI.md)
- [Contributing](CONTRIBUTING.md)

## Development

```bash
just validate   # fmt + check + clippy + test
just secrets    # gitleaks + ripsecrets
just docs       # regenerate README.md and docs/CLI.md
just docs-check # verify docs are up to date
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

<!-- doc-gen: truss_license -->
MIT
<!-- /doc-gen: truss_license -->
