# Research: Registry CLI

## Decision: CLI surface

- Subcommands:
  - `truss templates` — list embedded + registry (alias: `truss registry list`)
  - `truss registry add <name> --source <path> --kind dir|file [--force] [--target PATH]...`
  - `truss registry remove <name>`
- Reuse clap derive nesting under `Registry` subcommand with `List|Add|Remove`,
  and a top-level `Templates` alias that only lists.

## Decision: User registry path

- Keep `directories::BaseDirs` → `config_dir()/truss/registry.json`.
- System layer remains read-only merge-first then user override.

## Decision: Dry-run

- `sync` gains `--dry-run`; `new` remains write-only.
- Core API: `plan_workspace(...) -> Result<Vec<PlannedWrite>>` where each item
  is `{ path, action: WouldWrite|SkipProtected|Unchanged }`.
- Dry-run prints planned actions and returns `Ok`.

## Decision: Protect list

- CLI: `--protect PATH` (repeatable).
- File: project `.truss/protect` (one relative path per line, `#` comments).
- Union of both; validate each with `pathsafe::validate_relative_path`.

## Alternatives considered

| Option | Why rejected |
|--------|----------------|
| Full agent-sync symlink mode | Out of scope; protect+skip is enough for P2 |
| Remote git templates | Network + caching deferred |
| Overwrite-by-default protect | Unsafe for multi-project agent rules |

## Validation

- Explore agent confirmed registry load/save/add already exist.
- Spec Kit structure bootstrapped from tooned templates + truss constitution.
