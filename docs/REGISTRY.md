# Template registry

The registry lets you share and reuse template packs without embedding them in
the `truss` binary.  You can add local directory packs, single-file packs,
remote Git repositories, or (in the future) JSON-described packs.

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
      "subfolder": null,
      "file_mode": null
    },
    "license-file": {
      "name": "license-file",
      "source": "/home/user/templates/LICENSE-MIT",
      "kind": "file",
      "targets": ["LICENSE"],
      "pointer": null,
      "subfolder": null,
      "file_mode": null
    },
    "remote-pack": {
      "name": "remote-pack",
      "source": "https://github.com/example/pack.git",
      "kind": "git",
      "targets": [],
      "pointer": "main",
      "subfolder": "templates/rust",
      "file_mode": null
    }
  }
}
```

### Entry fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | The name used with `truss new`, `truss sync`, and `truss check`. |
| `source` | yes | Path or URL to the pack source. |
| `kind` | yes | `dir`, `file`, `git`, or `json`. `json` is currently unsupported. |
| `targets` | for `file` kind | Destination path(s) for a single-file pack. |
| `pointer` | no | For `git` entries, the branch, tag, or ref to checkout. |
| `subfolder` | no | For `git` entries, the sub-directory inside the repository to use as the template root. |
| `file_mode` | no | Octal string for file permissions (e.g. `"0o755"` or `"755"`). |
| `auth_env` | no | For `git` entries, the name of an environment variable containing an HTTPS token. |
| `ssh_key` | no | For `git` entries, the path to an SSH private key file. |

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

### Add a remote Git pack

```bash
truss registry add my-pack --source https://github.com/example/pack.git --kind git
truss registry add my-pack --source gh:example/pack --kind git --pointer v1 --subfolder templates/rust
```

Supported URL forms include `https://`, `ssh://`, `git@`, and `file://` URLs, as
well as `gh:`, `gl:`, `bb:`, and `sr:` shorthands. Bare `owner/repo` is treated
as a GitHub shorthand.

### Authentication for private Git repositories

For private Git repositories, `truss` supports several authentication methods
without storing secrets in `registry.json`:

#### HTTPS with environment variable token

```bash
# Set the token in an environment variable
export MY_GITHUB_TOKEN=ghp_xxxxxxxxxxxx

# Register the repository with the environment variable name
truss registry add private-pack --source https://github.com/example/private.git --kind git --auth-env MY_GITHUB_TOKEN
```

The `auth_env` field should contain the **name** of the environment variable, not
the token itself. `truss` validates that the value does not appear to be a secret
to prevent accidental token storage.

#### Per-host environment variable

For repositories that share credentials, you can set a per-host environment
variable:

```bash
export TRUSS_AUTH_GITHUB_COM=ghp_xxxxxxxxxxxx
truss registry add private-pack --source https://github.com/example/private.git --kind git
```

The variable name format is `TRUSS_AUTH_<HOST>` with dots replaced by underscores.

#### SSH authentication

For SSH URLs, `truss` uses your SSH agent or `~/.ssh/config` by default:

```bash
truss registry add private-pack --source git@github.com:example/private.git --kind git
```

To specify an explicit SSH key:

```bash
truss registry add private-pack --source git@github.com:example/private.git --kind git --ssh-key ~/.ssh/id_rsa
```

#### Git credential helper

If you have a Git credential helper configured (e.g., `git credential-osxkeychain`,
`git credential-cache`, or a custom helper), `truss` will use it automatically
for HTTPS URLs.

#### Netrc file

`truss` reads `~/.netrc` (or the path specified by the `NETRC` environment
variable) for credentials:

```
machine github.com
login x-access-token
password ghp_xxxxxxxxxxxx
```

#### Credential precedence

When multiple sources are available, `truss` uses the following precedence
(highest to lowest):

1. Per-entry `auth_env` environment variable
2. Per-host `TRUSS_AUTH_<HOST>` environment variable
3. Git credential helper
4. Netrc file
5. SSH agent/config (for SSH URLs)
6. Explicit `ssh_key` path (for SSH URLs)

If no credentials are found for a private repository, `truss` fails with a clear
error message and does not fall back to an unauthenticated attempt.

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
- the source path exists for `dir` and `file` kinds,
- `dir` sources are directories,
- `file` sources are files and have at least one `--target`,
- `git` sources are valid Git URLs or shorthands,
- `git` entries do not use `--target`,
- `git` `subfolder` values are relative and contain no path traversal,
- `json` sources are rejected (not yet implemented).

If validation fails, the entry is not written to the registry.

## Best practices

- Use **absolute paths** for `source` so the entry works from any working directory.
- Keep team packs in version control and register them from a checked-out path.
- Use `--dry-run` with `truss sync` before applying a pack update.
- Protect files that should survive sync (`--protect` or `.truss/protect`).
