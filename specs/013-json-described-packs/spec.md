# Feature Specification: JSON-Described Packs

**Feature Branch**: `013-json-described-packs`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Allow `truss` template packs to be described by a JSON manifest so pack authors can declare variables, file mappings, conditions, and metadata explicitly instead of relying solely on directory naming conventions."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Scaffold from a manifest-first pack (Priority: P1)

A pack author places a manifest file in a directory and declares the pack name, version, variables, and file mappings. A user runs `truss new` against the directory and receives the same project as if the pack had been convention-based.

**Why this priority**: Manifest-first packs are the foundation of this feature and unlock richer pack metadata without changing the user experience.

**Independent Test**: Create a pack with only a manifest file and no directory convention, run `truss new`, and verify the generated files match the manifest.

**Acceptance Scenarios**:

1. **Given** a pack directory containing a manifest that declares `name = "my-pack"` and a file mapping to `Cargo.toml`, **when** the user runs `truss new app --template /path/to/pack`, **then** the project is scaffolded from the manifest declarations.
2. **Given** a manifest pack with declared variables, **when** the user provides all variables, **then** the files render with the correct values.
3. **Given** a manifest pack with no `version` field, **when** it is loaded, **then** the tool either supplies a default version or fails with a clear error.

---

### User Story 2 - Declare and validate variables in the manifest (Priority: P1)

A pack author wants to declare each variable's type, whether it is required, its default value, and a description. When a user runs `truss new`, the tool validates the provided values against the manifest and reports clear errors for missing or invalid inputs.

**Why this priority**: Explicit variable schemas are a major reason to use a manifest and prevent runtime failures caused by bad user input.

**Independent Test**: Create a manifest with a required `project_name` string variable, run `truss new` without it, and verify the error names the missing variable.

**Acceptance Scenarios**:

1. **Given** a manifest with `project_name` marked required and type `string`, **when** the user omits it, **then** the command fails and reports `project_name` is missing.
2. **Given** a manifest with `member_count` typed as `integer`, **when** the user provides a non-integer value, **then** the command fails with a validation error.
3. **Given** a manifest with an optional `license` variable with default `"MIT"`, **when** the user omits it, **then** the scaffolded project uses `"MIT"`.

---

### User Story 3 - Conditional files and directories (Priority: P2)

A pack author wants some files or directories to be included only when a specific variable is true or has a given value. The manifest supports conditions, and the generated project includes only the files whose conditions are met.

**Why this priority**: Enables a single pack to serve multiple similar project shapes without duplicating packs.

**Independent Test**: Create a manifest where `src/bin/main.rs` is included only when `has_cli = true`, run `truss new` with both values, and verify inclusion/exclusion.

**Acceptance Scenarios**:

1. **Given** a file mapping with condition `has_cli == true`, **when** the user sets `has_cli` to true, **then** the file is generated.
2. **Given** the same condition, **when** the user sets `has_cli` to false, **then** the file is skipped.
3. **Given** a directory mapping with a condition, **when** the condition is false, **then** the entire directory and its descendants are skipped.

---

### User Story 4 - Validate manifest syntax and schema (Priority: P2)

A pack author or CI pipeline runs a validation command against a manifest and receives clear, actionable errors for missing required fields, invalid types, or contradictory file mappings.

**Why this priority**: Improves pack authoring experience and catches errors before users encounter them.

**Independent Test**: Run `truss pack validate /path/to/pack` against a manifest with a missing `name` and verify the error message.

**Acceptance Scenarios**:

1. **Given** a manifest missing a required field, **when** validation runs, **then** the command fails and names the missing field.
2. **Given** a manifest with a file mapping that references a non-existent source file, **when** validation runs, **then** the command reports the missing source.
3. **Given** a valid manifest, **when** validation runs, **then** the command exits successfully with no warnings or a summary of checks passed.

---

### User Story 5 - Backwards compatibility with convention packs (Priority: P2)

A pack that follows the existing directory conventions and has no manifest continues to work exactly as before. A pack with both a manifest and directory contents uses the manifest as the source of truth, with directory contents as fallback or supplementary data.

**Why this priority**: Existing packs and user registries must keep working; this feature adds an option, not a breaking change.

**Independent Test**: Run `truss new` against an existing convention-based pack and verify output is unchanged.

**Acceptance Scenarios**:

1. **Given** a pack with no manifest, **when** `truss new` runs, **then** the existing convention-based behavior is used.
2. **Given** a pack with a manifest and extra files in the directory not listed in the manifest, **when** `truss new` runs, **then** the behavior is deterministic and documented (e.g., manifest takes precedence or all files are included).

### Edge Cases

- What happens when the manifest contains a field with an unknown name?
- What happens when two file mappings target the same destination path?
- What happens when a variable is declared but never used in a file mapping?
- What happens when a conditional expression references an undefined variable?
- What happens when the manifest file is valid JSON but semantically empty?
- What happens when a pack contains multiple manifest files?
- What happens when the manifest declares a file mapping to a path outside the project root?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support loading a pack from a JSON manifest file.
- **FR-002**: The manifest MUST be able to declare pack metadata (name, version, description, author).
- **FR-003**: The manifest MUST be able to declare variables with name, type, required flag, default value, and description.
- **FR-004**: System MUST validate user-provided variable values against the manifest variable declarations and fail closed with clear errors.
- **FR-005**: The manifest MUST be able to declare file mappings from source paths inside the pack to destination paths in the generated project.
- **FR-006**: The manifest MUST support conditions that determine whether a file or directory mapping is included.
- **FR-007**: System MUST produce deterministic project output from a manifest-first pack.
- **FR-008**: System MUST continue to support convention-based packs without a manifest and produce unchanged behavior.
- **FR-009**: System MUST provide a validation command that checks a manifest for syntax, schema, and referential errors.
- **FR-010**: System MUST reject manifest file mappings that would write outside the target project root.
- **FR-011**: System MUST report manifest parse errors with the file path, line, and column when available.
- **FR-012**: System MUST support a documented manifest file name (e.g., `truss.json` or `truss-pack.json`) and fail clearly when the expected file is missing in a manifest-first pack.

### Key Entities

- **PackManifest**: The JSON description of a pack. Attributes: name, version, description, variables, file mappings, conditions, metadata.
- **ManifestVariable**: A declared variable in the manifest. Attributes: name, type, required, default, description, validation rules.
- **FileMapping**: A mapping from a source file inside the pack to a destination path in the generated project, with optional condition.
- **Condition**: A Boolean expression over variable values that controls inclusion of a mapping.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A pack described only by a JSON manifest scaffolds a project that passes `cargo check` (or equivalent) without manual fixes.
- **SC-002**: Manifest validation reports all schema and reference errors in a single run with line/column information.
- **SC-003**: 100% of existing convention-based packs continue to work with no user-facing changes.
- **SC-004**: Conditional file mappings produce correct inclusion and exclusion across the documented variable value space.
- **SC-005**: Invalid manifest input fails with a typed error before any file is written.

## Assumptions

- The manifest format is JSON for broad tooling support and easy generation.
- Variable types are limited to a stable set (string, integer, boolean) in the first release.
- Manifest-first packs are loaded from a single manifest file at the pack root.
- Conditions are simple expressions (equality, presence, boolean truthiness) and do not require a full expression language.
- Packs may mix manifest declarations with convention-based files using a documented precedence rule.

## Out of Scope

- A dedicated manifest editor or GUI.
- Remote manifest resolution outside of existing git/remote pack support.
- Manifest signing or verification.
- Arbitrary code execution or templating logic inside the manifest.
