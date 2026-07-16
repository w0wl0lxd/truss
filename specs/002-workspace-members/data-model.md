# Data Model: Workspace Members

## Inputs

### `MemberAddRequest` (internal)

| Field | Type | Rules |
|-------|------|-------|
| name | string | non-empty; used as default crate package name |
| kind | enum | `lib` or `bin` |
| workspace_root | path | absolute directory containing root `Cargo.toml`; defaults to current directory |
| member_path | string | relative to workspace root; defaults to `crates/<name>` |

### `MemberRemoveRequest` (internal)

| Field | Type | Rules |
|-------|------|-------|
| name | string | member path as recorded in `workspace.members`; basename defaults to `crates/<name>` |
| workspace_root | path | absolute directory containing root `Cargo.toml`; defaults to current directory |
| delete | bool | if true, delete the member directory after removing the entry |

## State

### Root `Cargo.toml` `[workspace]` table

- `members` is an array of relative paths (strings).
- New entries are appended to the end and duplicates are removed.
- Existing order, comments, and formatting are preserved.
- `resolver` and `workspace.package` are untouched.

### Member crate files

- `Cargo.toml` — generated from inline template.
- `src/lib.rs` or `src/main.rs` — generated from inline template.

## State transitions

```text
member add:
  validate workspace_root (root Cargo.toml exists and has [workspace])
  resolve member_path (default or --member-path)
  ensure member_path is under workspace_root
  if member_path not in workspace.members:
    append to workspace.members array
    deduplicate
    write Cargo.toml back
  if member directory does not exist:
    create directory
    render and write member Cargo.toml
    render and write src/lib.rs or src/main.rs
  return Ok

member list:
  parse root Cargo.toml at workspace_root
  if [workspace] missing: error
  print workspace.members entries (or empty)

member remove:
  parse root Cargo.toml at workspace_root
  if [workspace] or members missing: error
  resolve requested member path (exact, or crates/<name> if no separator)
  remove matching entry from workspace.members
  write Cargo.toml back
  if --delete:
    ensure member directory is under workspace_root
    remove directory
```
