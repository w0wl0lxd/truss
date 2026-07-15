# CLI Contract: Registry CLI

## `truss templates`

Lists embedded and registry templates.

```
truss templates
```

Exit 0. One entry per line:

```
NAME  KIND  SOURCE
default  embedded  (built-in)
team  dir  /home/u/templates/team
```

## `truss registry list`

Alias of templates list for registry + embedded.

## `truss registry add`

```
truss registry add <NAME> --source <PATH> --kind <dir|file> [--force]
                         [--target <REL>]...
```

- Creates config dir as needed.
- Exit 0 on success; non-zero on validation failure.

## `truss registry remove`

```
truss registry remove <NAME>
```

- Only removes user-layer keys (after save, system-only names may reappear on load).
- Exit non-zero if name absent from user file.

## `truss sync` additions

```
truss sync [--path DIR] [--template NAME] [--dry-run] [--protect REL]...
```

- `--dry-run`: no writes; print planned actions.
- `--protect`: skip those relative destinations (also reads `.truss/protect`).

## `truss new` / `check`

Unchanged flags; templates continue to resolve registry → embedded.
