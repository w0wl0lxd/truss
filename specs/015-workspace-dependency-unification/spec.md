# Feature Specification: Workspace Dependency Unification

**Feature Branch**: `015-workspace-dependency-unification`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Add workspace dependency unification to `truss` so multi-crate projects keep common dependency versions in a single workspace-level declaration and avoid silent version drift between member crates."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Unify common dependencies into the workspace root (Priority: P1)

A developer runs `truss sync` or a dedicated `unify` command in a multi-crate workspace. `truss` identifies dependencies that appear in multiple member crates, extracts them into the workspace-level dependency table, and updates each member crate to reference the unified version.

**Why this priority**: This is the core value of the feature. It reduces drift and makes version bumps a single change in the workspace root.

**Independent Test**: Create a workspace with two crates using `tokio = "1.0"` and `tokio = "1.1"`, run the unification command, and verify both crates reference `workspace = true` and the root declares `tokio` at a single resolved version.

**Acceptance Scenarios**:

1. **Given** two member crates with `serde` at the same version, **when** unification runs, **then** `serde` moves to the workspace root and both members reference it via `workspace = true`.
2. **Given** two member crates with `tokio` at different versions, **when** unification runs, **then** the command either reports a conflict or picks the highest version according to a documented rule.
3. **Given** a member crate with a dependency that appears only once, **when** unification runs, **then** that dependency is not moved to the workspace root unless explicitly configured.

---

### User Story 2 - Detect and report dependency drift (Priority: P1)

A developer runs `truss check` in a workspace and `truss` reports any member crate that uses a different version of a common dependency than the one declared in the workspace root. The report lists the crate, dependency, and versions in conflict.

**Why this priority**: Detection is required before unification and is independently useful in CI to prevent drift.

**Independent Test**: Introduce a version mismatch in a workspace, run `truss check`, and verify the output names the mismatching crate and dependency.

**Acceptance Scenarios**:

1. **Given** a workspace with `serde = "1.0"` in the root and a member using `serde = "1.1"`, **when** `truss check` runs, **then** it reports a drift for `serde`.
2. **Given** all common dependencies match the workspace root, **when** `truss check` runs, **then** it exits successfully with no drift reported.
3. **Given** a member crate with `default-features` or `features` differences, **when** `truss check` runs, **then** those differences are surfaced as drift or handled according to a documented rule.

---

### User Story 3 - Preserve member-specific features and options (Priority: P2)

A developer unifies dependencies while keeping per-crate `features`, `default-features`, `optional`, and target-specific settings. The member manifest retains these options while the version is moved to the workspace root.

**Why this priority**: Workspaces commonly share a dependency version but use different features. Losing member features would break projects.

**Independent Test**: Unify `tokio` across two crates where one uses `features = ["full"]`, and verify the member still declares `features` while `version` is replaced with `workspace = true`.

**Acceptance Scenarios**:

1. **Given** a member crate with `serde = { version = "1.0", features = ["derive"] }`, **when** unification runs, **then** the member manifest becomes `serde = { workspace = true, features = ["derive"] }`.
2. **Given** a member crate with `default-features = false`, **when** unification runs, **then** `default-features` is preserved in the member manifest.
3. **Given** a target-specific dependency (e.g., `[target.'cfg(unix)'.dependencies]`), **when** unification runs, **then** the target table is preserved and the version reference is unified.

---

### User Story 4 - Configure which dependencies to unify or skip (Priority: P2)

A team wants to exclude certain dependencies from unification or force unification of dependencies that appear only once. They configure an allowlist or blocklist in a project-local configuration file.

**Why this priority**: Teams have legitimate reasons to keep some dependencies local (e.g., internal-only crates). Configuration is required for real adoption.

**Independent Test**: Add `skip = ["internal-crate"]` to `.truss/unify.toml`, run unification, and verify `internal-crate` remains in the member crate.

**Acceptance Scenarios**:

1. **Given** a skip list containing `private-dep`, **when** unification runs, **then** `private-dep` is not moved to the workspace root.
2. **Given** an allowlist containing only `tokio`, **when** unification runs, **then** only `tokio` is unified.
3. **Given** a configuration with both allowlist and blocklist for the same dependency, **when** unification runs, **then** `truss` fails with a clear configuration error.

