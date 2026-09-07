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
│ template sync registry git protect  │
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
| `resolve_template` | Load a template by name from the registry or embedded assets. |
| `list_templates` | List available template names and their sources. |

### `truss-cli`

The binary crate in [`crates/truss-cli/src/main.rs`](../crates/truss-cli/src/main.rs)
parses arguments, prompts for missing values in interactive mode, and calls
`truss-core`.  It is intentionally thin: no template logic lives in the CLI.

## Module map

### `template`

- Loads **embedded** templates through [`rust-embed`](https://github.com/pyrossh/rust-embed).
- Loads **directory** and **git** templates by walking a local directory or a cached clone, skipping symlinks and `.git` directories and preserving Unix file modes where supported.
- Renders files through a [`minijinja`](https://github.com/mitsuhiko/minijinja) engine capped with a fuel budget to prevent runaway templates.
- Returns a list of `TemplateFile { path, content, mode }`.

### `sync`

- Builds a `SyncContext` from workspace metadata (`Cargo.toml`) or defaults.
- `sync_workspace` writes changed rendered files to disk, creating parent directories and setting Unix permissions; unchanged and protected files are skipped.
- `check_workspace` compares rendered output against the current project and returns a list of `Drift` records.
- `plan_workspace` returns a `PlannedWrite` list with `WouldWrite`, `Unchanged`, or `SkipProtected` actions.

### `registry`

- `RegistryEntry` describes a pack source, its `Kind` (`dir`, `file`, `git`, `json`), and optional metadata.
- `Registry::load` merges an optional **system registry** and the **user registry**.
- `Registry::user_path` uses the `directories` crate, so the file lives in the user's config directory (e.g. `$XDG_CONFIG_HOME/truss/registry.json` on Linux).
- `resolve_template` checks the registry first, then falls back to embedded templates.  User entries override embedded entries with the same name.

### `git`

- `GitUrl` normalizes and expands Git hosting shorthands (`gh:`, `gl:`, `bb:`, `sr:`, bare `owner/repo`) to full URLs.
- `GitCache` clones or fetches remote repositories into the platform cache directory (for example, `$XDG_CACHE_HOME/truss/git/<name>` on Linux), checks out a requested ref, and optionally selects a `subfolder`.

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

## Dependency unification

`truss unify` moves dependencies that several workspace members declare into
`[workspace.dependencies]`, and `truss check --deps` reports the ones that are
still declared per member.

**What is scanned.** Every member named by `workspace.members`, including glob
patterns such as `crates/*`, minus anything in `workspace.exclude` -- which is
read as a pattern too, so `crates/b*` excludes every crate it matches. A member
path that leaves the workspace, through `..` or an absolute path, is rejected
rather than followed. A member named without a `Cargo.toml` is an error, as it
is for Cargo itself, so a check never reports clean for a crate nobody read.
In each member manifest the scanner reads `[dependencies]`,
`[dev-dependencies]`, `[build-dependencies]`, and the same three tables under
`[target.'cfg(...)']`. An entry is read whether it is written as
`dep = "1"`, as an inline table, or as a `[dependencies.dep]` section.

**What is skipped.** A `path` or `git` dependency names its own source, so it
has nothing to inherit and never appears as drift. So does an entry carrying
`package` or `registry`: its table key is not the crate it resolves to, so it
cannot inherit under that key.

**Broken inheritance.** A member that says `workspace = true` for a dependency
the root does not declare is reported as `missing in workspace root`. Cargo
refuses to build such a workspace, so reporting it clean would hide a manifest
that is already broken.

**Drift kinds.**

| Kind | Meaning |
| --- | --- |
| `missing in workspace root` | The root has no entry for the dependency. |
| `version mismatch` | The member and the root state different version requirements. `semver` canonicalises both first, so `1` and `^1` count as the same. |
| `features differ` | Versions agree, but inheriting would resolve a different feature set. |
| `not using workspace reference` | Versions and features agree; the member simply does not say `workspace = true`. |

A member that repeats the root version verbatim is still drift: the root can
change later without the member following.

**Unification rules.**

- A dependency is unified once it reaches the occurrence threshold (two by
  default), counted in distinct members. A member that already inherits counts
  toward that threshold; a member that declares the same dependency in two
  tables still counts once.
- Members must agree on the version requirement and on `default-features`,
  otherwise the command fails rather than picking one.
- Cargo ignores `default-features` on an inheriting member, so
  `default-features = false` moves to the workspace entry and is removed from
  the member.
- `features` and `optional` stay on the member, where Cargo still honours them.
- Bumping an existing root version is refused while another member inherits it,
  because that would silently change the version that member resolves.
- An existing root entry that names its own source, or that enables features the
  members did not ask for, is refused: inheriting it would change what those
  members resolve to.
- Every manifest is rendered before any of them is written, so a failure part
  way through cannot leave the workspace half unified. If a write itself fails,
  the manifests already written are restored.

`.truss/unify.toml` narrows the set with `allowlist` and `blocklist` arrays.
`truss check --deps` reads the same file, so the check and the command agree on
which dependencies are in scope.

## Error handling

- Library code uses `truss_core::Result<T>` and the `Error` enum.
- The CLI uses `color-eyre` to present a human-friendly error report.
- Invalid user arguments, missing templates, and path-safety violations all produce typed errors rather than panics.
