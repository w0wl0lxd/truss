# Feature Specification: Dry Run and Template Definition

**Feature Branch**: `011-dry-run-and-define`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Add a dry-run mode to `truss new` and `truss sync` that previews every file that would be written, updated, or skipped without changing disk, and add a `define` command that lists the variables a template pack expects so users can prepare inputs before scaffolding."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Preview a scaffold before writing (Priority: P1)

A developer wants to see exactly what `truss new` would create before committing files to disk. They run with a dry-run flag and receive a report of every planned file, its destination path, and whether it would be created, overwritten, or skipped, with no changes written.

**Why this priority**: Dry-run prevents destructive surprises and is the primary value of this feature. It can be tested independently of any template definition or variable listing.

**Independent Test**: Run `truss new myapp --dry-run` against any pack and confirm the target directory remains empty after exit.

**Acceptance Scenarios**:

1. **Given** a pack that would create `Cargo.toml` and `src/main.rs`, **when** the user runs `truss new` with dry-run, **then** both files are listed as planned writes and no files are created on disk.
2. **Given** an existing project, **when** the user runs `truss sync --dry-run`, **then** planned updates, additions, and deletions are listed and the existing project tree is byte-identical afterward.
3. **Given** a protected path configured in the project, **when** dry-run is used, **then** the protected file is listed as skipped and remains untouched.

---

### User Story 2 - Discover template variables before running (Priority: P1)

A developer wants to know what inputs a pack needs (project name, author, license, crate type) before invoking the command. They run a `define` command against a pack and see every variable, whether it is required or optional, its default value, and a short description.

**Why this priority**: Reduces trial-and-error scaffolding and failed runs due to missing variables. It can be demonstrated without any dry-run or disk write.

**Independent Test**: Run `truss define --template my-pack` and verify the output lists all variables and no project files are created.

**Acceptance Scenarios**:

1. **Given** a pack with a `project_name` variable, **when** the user runs `truss define --template my-pack`, **then** `project_name` appears in the output marked as required.
2. **Given** a pack with an optional `license` variable whose default is `"MIT"`, **when** the user runs `truss define`, **then** the output shows `license` as optional with default `"MIT"`.
3. **Given** a pack that can be resolved from the registry or a remote source, **when** the user runs `truss define --template NAME`, **then** the variables from that resolved pack are reported.

---

### User Story 3 - Combine dry-run with variable validation (Priority: P2)

When a developer runs dry-run, `truss` should validate that all required variables are supplied and report any missing or invalid values without writing files. This catches user input errors early.

**Why this priority**: Improves the dry-run report from a simple file list to an accurate preview that will match a real run. It depends on the P1 stories but adds independent validation value.

**Independent Test**: Run `truss new --dry-run` without a required variable and confirm the command fails with a clear error listing the missing inputs.

**Acceptance Scenarios**:

1. **Given** a pack that requires `project_name`, **when** the user runs dry-run without providing it, **then** the command fails and lists `project_name` as missing.
2. **Given** all required variables provided, **when** dry-run runs, **then** the planned file list uses the resolved variable values in rendered output.

---

### User Story 4 - Export dry-run plan as structured output (Priority: P3)

A developer or automation pipeline wants to consume the dry-run plan as structured data for further processing. They can request output in a machine-readable format that lists every planned operation and variable binding.

**Why this priority**: Enables CI checks, diff tooling, and scripting. Not required for the core user experience.

**Independent Test**: Run `truss sync --dry-run --format json` and parse the output to identify planned write and skip operations.

**Acceptance Scenarios**:

1. **Given** a dry-run with two planned writes and one skip, **when** structured output is requested, **then** the result contains three operation objects with `path`, `action`, and `reason` fields.

### Edge Cases

- What happens when dry-run is combined with a non-existent output path?
- What happens when a template variable is provided but has an empty value?
- What happens when a pack defines no variables?
- What happens when the same file would be created and skipped because it is protected?
- What happens when dry-run is run against a path the user does not have permission to read?
- What happens when `define` is run against a remote or git pack that has not been cached?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support a dry-run mode for `truss new` that reports planned file operations without creating files or directories.
- **FR-002**: System MUST support a dry-run mode for `truss sync` that reports planned additions, updates, and skips without modifying the target project.
- **FR-003**: System MUST leave the target filesystem byte-identical when dry-run is active.
- **FR-004**: System MUST include the destination path, operation type (create, update, skip, delete), and reason for each planned operation in the dry-run report.
- **FR-005**: System MUST provide a `define` (or equivalent) command that lists every variable expected by a pack.
- **FR-006**: The `define` output MUST indicate whether each variable is required or optional and show its default value when available.
- **FR-007**: The `define` output MUST include a short human-readable description for each variable when one is declared by the pack.
- **FR-008**: System MUST validate required variables during dry-run and fail with a typed error listing missing variables before any file operation is reported.
- **FR-009**: System MUST render the dry-run plan using the resolved variable values so the preview matches what a real run would produce.
- **FR-010**: System MUST respect protected-path configuration during dry-run and list protected files as skipped.
- **FR-011**: System MUST support deterministic ordering of the dry-run report and `define` output.

### Key Entities

- **DryRunPlan**: A list of planned file operations with path, action, and reason. Rendered in user output or exported as structured data.
- **TemplateVariable**: A variable expected by a pack. Attributes: name, required flag, default value, description.
- **VariableBinding**: A resolved name-value pair used to render a template.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Running `truss new --dry-run` leaves the target directory empty or unchanged in 100% of cases.
- **SC-002**: A developer can discover all variables for a pack in under 30 seconds using a single `define` command.
- **SC-003**: A dry-run report with missing required variables fails within 1 second and lists every missing variable by name.
- **SC-004**: Dry-run reports the exact set of paths that would change, with no path omitted and no false positives, when compared to the subsequent real run using identical inputs.
- **SC-005**: `define` and dry-run outputs remain deterministic and ordered across repeated runs with identical inputs.

## Assumptions

- Dry-run is exposed through a CLI flag (e.g., `--dry-run`) that is optional on `new` and `sync`.
- The `define` command is read-only and does not require a target project path.
- Variable discovery can be derived from the pack's template placeholders, manifest, or an auxiliary schema; packs without explicit metadata still expose at least the placeholders found.
- Structured output for dry-run is optional in the first release.
- Protected paths and the existing registry resolution mechanism continue to work as specified in prior features.

## Out of Scope

- Automatic backup or rollback of files during a real `sync`.
- Interactive editing of variables inside the `define` command.
- Verification that rendered content is semantically correct (only path and presence are previewed).
- Network-less operation for remote `define` if the pack has not been cached; graceful failure is sufficient.
