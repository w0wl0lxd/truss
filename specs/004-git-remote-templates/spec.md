# Feature Specification: Git-based Remote Templates

**Feature Branch**: `004-git-remote-templates`

**Created**: 2026-07-16

**Status**: Draft

**Input**: User description: "Add git-based remote template packs so `truss` users can share templates from a Git repository URL without manually cloning or updating local absolute paths."

## User Scenarios & Testing

### User Story 1 - Register and use a remote Git template (Priority: P1)

A user wants to create a project from a template that is stored in a public Git repository. They register the repository URL once with `truss registry add`, then run `truss new` with the registered name. `truss` clones the repository, resolves the requested branch or tag, and scaffolds the project the same way it would from a local directory pack.

**Why this priority**: This removes the need for every team member to maintain a local checkout and an absolute path in their registry. It is the core value of the feature and unblocks the other stories.

**Independent Test**: Register a Git URL pointing at a local bare repository, run `truss new`, and verify `cargo check` passes in the generated workspace.

**Acceptance Scenarios**:

1. **Given** a registry entry with `kind = "git"` and a valid repository URL, **When** the user runs `truss new myapp --template my-remote-pack`, **Then** the project is scaffolded from the repository contents and matches the remote template.
2. **Given** the same remote template, **When** the user runs `truss sync --path myapp --template my-remote-pack --dry-run`, **Then** the plan reflects only the differences between the local project and the resolved remote template.
3. **Given** a `dir` or `file` registry entry from a previous release, **When** the user runs `truss new` or `truss sync`, **Then** behavior is unchanged.

---

### User Story 2 - Pin a branch, tag, or commit and select a subfolder (Priority: P1)

A template repository may contain multiple templates in subfolders or evolve on a `main` branch. The user wants to pin a specific ref and optionally a subfolder so the generated project is stable and uses the right template root.

**Why this priority**: Teams need reproducible scaffolds. Pinning a ref and selecting a subfolder are standard expectations for any Git-based template tool.

**Independent Test**: Register a Git entry with a `pointer` (ref) and `subfolder`, run `truss new`, and verify the generated files come from the subfolder at the specified ref.

**Acceptance Scenarios**:

1. **Given** a registry entry with `pointer = "v1.2.0"`, **When** the user runs `truss new`, **Then** the template is resolved at tag `v1.2.0`.
2. **Given** a registry entry with `subfolder = "templates/rust-service"`, **When** the user runs `truss new`, **Then** files are scaffolded from that subfolder, not the repository root.
3. **Given** an invalid or missing ref, **When** the entry is used, **Then** `truss` fails with a clear error before writing any files.

---

### User Story 3 - Cache and update remote templates efficiently (Priority: P2)

After the first clone, `truss` should avoid re-downloading the entire repository on every `new` or `sync`. It should keep a local cache and fetch updates when the network is available, falling back to the cache when offline.

**Why this priority**: Performance and offline usage are important for CLI ergonomics, but they do not block the basic P1 stories.

**Independent Test**: Run `truss new` twice against the same Git entry and confirm the second run does not perform a full clone from scratch.

**Acceptance Scenarios**:

1. **Given** an existing cache for a Git entry, **When** the user runs `truss new` and the remote has new commits on the pinned ref, **Then** the cache is updated and the latest ref content is used.
2. **Given** an existing cache and no network connectivity, **When** the user runs `truss new`, **Then** `truss` uses the cached content if the ref is present.
3. **Given** a fresh cache, **When** the user runs `truss registry remove my-remote-pack`, **Then** the associated cache directory is removed.

---

### User Story 4 - Shorthand Git URLs and common hosting platforms (Priority: P3)

Users should be able to register templates with short names like `gh:myorg/myrepo` instead of full HTTPS URLs, reducing copy-paste friction for common hosts.

**Why this priority**: Nice-to-have ergonomics for popular hosts. It is not required for the MVP.

**Independent Test**: Register a Git entry using `gh:owner/repo` shorthand and verify it resolves to the equivalent HTTPS URL before cloning.

**Acceptance Scenarios**:

1. **Given** a registry entry source `gh:truss-packs/monorepo`, **When** `truss` resolves it, **Then** it is expanded to `https://github.com/truss-packs/monorepo.git`.
2. **Given** a fully-qualified HTTPS or SSH URL, **When** it is registered, **Then** it is used as-is.

### Edge Cases

- What happens when the repository URL is malformed or unreachable?
- What happens when the requested ref does not exist in the remote?
- What happens when the `subfolder` does not exist or is not a directory?
- What happens when the repository contains a `.git` directory or other VCS metadata?
- What happens when the user registers a local directory path with `kind = "git"`?
- What happens when the cached repository contains uncommitted changes or conflicts?
- What happens when the network is unavailable on first use?
- What happens when two registry entries point at the same URL with different refs?

## Requirements

### Functional Requirements

- **FR-001**: The system MUST allow a registry entry with `kind = "git"`.
- **FR-002**: The system MUST accept a Git repository URL or a supported shorthand as the `source` for a `git` entry.
- **FR-003**: The system MUST support an optional `pointer` field on `git` entries that specifies a branch, tag, or commit to resolve.
- **FR-004**: The system MUST support an optional `subfolder` field on `git` entries that selects a directory inside the repository as the template root.
- **FR-005**: The system MUST validate a `git` entry before persisting it (e.g., URL syntax, no path traversal in `subfolder`).
- **FR-006**: The system MUST clone or update the remote repository into a local cache before rendering the template.
- **FR-007**: The system MUST resolve the requested `pointer` to a concrete revision and check out the corresponding file tree.
- **FR-008**: The system MUST NOT copy VCS metadata such as `.git` into the generated project.
- **FR-009**: The system MUST produce the same `truss new`, `truss sync`, and `truss check` behavior for a `git` template as for a local `dir` template once it is resolved.
- **FR-010**: The system MUST fail closed with an actionable error for invalid URLs, missing refs, network failures, or malformed subfolder paths.

### Key Entities

- **GitRegistryEntry**: A registry entry that describes a remote Git template. Attributes: display name, repository URL, resolved ref pointer, optional subfolder path, optional file mode.
- **GitCache**: A local on-disk copy of one or more remote repositories keyed by registry entry. Attributes: cache root directory, entry key, repository worktree, last resolved ref.

## Success Criteria

### Measurable Outcomes

- **SC-001**: A project generated from a `git` template passes `cargo check` (or its equivalent for the pack) without manual intervention.
- **SC-002**: `truss sync --dry-run` against a `git` template reports no unexpected writes when the local project is up to date.
- **SC-003**: A change to the remote ref is reflected in a subsequent `truss new` or `truss sync` without requiring the user to delete a local directory.
- **SC-004**: Invalid Git URLs, missing refs, and missing subfolders produce a clear error before any files are written.
- **SC-005**: Existing `dir` and `file` registry entries and embedded templates continue to work exactly as before.

## Assumptions

- A network connection is available for the initial clone of a remote template.
- Public Git repositories and SSH-agent-based authentication are the primary use cases; explicit HTTPS token authentication is out of scope for the MVP.
- Users have a supported Git URL scheme (HTTPS, SSH, or a recognized shorthand) and the remote host is reachable.
- The `truss` CLI and core library already support local directory packs, multi-crate layouts, and registry management.
- Cache storage can use the platform cache directory (e.g., `$XDG_CACHE_HOME/truss` on Linux).
