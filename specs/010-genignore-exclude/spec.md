# Feature Specification: Pack-Level Exclude Lists (`.genignore`)

**Feature Branch**: `010-genignore-exclude`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Template packs should be able to declare which files or patterns should be excluded from the generated project, so pack authors can keep build artifacts, IDE metadata, VCS data, and other unwanted files out of `truss new`, `sync`, `check`, and `update` output.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Exclude files and directories from generation (Priority: P1)

A pack author adds a `.genignore` file to the pack listing `target/`, `.idea/`, `.DS_Store`, and `*.log`. When a developer runs `truss new`, these files and directories are not copied into the generated project.

**Why this priority**: This is the core value of the feature. Exclusion is essential for any non-trivial pack that contains tooling artifacts.

**Independent Test**: Create a pack with a `.genignore` excluding `tmp/` and `*.tmp`, run `truss new`, and verify the excluded items are absent.

**Acceptance Scenarios**:

1. **Given** a pack containing `target/` in its exclude list, **when** `truss new` runs, **then** the generated project contains no `target/` directory.
2. **Given** a pack with `*.log` in its exclude list and a file `debug.log` in the pack, **when** `truss new` runs, **then** `debug.log` is not present in the project.
3. **Given** a pack with no exclude list, **when** `truss new` runs, **then** all files from the pack are copied as before.

---

### User Story 2 - Support glob patterns and directory patterns (Priority: P2)

Pack authors want flexible exclusion patterns: wildcards for file extensions, directory patterns that exclude entire subtrees, and recursive `**` patterns.

**Why this priority**: Glob support matches user expectations from `.gitignore` and makes exclusion lists concise and powerful.

**Independent Test**: Create a pack with patterns `**/*.bak`, `tmp/`, and `node_modules/`, run `truss new`, and verify all matching files and directories are excluded.

**Acceptance Scenarios**:

1. **Given** an exclude pattern `**/*.bak`, **when** `truss new` runs, **then** every `.bak` file anywhere in the pack is excluded.
2. **Given** an exclude pattern `tmp/`, **when** `truss new` runs, **then** the entire `tmp/` subtree is excluded.
3. **Given** an exclude pattern `data/*.tmp`, **when** `truss new` runs, **then** `.tmp` files directly inside `data/` are excluded but `.tmp` files elsewhere are copied.

---

### User Story 3 - Project-local un-excludes and dry-run visibility (Priority: P3)

A team wants a pack-level exclusion for `.github/` but one project needs it. The team can un-exclude specific paths project-locally, and `truss sync --dry-run` shows exactly which files are excluded.

**Why this priority**: Per-project overrides and dry-run visibility reduce surprises when packs are shared across many projects.

**Independent Test**: Add a pack excluding `.github/` but create a project `.truss/exclude` that un-excludes `.github/workflows/`. Run `truss sync --dry-run` and verify the workflow files appear in the plan.

**Acceptance Scenarios**:

1. **Given** a pack excluding `.github/` and a project-local un-exclude for `.github/workflows/`, **when** `truss sync` runs, **then** `.github/workflows/` is included and other `.github/` paths remain excluded.
2. **Given** a pack with exclude patterns, **when** `truss sync --dry-run` runs, **then** the plan lists excluded paths separately from protected and written paths.
3. **Given** an un-exclude pattern that does not match any file, **when** `truss sync` runs, **then** no error is raised and generation proceeds normally.

### Edge Cases

- An exclude pattern is invalid (e.g., unbalanced brackets or invalid glob syntax). The command fails with a clear error before any file is written.
- An exclude pattern contains `..` or an absolute path. The command rejects it as a path-safety violation.
- An exclude pattern matches the pack's root `Cargo.toml` or another file required by the layout. The command warns or fails, depending on severity, so the generated project is not broken.
- An exclude pattern matches the exclude list file itself. The exclude list file is never copied to the generated project.
- A file is both excluded by the pack and protected by the project. Exclusion applies to source selection; protection applies to destination writes. The file is not generated because it was never selected.
- A glob pattern matches nothing. Generation proceeds without warning.
- An exclude pattern is a directory but written as a file path. The pattern still matches the directory and its contents if the pattern semantics define it that way.
- A `.git` directory is present in a `dir` pack. It is excluded by default or by an explicit pattern.
- A project-local un-exclude conflicts with a project-local exclude. The project-local definition takes precedence over the pack-level definition.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A pack MAY declare an ordered exclude list of relative paths and glob patterns.
- **FR-002**: `truss new`, `sync`, `check`, and `update` MUST skip source pack paths that match the exclude list.
- **FR-003**: Exclude patterns MUST be relative to the pack root and MUST be validated for path safety (no `..`, no absolute components).
- **FR-004**: The system MUST support literal path matches, single-character wildcards (`?`), single-segment wildcards (`*`), and recursive multi-segment wildcards (`**`).
- **FR-005**: The system MUST support directory patterns (trailing `/`) that exclude entire subtrees.
- **FR-006**: The system MUST support project-local un-exclude patterns that override pack-level exclusions.
- **FR-007**: Excluded files MUST not appear in `truss sync --dry-run` or `truss check` drift reports.
- **FR-008**: The system MUST produce a clear error when an exclude pattern is invalid or would match outside the project root.
- **FR-009**: When no exclude list is present, the system MUST copy all pack files as before (no implicit ignores).
- **FR-010**: The exclude list MUST be evaluated in declared order so later rules can override earlier ones where the semantics allow.

### Key Entities

- **ExcludeList**: A pack-level or project-local ordered list of include/exclude patterns.
- **ExcludePattern**: A single glob or literal pattern, a flag indicating include or exclude, and optional directory-only semantics.
- **ExcludeMatch**: The result of applying an exclude list to a source path (include or exclude).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Pack authors can exclude build artifacts, IDE folders, and VCS metadata from generated projects.
- **SC-002**: 100% of excluded paths are absent from generated projects and dry-run plans.
- **SC-003**: Exclude patterns with glob syntax match expected files in at least 95% of common cases compared to `.gitignore`-style semantics.
- **SC-004**: Invalid or unsafe exclude patterns are rejected before any file is written.
- **SC-005**: Existing projects using packs without an exclude list behave identically to before.

## Assumptions

- The exclude list is declared in a pack-level manifest file (e.g., `.genignore`) or a section of a pack manifest.
- Glob semantics follow common shell glob conventions with `**` for recursive matching.
- Exclude patterns apply to source pack files, not to existing project files during `sync`.
- The exclude list file itself is never copied to the generated project.
- Path safety, typed errors, no unsafe code, and deterministic ordering are enforced as per the project constitution.
- [NEEDS CLARIFICATION: Should the exclude list be a standalone `.genignore` file in the pack root, a section inside a pack manifest (e.g., `truss.toml`), or both?]
- [NEEDS CLARIFICATION: Should `truss` support the full `.gitignore` semantics including negation `!`, character classes `[a-z]`, and `**` only in specific positions, or a deliberately minimal subset?]
- [NEEDS CLARIFICATION: Should project-local un-excludes be stored in `.truss/exclude`, in a dedicated project file, or passed only via CLI flags?]

## Out of Scope

- Excluding files from the pack based on content (content-based filters).
- Excluding destination files that are not present in the pack source.
- Wildcard patterns that cross symlink boundaries.
- Automatically generating an exclude list from a `.gitignore` file.
- Exclusion of embedded template packs at build time.
