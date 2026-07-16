# Architecture

`truss` is split into a reusable core library and a thin command-line frontend.

```text
┌─────────────────────────────────────┐
│           truss-cli                 │
│  clap / inquire / color-eyre        │
└──────────────┬──────────────────────┘
               │ API calls
┌──────────────▼──────────────────────┐
│           truss-core                │
│  template  sync  registry  protect    │
│  pathsafe  error                    │
└─────────────────────────────────────┘
```

## Crate responsibilities

### `truss-core`

The library crate contains all domain logic and is designed to be embeddable in
other tools or tests.  It exposes a small public API through
[`crates/truss-core/src/lib.rs`](../crates/truss-core/src/lib.rs):

| Function | Purpose |
|----------|---------|
| `new_workspace` | Render a template into a fresh project directory. |
| `sync_workspace` / `sync_workspace_with` | Re-apply a template to an existing project. |
| `check_workspace` | Compare a project against a rendered template and report drift. |
| `plan_workspace` | Produce a write plan without modifying files. |
| `resolve_template` / `list_templates` | Load templates from the registry or from embedded assets. |

### `truss-cli`

The binary crate in [`crates/truss-cli/src/main.rs`](../crates/truss-cli/src/main.rs)
parses arguments, prompts for missing values in interactive mode, and calls
`truss-core`.  It is intentionally thin: no template logic lives in the CLI.

## Module map

### `template`

- Loads **embedded** templates through [`rust-embed`](https://github.com/pyrossh/rust-embed).
- Loads **directory** templates by walking a local directory, skipping symlinks and preserving file modes.
- Renders files through a [`minijinja`](https://github.com/mitsuhiko/minijinja) engine capped with a fuel budget to prevent runaway templates.
- Returns a list of `TemplateFile { path, content, mode }`.

### `sync`

- Builds a `SyncContext` from workspace metadata (`Cargo.toml`) or defaults.
- `sync_workspace` writes rendered files to disk, creating parent directories and setting Unix permissions.
- `check_workspace` compares rendered output against the current project and returns a list of `Drift` records.
- `plan_workspace` returns a `PlannedWrite` list with `WouldWrite`, `Unchanged`, or `SkipProtected` actions.

### `registry`

- `RegistryEntry` describes a pack source, its `Kind` (`dir`, `file`, `json`), and optional metadata.
- `Registry::load` merges an optional **system registry** and the **user registry**.
- `Registry::user_path` uses the `directories` crate, so the file lives in the user's config directory (e.g. `$XDG_CONFIG_HOME/truss/registry.json` on Linux).
- `resolve_template` checks the registry first, then falls back to embedded templates.  User entries override embedded entries with the same name.

### `protect`

- `ProtectList` holds relative paths that sync must not overwrite.
- Paths can come from the CLI (`--protect`) or from `.truss/protect` inside the project.
- Protected files appear as `SkipProtected` in plans and are left untouched during sync.

### `pathsafe`

- `validate_relative_path` rejects empty, absolute, and `..`-containing paths.
- `ensure_under_root` canonicalizes a destination and confirms it still lives under the project root.
- `is_symlink` returns true for any symlink, including dangling ones, so sync never writes through or over a link.

### `error`

- A typed `thiserror` enum covers I/O, template, TOML, JSON, validation, and argument errors.
- All fallible functions return `truss_core::Result<T>`.

## Data flow

### `truss new my-project`

```text
CLI prompts / args  ->  SyncContext
                        │
                        ▼
               resolve_template("default")
                        │
                        ▼
         Template::load("default") from rust-embed
                        │
                        ▼
          render(ctx, engine) -> Vec<TemplateFile>
                        │
                        ▼
           sync_workspace(path, template, ctx)
                        │
                        ▼
                  files on disk
```

### `truss sync --path my-project --template default`

```text
CLI args  ->  SyncContext::from_workspace(my-project/Cargo.toml)
                       │
                       ▼
           resolve_template("default")
                       │
                       ▼
        render template -> compare with on-disk files
                       │
                       ▼
           write changed files; skip protected paths
```

### `truss check --path my-project --template default`

Same flow as `sync`, but instead of writing files `check_workspace` collects any
mismatches and returns them as `Drift` records.  If the list is non-empty the CLI
exits with an error.

## Registry layering

Templates are resolved in this order:

1. **User registry** (`$XDG_CONFIG_HOME/truss/registry.json` or platform equivalent).
2. **System registry** if `TRUSS_SYSTEM_REGISTRY` is set, or `/etc/truss/registry.json`, or `/usr/local/etc/truss/registry.json`.
3. **Embedded templates** baked into the binary with `rust-embed`.

A higher layer overrides a lower layer when names collide.  `truss templates`
shows the merged view.

## Path safety

Template packs are treated as untrusted input:

- Absolute template paths and `..` components are rejected before rendering.
- Before writing, the destination is canonicalized and checked against the project root.
- Symlinks (including dangling links) are never followed or overwritten.
- Empty paths are rejected.

## Error handling

- Library code uses `truss_core::Result<T>` and the `Error` enum.
- The CLI uses `color-eyre` to present a human-friendly error report.
- Invalid user arguments, missing templates, and path-safety violations all produce typed errors rather than panics.
