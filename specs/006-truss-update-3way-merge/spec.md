# Feature Specification: `truss update` with 3-Way Merge

**Feature Branch**: `006-truss-update-3way-merge`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Add a `truss update` command that applies upstream template changes to an existing project while preserving local edits, using a 3-way merge between the original template snapshot, the current project state, and the latest template render.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Apply non-conflicting template updates (Priority: P1)

A project was scaffolded from a template and the template has since changed. The developer runs `truss update` and the latest template changes are merged into the project without touching files the developer edited locally.

**Why this priority**: This is the core value of the feature. Most updates are additive or touch files the user has not modified, so they should apply cleanly and automatically.

**Independent Test**: Create a project from a pack, edit one file locally, change the pack, run `truss update`, and verify the pack change is applied while the local edit remains.

**Acceptance Scenarios**:

1. **Given** a project generated from a pack where the local file `README.md` was not edited, **when** the pack's `README.md` is updated, **then** `truss update` writes the new `README.md`.
2. **Given** a project where `Cargo.toml` was edited locally but the pack's `Cargo.toml` did not change, **when** `truss update` runs, **then** the local `Cargo.toml` remains unchanged.
3. **Given** a project where neither the pack nor the user changed `src/main.rs`, **when** `truss update` runs, **then** `src/main.rs` is unchanged and reported as such.

---

### User Story 2 - Report and resolve merge conflicts (Priority: P1)

Both the template and the developer edited the same file. `truss update` detects the conflict, reports which files are in conflict, and does not write a partially merged project unless the developer explicitly asks the tool to write conflict markers.

**Why this priority**: Conflicts are inevitable when a template and a project evolve independently. The tool must fail closed and make conflicts easy to find.

**Independent Test**: Edit a file locally and also update the pack's version of that file. Run `truss update` and confirm the conflict is reported and the original file is preserved.

**Acceptance Scenarios**:

1. **Given** a file changed in both the pack and the project, **when** `truss update` runs, **then** the command reports a conflict and exits with a non-zero status without modifying the file.
2. **Given** a conflict, **when** the user runs `truss update --write-conflicts`, **then** the file is written with conflict markers showing both the local and the template versions.
3. **Given** a conflict-free update, **when** the command completes, **then** the project passes `cargo check` (or equivalent project validation) without manual intervention.

---

### User Story 3 - Dry-run and base snapshot management (Priority: P2)

Before running an update, the developer wants to see what would change. `truss update --dry-run` lists files that would be modified, added, removed, or put into conflict. The tool also remembers the last template snapshot used so subsequent updates are incremental.

**Why this priority**: Dry-run reduces risk; stored snapshots make repeated updates reliable. These are important but secondary to the core merge behavior.

**Independent Test**: Run `truss update --dry-run` against a project with pending pack changes and confirm the plan matches the actual outcome. Run `truss update` and then change the pack again; confirm the second update uses the previous rendered template as the base.

**Acceptance Scenarios**:

1. **Given** pending pack changes, **when** `truss update --dry-run` runs, **then** no project files are modified and the plan lists expected changes and conflicts.
2. **Given** a successful `truss update`, **when** the pack changes again, **then** the next `truss update` uses the rendered template from the previous update as the new base snapshot.
3. **Given** a project with no recorded base snapshot, **when** `truss update` runs, **then** the command prompts for or requires a base reference before merging.

### Edge Cases

- A file exists in the base snapshot and the pack but the user deleted it locally. The update reports a conflict or re-adds the file depending on the merge policy.
- A file was added locally but did not exist in the base or pack. The update preserves it.
- A file is identical in base, pack, and project. It is reported as unchanged.
- The pack removes a file that existed in the base and the local copy is identical to the base. The update removes the local file.
- The pack removes a file that the user edited. The update reports a conflict.
- A protected path (`--protect` or `.truss/protect`) is changed in the pack. The update skips it and reports the skip.
- A symlink is present in the project or pack. The update refuses to follow or overwrite it and reports a typed error.
- A base snapshot is missing because the project was created before `truss update` existed. The user must supply a base reference or regenerate the project.
- A binary file differs between pack and project. The update treats it as a conflict and does not attempt line-based merging.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a `truss update` command that applies template changes to an existing project using a 3-way merge.
- **FR-002**: The 3-way merge MUST compare a base template snapshot, the current project state, and the latest template render.
- **FR-003**: The system MUST apply non-conflicting changes from the template automatically while preserving non-conflicting local edits.
- **FR-004**: The system MUST detect file-level conflicts where the same file changed in both the project and the template.
- **FR-005**: The system MUST fail closed when conflicts exist and not write partial merges unless the user explicitly enables conflict-marker output.
- **FR-006**: The system MUST support `--dry-run` that reports the merge plan without modifying project files.
- **FR-007**: The system MUST store a base snapshot after each successful update so later updates are incremental.
- **FR-008**: The system MUST respect protected paths and never overwrite them during an update.
- **FR-009**: The system MUST enforce the same path-safety rules as `truss new` and `truss sync` (no absolute paths, no `..` traversal, no writing through symlinks).
- **FR-010**: The system MUST support all template kinds (embedded, `dir`, `file`, `git`) that the registry can resolve.

### Key Entities

- **BaseSnapshot**: The recorded render of the template that was used to create or last update the project.
- **MergeResult**: A per-file result such as `Applied`, `Unchanged`, `Conflict`, `Removed`, or `SkipProtected`.
- **UpdatePlan**: The ordered list of `MergeResult` records produced by the merge.
- **ConflictMarker**: A textual representation of both the local and template versions of a conflicting file.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 95% of template updates that do not touch locally edited files apply with zero user intervention.
- **SC-002**: All merge conflicts are reported before any file is written, and the working tree remains unchanged on conflict unless the user opts into conflict markers.
- **SC-003**: `truss update --dry-run` predicts the final merge plan with 100% accuracy compared to the non-dry run.
- **SC-004**: Local edits in protected paths survive an update in 100% of cases.
- **SC-005**: A complete update of a git-based template reflects the remote ref within one minute after cache resolution.

## Assumptions

- The 3-way merge operates on line-oriented text files; binary files are treated as whole-file conflicts.
- The base snapshot is a recorded render of the template as it existed when the project was created or last updated.
- Updates do not delete files that are unique to the project unless the template removed a file that the base had and the local copy is unchanged.
- The user is responsible for resolving conflicts when they occur.
- Path safety, typed errors, no unsafe code, and deterministic ordering are enforced as per the project constitution.
- [NEEDS CLARIFICATION: Should `truss update` store the base snapshot inside the project directory (e.g., `.truss/base`), derive it from the project's Git history, or store it in a global cache keyed by project path?]
- [NEEDS CLARIFICATION: When a conflict is detected, should the tool write standard conflict markers into the file, create separate `.local` / `.template` side files, or stop and require external resolution?]
- [NEEDS CLARIFICATION: When the pack removes a file that the user has not edited, should `truss update` delete the local file or preserve it and warn?]

## Out of Scope

- Automatic conflict resolution heuristics beyond "take both with markers".
- Interactive merge conflict resolution inside `truss`.
- Merging across branches of a Git repository that are unrelated to the template pack.
- Backing up files before update (may come later).
- Updating multiple templates into the same project in one invocation.
