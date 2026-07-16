# Feature Specification: Workspace Members

**Feature Branch**: `002-workspace-members`

**Created**: 2026-07-16

**Status**: Draft

**Input**: After `truss new` scaffolds a workspace shell, developers currently have to
hand-edit `Cargo.toml` to add new crates. Enable `truss member add` to append a
member to the workspace, scaffold the crate directory, and keep `Cargo.toml`
formatting intact. Also support `member list` and `member remove` for basic
workspace maintenance.

## Clarifications

### Session 2026-07-16

- **Q1**: What do `member` subcommands use for the workspace root and for the member path?
  - **A1**: `--path` is the workspace root and defaults to the current directory
    (consistent with `truss sync` / `truss check`). `member add` uses
    `--member-path` to override the default member directory `crates/<name>`.
- **Q2**: What happens on `member add` re-runs or partial directories?
  - **A2**: `add` only writes `Cargo.toml` and `src/{lib,main}.rs` when it creates
    the member directory. If the directory already exists, workspace entry
    deduplication still occurs, but existing files are not read, written, or
    overwritten.
- **Q3**: How are `member remove` names resolved?
  - **A3**: `remove <NAME>` treats `NAME` as the member path recorded in
    `workspace.members`. If `NAME` contains no path separator, it defaults to
    `crates/<NAME>`.
- **Q4**: Should `workspace.members` be sorted after insertion?
  - **A4**: No. The array preserves the existing user order and only deduplicates
    new entries, keeping comments attached to array items intact.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Add a library crate (Priority: P1)

A developer runs `truss member add <name> --kind lib` from the workspace root
(or points to it with `--path`). The member path is appended to the root
`Cargo.toml` `workspace.members` array, and a new crate directory is created with
a minimal `Cargo.toml` and `src/lib.rs` containing the project name.

**Why this priority**: This is the core value of the phase — removing the manual
editing step that every new workspace member requires.

**Independent Test**: Create a project with `truss new`, run `truss member add mylib
--kind lib`, assert `Cargo.toml` contains `"crates/mylib"` and
`crates/mylib/src/lib.rs` exists and compiles.

**Acceptance Scenarios**:

1. **Given** a valid workspace project, **when** `truss member add mylib --kind lib`
   runs, **then** `workspace.members` in the root `Cargo.toml` includes
   `"crates/mylib"`, and the crate directory contains a valid `Cargo.toml` and
   `src/lib.rs`.
2. **Given** a workspace where `crates/mylib` already exists in `workspace.members`,
   **when** the same `member add` runs again, **then** it exits successfully and
   does not duplicate the member.
3. **Given** a workspace where `crates/mylib` already exists on disk but not in
   `workspace.members`, **when** `member add` runs, **then** it adds the member to
   `workspace.members` and does not overwrite the existing directory.
4. **Given** a workspace where `crates/mylib` does not exist and `workspace.members`
   already contains `crates/mylib` (orphan entry), **when** `member add` runs,
   **then** it scaffolds the directory and does not duplicate the member.

---

### User Story 2 — Add a binary crate (Priority: P1)

A developer runs `truss member add <name> --kind bin` to add a binary crate. The
scaffold produces `src/main.rs` instead of `src/lib.rs`.

**Why this priority**: Workspaces commonly contain both libraries and binaries;
parity is expected.

**Independent Test**: `truss member add mybin --kind bin` creates
`crates/mybin/src/main.rs` with a `fn main`.

**Acceptance Scenarios**:

1. **Given** a valid workspace, **when** `truss member add mybin --kind bin` runs,
   **then** `crates/mybin/Cargo.toml` and `crates/mybin/src/main.rs` are created.
2. **Given** a binary crate scaffold, **when** `cargo check` runs in the member
   directory, **then** it compiles without errors.

---

### User Story 3 — List workspace members (Priority: P2)

A developer runs `truss member list` to see the members declared in the root
`Cargo.toml`.

**Why this priority**: Small convenience that pairs with add/remove and is cheap
to implement.

**Independent Test**: After adding two members, `truss member list` prints both.

**Acceptance Scenarios**:

1. **Given** a workspace with two members, **when** `truss member list` runs,
   **then** both member paths are printed, one per line.
2. **Given** a `Cargo.toml` with no `[workspace]` table, **when** `truss member list`
   runs, **then** it fails with a clear message.

---

### User Story 4 — Remove a workspace member (Priority: P2)