---

### User Story 5 - Dry-run unification (Priority: P3)

A developer wants to preview the changes the unification command would make before modifying manifests. They run with a dry-run flag and see a report of every manifest change.

**Why this priority**: Adds safety to a potentially wide-reaching change. Depends on the core unification logic.

**Independent Test**: Run `truss unify --dry-run` and verify no `Cargo.toml` files are modified.

**Acceptance Scenarios**:

1. **Given** a workspace with drift, **when** dry-run unification runs, **then** the report lists every manifest change without modifying files.

### Edge Cases

- What happens when a workspace root does not exist (single-crate project)?
- What happens when two member crates have conflicting version requirements that cannot be unified?
- What happens when a dependency is already declared in the workspace root but not used by a member?
- What happens when a member uses a path dependency or git dependency instead of a registry version?
- What happens when `truss check` finds a dependency that is newer in the member than in the workspace root?
- What happens when a member manifest is malformed or cannot be parsed?
- What happens when unification would create a dependency table that already exists in the workspace root?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST identify dependencies that appear in multiple workspace member crates.
- **FR-002**: System MUST move unified dependencies to the workspace root manifest with a single version declaration.
- **FR-003**: System MUST update member crate manifests to reference workspace dependencies using `workspace = true`.
- **FR-004**: System MUST preserve per-crate `features`, `default-features`, `optional`, and target-specific settings during unification.
- **FR-005**: System MUST detect version drift between workspace root declarations and member crate declarations in `truss check`.
- **FR-006**: System MUST report drift with the crate name, dependency name, and the conflicting versions.
- **FR-007**: System MUST support a project-local configuration to skip or force specific dependencies during unification.
- **FR-008**: System MUST support dry-run mode for unification that reports planned manifest changes without writing files.
- **FR-009**: System MUST handle conflicts (different versions of the same dependency) according to a documented resolution strategy or fail closed with a clear error.
- **FR-010**: System MUST produce deterministic output for `check`, `sync`, and `unify` commands.
- **FR-011**: System MUST NOT modify non-workspace single-crate projects when unification is requested.
- **FR-012**: System MUST validate workspace member paths and fail closed if a member listed in the workspace manifest is missing or unreadable.

### Key Entities

- **WorkspaceDependencyTable**: The `[workspace.dependencies]` section in the root manifest. Attributes: dependency name, version or version requirement, shared features, optional flag.
- **MemberDependency**: A dependency declared inside a workspace member crate. Attributes: name, version requirement, features, default-features flag, optional flag, target table.
- **UnifyConfiguration**: Project-local settings controlling which dependencies are unified or skipped. Attributes: allowlist, blocklist, conflict resolution strategy.
- **DriftReport**: A report listing dependencies whose member declarations differ from the workspace root. Attributes: member path, dependency name, root version, member version, kind of difference.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After running unification, all dependencies appearing in two or more member crates are declared exactly once in the workspace root.
- **SC-002**: `truss check` reports 100% of version mismatches between member crates and the workspace root without false negatives.
- **SC-003**: Member-specific `features`, `default-features`, and target tables are preserved in 100% of unification operations.
- **SC-004**: Dry-run unification produces a report that matches the actual changes in a subsequent real run for the same workspace and inputs.
- **SC-005**: Unification completes on a 20-member workspace in under 2 seconds.

## Assumptions

- The workspace is defined by a root `Cargo.toml` with a `[workspace]` section.
- The unification command is either a new subcommand or an option on `truss sync`/`truss check`.
- Dependency manifests use TOML and follow Cargo conventions.
- Path dependencies and git dependencies are not unified unless their references are identical and the user configures it.
- The conflict resolution strategy defaults to reporting conflicts rather than silently choosing a version.

## Out of Scope

- Automatic version bumping to the latest compatible version from crates.io.
- Unification of non-Cargo manifest files (e.g., `package.json`, `pyproject.toml`).
- Rewriting of `[dev-dependencies]` and `[build-dependencies]` beyond the same rules as `[dependencies]`.
- Enforcement of dependency licensing or audit policies.
