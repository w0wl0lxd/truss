# Template registry

The registry lets you share and reuse template packs without embedding them in
the `truss` binary.  You can add local directory packs, single-file packs, or
(in the future) JSON-described packs.

## User registry path

The user registry is stored in the platform config directory under `truss/registry.json`:

- Linux: `$XDG_CONFIG_HOME/truss/registry.json` (falls back to `~/.config`)
- macOS: `~/Library/Application Support/truss/registry.json`
- Windows: `%APPDATA%/truss/registry.json`

`truss` uses the `directories` crate to locate this path.

## Registry file format

`registry.json` is a single JSON object with an `entries` map:

```json
{
  "entries": {
    "team": {
      "name": "team",
      "source": "/home/user/packs/team",
      "kind": "dir",
      "targets": [],
      "pointer": null,
      "file_mode": null
    },
    "license-file": {
      "name": "license-file",
      "source": "/home/user/templates/LICENSE-MIT",
      "kind": "file",
      "targets": ["LICENSE"],
      "pointer": null,
      "file_mode": null
    }
  }
}
```

### Entry fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | The name used with `truss new`, `truss sync`, and `truss check`. |
| `source` | yes | Absolute path to the pack source. |
| `kind` | yes | `dir`, `file`, or `json`. `json` is currently unsupported. |
| `targets` | for `file` kind | Destination path(s) for a single-file pack. |
| `pointer` | no | Reserved for future use. |
| `file_mode` | no | Octal string for file permissions (e.g. `"0o755"`). |

## Managing the registry

### List all available templates

```bash
truss templates
# or
truss registry list
```

### Add a directory pack

```bash
truss registry add my-pack --source /absolute/path/to/my-pack --kind dir
```

Use `--force` to replace an existing entry:

```bash
truss registry add my-pack --source /absolute/path/to/my-pack-v2 --kind dir --force
```

### Add a single-file pack

```bash
truss registry add mit-license --source /absolute/path/to/LICENSE --kind file --target LICENSE
```

### Remove an entry

```bash
truss registry remove my-pack
```

## System registry

Organizations can provide a site-wide registry that is read but not modified by
`truss`.  The following paths are checked in order:

1. `TRUSS_SYSTEM_REGISTRY` environment variable.
2. `/etc/truss/registry.json`
3. `/usr/local/etc/truss/registry.json`

The first one found is loaded.  Missing files are ignored.

## Resolution order

`truss` builds a merged registry from the optional system registry and the user
registry; user entries override system entries with the same name.  When a
template name is requested, the merged registry is checked first and falls back
to embedded packs.  The effective precedence is:

1. User registry entries (highest).
2. System registry entries.
3. Embedded templates (fallback).

For example, a user entry named `default` would replace the built-in `default`
pack.

## Validation

When you add an entry, `truss` validates that:

- the name is not empty,
- the source path exists,
- `dir` sources are directories,
- `file` sources are files and have at least one `--target`,
- `json` sources are rejected (not yet implemented).

If validation fails, the entry is not written to the registry.

## Best practices

- Use **absolute paths** for `source` so the entry works from any working directory.
- Keep team packs in version control and register them from a checked-out path.
- Use `--dry-run` with `truss sync` before applying a pack update.
- Protect files that should survive sync (`--protect` or `.truss/protect`).
