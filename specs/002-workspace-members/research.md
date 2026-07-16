# Research: Workspace Members

## Decision: CLI surface

- `truss member add <name> --kind lib|bin [--member-path <REL>] [--path <DIR>]`
- `truss member list [--path <DIR>]`
- `truss member remove <name> [--path <DIR>] [--delete]`

`--path` is the workspace root and defaults to the current directory, consistent
with `truss sync` and `truss check`. `member add` additionally accepts
`--member-path` to override the default `crates/<name>` directory.

## Decision: Cargo.toml editing

- Use `toml_edit` (already a workspace dependency) to parse the root `Cargo.toml`
  into a `DocumentMut`, navigate to `workspace.members`, append the new entry, and
  deduplicate.
- Existing array order and comments are preserved; new entries are appended at
  the end. Sorting is intentionally avoided so comments attached to array items
  are not reordered.
- `toml_edit` preserves comments and formatting, unlike a serde round-trip.

## Decision: Member scaffolding

- Member `Cargo.toml` mirrors the existing default template member structure:
  ```toml
  [package]
  name = "{{ project_name }}"
  version.workspace = true
  edition.workspace = true
  authors.workspace = true
  {% if license %}license.workspace = true{% endif %}
  {% if repository %}repository.workspace = true{% endif %}

  [lints]
  workspace = true
  ```
- `src/lib.rs` contains a module-level doc comment and a placeholder function.
- `src/main.rs` contains a `fn main` placeholder that prints the project name.
- The existing `minijinja` engine can render these inline templates using the
  current rendering context.

## Decision: Path handling

- Default member path: `crates/<name>`.
- Custom `--member-path` is stored literally in `workspace.members` but validated
  to be under the workspace root before any disk write.
- For `remove --delete`, the resolved member directory is validated to be under
  the workspace root before deletion.

## Decision: Idempotency

- `add` succeeds if the member path is already in `workspace.members`, regardless
  of whether the directory exists. This covers both re-runs and orphan entries.
- Files are written only when the member directory is created by `add`. If the
  directory already exists, `add` leaves existing files untouched.

## Decision: `remove` name resolution

- `remove <NAME>` treats `NAME` as the member path recorded in
  `workspace.members`. If `NAME` contains no path separator, it defaults to
  `crates/<NAME>`.

## Alternatives considered

| Option | Why rejected |
|--------|--------------|
| Edit `Cargo.toml` with string regex | Fragile, destroys comments and formatting. |
| Use `cargo new --lib` subprocess | Introduces a runtime dependency on `cargo` and shell escaping. |
| Always overwrite existing member files on `add` | Too destructive; skip is the safer default. |
| Sort `workspace.members` after every insertion | Would move comments attached to array items; append + dedup preserves formatting. |
| Support virtual workspaces without root package | Out of scope per 002 sketch; default template always has a root package. |

## Validation

- Explored agent confirmed `toml_edit` is already in `Cargo.toml` and used in
  `truss-core/src/sync.rs` for reading workspace metadata.
- External research (cargo-generate) shows workspace member addition is a common
  scaffolder feature; it uses `cargo_util_schemas::manifest::TomlManifest`, but
  `toml_edit` is the better fit for formatting-preserving edits.
- `toml_edit` docs confirm `Array::push` and `DocumentMut` indexing exist.
