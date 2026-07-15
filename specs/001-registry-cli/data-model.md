# Data Model: Registry CLI

## RegistryEntry

| Field | Type | Rules |
|-------|------|-------|
| name | string | non-empty; registry key |
| source | string | filesystem path; must exist on add |
| kind | enum | `dir`, `file`, `json` |
| targets | string[] | required non-empty for `file` |
| pointer | string? | reserved; optional |
| file_mode | string? | octal `0o644` or decimal |
| dir_mode | string? | reserved |

## Registry

- Ordered map `name → RegistryEntry` (IndexMap).
- Load: system file (if present) then user file (overrides keys).
- Save: user file only, pretty JSON.

## ProtectList

- Set of relative UTF-8 paths.
- Sources: CLI flags ∪ `.truss/protect` lines.
- Empty set is valid.

## PlannedWrite

| Field | Type |
|-------|------|
| path | relative string |
| action | `WouldWrite` \| `SkipProtected` \| `Unchanged` |

## State transitions

```text
add:  validate → insert/replace(if force) → save user registry
remove: require key → drop → save
sync dry-run: render → classify each file → print plan → no disk write
sync: render → skip protected → write rest
```
