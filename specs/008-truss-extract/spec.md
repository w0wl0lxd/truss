# Feature Specification: `truss extract` - Reverse Scaffold a Template Pack

**Feature Branch**: `008-truss-extract`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Add a `truss extract` command that turns an existing Rust project into a reusable truss template pack by replacing project-specific values with template variables and preserving the project structure.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Extract an existing project into a pack (Priority: P1)

A developer has a well-structured Rust workspace they want to reuse as a template. They run `truss extract --source ./my-existing-app --pack ./my-pack` and `truss` produces a directory pack where project-specific strings (project name, author, license, edition, repository URL) are replaced by template variables.

**Why this priority**: This is the core value of the feature. Without reverse extraction, users must manually recreate a pack from an existing project.

**Independent Test**: Create a Rust project with a `Cargo.toml` and source files, run `truss extract`, then run `truss new demo --template the-extracted-pack` and verify the generated project looks the same.

**Acceptance Scenarios**:

1. **Given** an existing workspace with `project_name = "my-existing-app"`, **when** `truss extract` runs, **then** the generated pack contains `{{ project_name }}` wherever the literal name appeared in file paths or contents.
2. **Given** an existing workspace with `author`, `license`, `edition`, and `repository` metadata, **when** `truss extract` runs, **then** those values are replaced by the corresponding template variables in pack files.
3. **Given** a valid source and destination path, **when** extraction completes, **then** `truss new --dry-run` using the pack succeeds and reports the expected file tree.

---

### User Story 2 - Preserve structure and file modes (Priority: P2)

The extracted pack must keep the same directory layout, member crates, executable scripts, and non-templated files as the original project so that the resulting pack is a faithful template.

**Why this priority**: Faithful extraction makes the pack useful immediately. Without preserving modes and layout, generated projects would be subtly broken.

**Independent Test**: Extract a project containing an executable script and a workspace member, then scaffold a new project and verify the script remains executable and the member appears in the workspace.

**Acceptance Scenarios**:

1. **Given** a source project with a workspace member at `crates/lib`, **when** `truss extract` runs, **then** the pack contains `crates/lib/Cargo.toml` and a layout descriptor that records the member.
2. **Given** a source file with mode `0o755`, **when** `truss extract` runs, **then** the corresponding pack file has the same executable mode.
3. **Given** a source file that contains no project-specific strings, **when** `truss extract` runs, **then** the file is copied unchanged into the pack.

---

### User Story 3 - Generate a prompt manifest from discovered values (Priority: P3)

After extraction, the pack should include a prompt manifest that asks future users for the values that were discovered and replaced. This reduces manual work for the template author.

**Why this priority**: Automatically creating prompts turns an extracted pack into a reusable, user-friendly template. It is a nice-to-have on top of the core extraction.

**Independent Test**: Run `truss extract` on a project and verify the pack contains a prompt manifest with entries for `project_name`, `author`, `license`, `edition`, and `repository`.

**Acceptance Scenarios**:

1. **Given** a source project with a `Cargo.toml` containing `name = "demo"` and `authors = ["Alice"]`, **when** `truss extract` runs, **then** the pack manifest includes prompts for project name and author with `demo` and `Alice` as defaults.
2. **Given** a value that appears in multiple source files, **when** the prompt manifest is generated, **then** a single prompt variable is created and used consistently across the pack.
3. **Given** the user passes `--skip-prompts`, **when** `truss extract` runs, **then** no prompt manifest is generated and the pack uses static defaults instead.

### Edge Cases

- The source project has no `Cargo.toml`. Extraction uses the source directory name as the default project name and treats the source as a generic file tree.
- Project-specific values overlap (e.g., the project name is a substring of the repository URL). The longest value is replaced first to avoid partial replacements.
- A value appears inside a binary file. The binary is copied as-is and the value is not replaced.
- A file contains a value in a derived form (e.g., `my_project` vs `my-project`). Only exact literal matches are replaced; derived forms are left for manual cleanup.
- The destination pack directory already exists. The command fails unless `--force` is supplied, in which case the directory is replaced.
- The source directory is the same as the destination directory. The command fails with a clear error.
- A symlink exists in the source tree. The command skips symlinks and reports the skip, preserving safety.
- A file path would escape the destination directory after variable substitution. The command rejects the generated path before writing.
- A workspace member path is absolute or contains `..`. The command rejects it as a path-safety violation.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a `truss extract` command that accepts a source project path and an output pack path.
- **FR-002**: The system MUST discover common project-specific values from the source project (project name, author, license, edition, repository URL) and replace them with the corresponding template variables in file paths and contents.
- **FR-003**: The system MUST produce a directory pack that can be registered and used with `truss new` without manual editing.
- **FR-004**: The system MUST preserve the source directory structure, workspace member layout, and original file modes in the extracted pack.
- **FR-005**: The system MUST refuse to write outside the destination directory and must reject absolute or `..` paths.
- **FR-006**: The system MUST reject extraction when the source and destination paths are the same or overlap.
- **FR-007**: The system MUST handle binary files by copying them unchanged and skipping string replacement.
- **FR-008**: The system MUST generate a layout descriptor for multi-member workspaces discovered in the source.
- **FR-009**: The system MUST produce deterministic output (same input yields identical file order and content across runs).
- **FR-010**: The system MUST fail closed with a typed error when the source cannot be read or the destination cannot be safely created.

### Key Entities

- **ExtractedPack**: The output directory pack produced from a source project, including files, a layout descriptor, and an optional prompt manifest.
- **TemplateValue**: A discovered project-specific string and the template variable name that replaces it.
- **ExtractOptions**: The user-supplied source path, destination path, force flag, and optional list of values to replace.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An existing Rust workspace can be extracted into a pack and used to scaffold an identical-looking new project in under 5 minutes.
- **SC-002**: At least 90% of occurrences of discovered project-specific values in source files are replaced with template variables.
- **SC-003**: The generated pack passes `truss new --dry-run` without errors and produces a project with a matching file tree.
- **SC-004**: No file outside the destination pack directory is created or modified during extraction.
- **SC-005**: Repeated extraction of the same source produces byte-identical pack contents and file ordering.

## Assumptions

- The source project is a Rust workspace. Support for other ecosystems is out of scope for the first release.
- Binary files are detected by null bytes or invalid UTF-8 content.
- Template variable placeholders use the same syntax as the existing template engine (e.g., `{{ project_name }}`).
- Derived forms of values (kebab-case, snake_case, etc.) are not automatically normalized.
- Path safety, typed errors, no unsafe code, and deterministic ordering are enforced as per the project constitution.
- [NEEDS CLARIFICATION: Should `truss extract` produce a prompt manifest from discovered values, or should the user manually create prompts after extraction?]
- [NEEDS CLARIFICATION: Should multi-form value replacement (kebab-case, snake_case, etc.) be performed automatically, or should only exact literal matches be replaced?]
- [NEEDS CLARIFICATION: Should `truss extract` operate on the source directory in-place, or should it always write to a separate output pack directory?]

## Out of Scope

- Extracting projects from non-Rust ecosystems.
- Automatic detection and refactoring of source code beyond simple string replacement.
- Extracting from a Git history or diff; only the current working tree is used.
- De-duplication of files across multiple source projects.
- Automatically uploading or publishing the generated pack.
