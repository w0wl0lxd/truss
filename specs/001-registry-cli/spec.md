# Feature Specification: Registry CLI

**Feature Branch**: `001-registry-cli`

**Created**: 2026-07-15

**Status**: Draft

**Input**: Expose truss's existing local registry so developers can list embedded
templates, register custom directory/file packs, remove entries, and use them
with `new` / `sync` / `check` without forking the tool. Include dry-run and
protected-path options so multi-project agent-rules sync cannot clobber local edits.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - List available templates (Priority: P1)

A developer runs `truss templates` (or `truss registry list`) and sees every
embedded pack plus every custom registry entry with name, kind, and source.

**Why this priority**: Without discoverability, registry features are invisible.

**Independent Test**: Empty user registry still lists `default`, `nixdex`,
`spec-kit`, and `agent-rules`. After adding a custom entry, it appears in the list.

**Acceptance Scenarios**:

1. **Given** a fresh machine with no user registry file, **when** the user runs
   the list command, **then** all embedded template names are printed.
2. **Given** a user registry with one directory entry `team-rules`, **when** they
   list templates, **then** output includes both embedded packs and `team-rules`
   with its source path and kind.

---

### User Story 2 - Register a custom template (Priority: P1)

A developer points truss at a local directory pack (`truss registry add NAME
--source PATH --kind dir`) so subsequent `truss new --template NAME` and
`truss sync --template NAME` resolve it.

**Why this priority**: Unlocks team-specific scaffolds without modifying the binary.

**Independent Test**: Add a temp directory containing `AGENTS.md`, then
`truss new --template that-name --path /tmp/demo demo` creates the file.

**Acceptance Scenarios**:

1. **Given** a valid directory, **when** add runs with kind `dir`, **then**
   `~/.config/truss/registry.json` contains the entry and a later load finds it.
2. **Given** a missing path, **when** add runs, **then** the command fails with
   a clear path error and does not write a partial registry.
3. **Given** an existing name, **when** add runs without force, **then** the
   command fails; with `--force`, the entry is replaced.

---

### User Story 3 - Remove a registry entry (Priority: P2)

A developer removes a stale custom entry so it no longer appears in selection.

**Why this priority**: Maintenance; not required for first successful custom template use.

**Independent Test**: Add then remove; list no longer shows the custom name;
embedded names remain.

**Acceptance Scenarios**:

1. **Given** an existing custom entry, **when** remove succeeds, **then** the
   registry file no longer contains that key.
2. **Given** a name that does not exist, **when** remove runs, **then** the
   command fails clearly without corrupting the registry.

---

### User Story 4 - Dry-run sync and protect paths (Priority: P2)

A developer runs `truss sync --dry-run` to preview which files would change, and
marks paths as protected so sync never overwrites them (e.g. local secrets or
hand-edited `AGENTS.local.md`).

**Why this priority**: Prevents silent clobber when adopting agent-rules packs across
many repos.

**Independent Test**: Sync dry-run exits 0 and prints planned writes without
modifying disk; protected path remains unchanged after a real sync.

**Acceptance Scenarios**:

1. **Given** a project that would receive updates, **when** sync uses `--dry-run`,
   **then** no file content changes and planned paths are listed.
2. **Given** a protect list containing `AGENTS.local.md`, **when** sync runs,
   **then** that file is skipped while other template files still write.

---

### Edge Cases

- Registry file missing: treat as empty user registry (not an error for list/load).
- Registry file corrupt JSON: fail with parse error; do not overwrite blindly.
- Kind `file` requires at least one target path.
- Kind `json` remains unsupported and returns a typed error.
- Concurrent writes to registry are last-write-wins; no multi-process lock required
  in this phase.
- Protect paths that are absolute or contain `..` are rejected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST list all embedded template names.
- **FR-002**: System MUST list all user/system registry entries with name, kind, source.
- **FR-003**: System MUST allow adding a directory registry entry with validated
  existing source path.
- **FR-004**: System MUST allow removing a user registry entry by name.
- **FR-005**: System MUST persist user registry under the platform config dir
  (`truss/registry.json`).
- **FR-006**: System MUST refuse to add entries with empty names or nonexistent sources.
- **FR-007**: System MUST support `truss sync --dry-run` that reports planned file
  operations without writing.
- **FR-008**: System MUST support a protect list (CLI repeatable flag and/or
  project-local `.truss/protect` file listing relative paths) that skips those
  destinations during sync.
- **FR-009**: System MUST keep `new`, `sync`, and `check` resolving templates via
  registry first, then embedded packs (existing behavior preserved).
- **FR-010**: Check and dry-run MUST return non-zero exit status when drift/planned
  changes exist if `--fail-on-drift` is set (default off for check remains
  current: already fails on drift).

### Key Entities

- **RegistryEntry**: name, source, kind (`dir` | `file` | `json`), targets, optional modes.
- **Registry**: ordered map of entries loaded from system then user layers.
- **ProtectList**: set of relative project paths never overwritten by sync.
- **SyncPlan**: list of file operations (write/skip) produced by dry-run.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can register a custom directory pack and scaffold a
  project from it in under one minute with documented commands only.
- **SC-002**: List command always surfaces at least the four embedded packs on a
  clean install.
- **SC-003**: Dry-run sync leaves the project tree byte-identical while reporting
  every path that would change.
- **SC-004**: Protected paths are unchanged after sync while unprotected template
  files still update.
- **SC-005**: Automated tests cover add/list/remove, dry-run no-write, and protect
  skip without network.

## Assumptions

- User registry path uses the `directories` crate config dir (already used).
- System registry at `/etc/nixos/truss/registry.json` is read-only in this phase.
- Embedded templates remain authoritative names that user entries may shadow by
  name if deliberately added with the same key.
- Protect list defaults empty; no global defaults beyond empty set.

## Out of Scope

- Remote git/http template fetch
- Template marketplace / signing
- Interactive TUI beyond existing inquire prompts
- Atomic rename / backup of every write (may come later)
- Managing the system-wide registry file
