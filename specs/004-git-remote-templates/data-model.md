# Data Model: Git-based Remote Templates

## Entities

### GitRegistryEntry

A registry entry that describes a remote Git template pack.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | yes | The key used with `truss new`, `sync`, and `check`. |
| `source` | String | yes | Git repository URL or shorthand (`https://...`, `gh:owner/repo`, etc.). |
| `kind` | Enum | yes | Must be `"git"` for this feature. |
| `pointer` | Option<String> | no | Branch, tag, or commit SHA to resolve. Defaults to the remote default branch when absent. |
| `subfolder` | Option<String> | no | Relative path inside the repository that acts as the template root. Must be a normalized relative path. |
| `file_mode` | Option<String> | no | Octal file mode applied to all generated files, as in `dir` and `file` entries. |
| `targets` | Vec<String> | no | Not used for `git` entries; must be empty. |

### GitCache

A local on-disk cache that holds resolved remote templates.

| Field | Type | Description |
|-------|------|-------------|
| `root` | PathBuf | Platform cache directory root, e.g. `$XDG_CACHE_HOME/truss/git`. |
| `key` | String | Stable cache key derived from the registry entry name (sanitized for the filesystem). |
| `repo_path` | PathBuf | Directory containing the cloned worktree. |
| `resolved_ref` | Option<String> | The ref that was last successfully resolved and checked out. |

## Relationships

- A `Registry` contains zero or more `RegistryEntry` records, some of which may be `GitRegistryEntry` records.
- A `GitRegistryEntry` maps to exactly one `GitCache` record keyed by its entry name.
- A `GitCache` worktree is consumed by the existing `Template::from_directory` loader after VCS metadata is excluded.
- The output of the cache resolution is a `Template` value that is indistinguishable from a local `dir` template for downstream operations (`sync`, `check`, `new`).

## Constraints

- `source` must be a valid Git URL or a supported shorthand. Local filesystem paths are not accepted when `kind = "git"`.
- `subfolder` must be a normalized relative path: no `..`, no absolute components, and no backslashes on non-Windows platforms.
- `pointer` may be any valid git ref string. Missing refs must fail before any files are written.
- The cache key must be filesystem-safe and stable across runs.
