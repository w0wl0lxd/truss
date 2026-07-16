# CLI Contract: Workspace Members

## `truss member add <NAME> --kind <lib|bin> [OPTIONS]`

```
truss member add <NAME> --kind <KIND> [--member-path <REL>] [--path <DIR>]
```

- `NAME` is the crate name; used as the package name and the default member path
  `crates/<NAME>`.
- `--kind` is required and accepts `lib` or `bin`.
- `--member-path` is optional and sets the relative path stored in
  `workspace.members`.
- `--path` is optional and points to the workspace root; it defaults to the
  current directory.
- Exit 0 on success; non-zero on validation failure or missing `[workspace]`.

Behavior:

- Appends `--member-path` (or `crates/<NAME>`) to `workspace.members` in the root
  `Cargo.toml` if not present.
- Creates the member directory if it does not exist.
- Writes a member `Cargo.toml` and `src/lib.rs` or `src/main.rs` only when the
  directory is newly created.
- Re-running with an existing member is a no-op for `workspace.members` and
  leaves existing files untouched.

## `truss member list [OPTIONS]`

```
truss member list [--path <DIR>]
```

- `--path` is optional and points to the workspace root; it defaults to the
  current directory.
- Prints one member path per line from the root `Cargo.toml` `workspace.members`.
- If `workspace.members` is empty, prints nothing and exits 0.
- Exit non-zero if the root `Cargo.toml` has no `[workspace]` table.

## `truss member remove <NAME> [OPTIONS]`

```
truss member remove <NAME> [--path <DIR>] [--delete]
```

- `NAME` is the member path as recorded in `workspace.members`. If `NAME`
  contains no path separator, it defaults to `crates/<NAME>`.
- `--path` is optional and points to the workspace root; it defaults to the
  current directory.
- `--delete` also removes the member directory; without it, the directory is
  preserved.
- Exit non-zero if the member is not found or the resolved path escapes the
  project root.