A developer runs `truss member remove <name>` to remove a member from
`workspace.members`. By default the directory is left untouched; `--delete`
removes it.

**Why this priority**: Maintenance; avoids manual Cargo.toml editing when retiring
a crate.

**Independent Test**: Add a member, remove it, assert it no longer appears in
`workspace.members` and the directory still exists unless `--delete` is passed.

**Acceptance Scenarios**:

1. **Given** an existing member `crates/old`, **when** `truss member remove old`
   runs, **then** `workspace.members` no longer contains `crates/old` and the
   directory is preserved.
2. **Given** an existing member `crates/old`, **when** `truss member remove old
   --delete` runs, **then** the member is removed from `workspace.members` and the
   directory is deleted.
3. **Given** a member name that is not in `workspace.members`, **when** `remove` runs,
   **then** it fails clearly.

## Edge Cases

- Root `Cargo.toml` has no `[workspace]` table: fail closed.
- `workspace.members` is missing but `[workspace]` exists: create the `members`
  array with the new member.
- Member path already in `workspace.members`: no-op (idempotent).
- Member path escapes the project root: reject with a clear error.
- Member name collides with an existing file (not directory) at the target path:
  fail.
- `--kind` invalid or missing: fail at CLI parse time.
- `--path` is the workspace root and defaults to the current directory.
- `--member-path` overrides the default member directory `crates/<name>` and is
  stored literally in `workspace.members`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support `truss member add <name> --kind lib|bin`
  with an optional `--path` for the workspace root (default: current directory)
  and an optional `--member-path` for the member directory (default:
  `crates/<name>`).
- **FR-002**: System MUST append the member path to `workspace.members` in the
  root `Cargo.toml` without duplicate entries, preserving the existing order and
  formatting.
- **FR-003**: System MUST scaffold a minimal `Cargo.toml` and `src/lib.rs` or
  `src/main.rs` only when it creates the member directory.
- **FR-004**: System MUST fail if the root `Cargo.toml` does not contain a
  `[workspace]` table.
- **FR-005**: System MUST make `member add` idempotent (re-running on an existing
  member succeeds without changes to `workspace.members` and without touching
  existing files).
- **FR-006**: System MUST support `truss member list [--path <workspace-root>]`
  printing workspace members.
- **FR-007**: System MUST support `truss member remove <name> [--path
  <workspace-root>]` removing the member from `workspace.members`.
- **FR-008**: System MUST refuse to remove a member whose resolved path is outside
  the project root.
- **FR-009**: System MUST preserve comments and formatting in `Cargo.toml` when
  editing.

### Key Entities

- **MemberAddRequest**: name, kind (`lib` | `bin`), workspace root, member path
  (relative to root), project metadata from the rendering context.
- **MemberCargoToml**: minimal `Cargo.toml` for a member referencing
  `version.workspace`, `edition.workspace`, `license.workspace` (if set),
  `repository.workspace` (if set), and `lints.workspace`.
- **MemberSourceFile**: `src/lib.rs` or `src/main.rs` with a project-name comment.
- **MemberRemoveRequest**: name, workspace root, delete flag.

### Success Criteria *(mandatory)*

- **SC-001**: A developer can add a lib and a bin crate to a fresh `truss new`
  project in under one minute.
- **SC-002**: Re-running `member add` for an existing member does not change
  `workspace.members` or existing files.
- **SC-003**: `cargo check` passes in the workspace after adding a lib and a bin.
- **SC-004**: `member list` and `member remove` work as documented and are covered
  by tests.

## Assumptions

- The workspace root is identified by the presence of `[workspace]` in the root
  `Cargo.toml`.
- `member` subcommands accept `--path` for the workspace root and default to the
  current directory, consistent with `truss sync` and `truss check`.
- `member add` uses `--member-path` to override the default member directory
  `crates/<name>`.
- Member crates live under `crates/<name>` by default, but arbitrary relative
  paths are allowed.
- The project metadata (project name, author, license, edition, repository)
  used when scaffolding the workspace is reused for the member `Cargo.toml`.
- No virtual workspaces without a root package are supported (matches the default
  template).

## Out of Scope

- Publishing crates.
- Feature-flag graphs or dependency wiring between members.
- Virtual workspaces without a root package beyond the current default template.
- Auto-fixing `workspace.dependencies` or `workspace.lints`.
- Renaming members (use remove + add).
