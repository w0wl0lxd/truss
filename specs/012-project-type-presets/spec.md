# Feature Specification: Project Type Presets

**Feature Branch**: `012-project-type-presets`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Add project-type presets to `truss` so users can scaffold common shapes (binary, library, workspace, micro-service, etc.) with a single selection instead of memorizing pack names and variable combinations."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Select a built-in preset when creating a project (Priority: P1)

A developer runs `truss new` and selects a project type preset such as "binary" or "library". `truss` maps that choice to a known pack and a default set of variable bindings, then scaffolds the project with no further decisions required.

**Why this priority**: This is the core value of the feature. It removes the need to remember pack names and provides a fast path for the most common project shapes.

**Independent Test**: Run `truss new myapp --type binary` and verify the generated project matches the binary pack and variable defaults.

**Acceptance Scenarios**:

1. **Given** a built-in preset named `binary`, **when** the user runs `truss new myapp --type binary`, **then** the project is scaffolded from the binary preset.
2. **Given** a built-in preset named `workspace`, **when** the user runs `truss new myapp --type workspace`, **then** a workspace layout is created with member crates matching the preset.
3. **Given** no preset selected, **when** the user runs `truss new`, **then** the existing behavior (prompt for template or use default) is preserved.

---

### User Story 2 - Override preset defaults with explicit variables (Priority: P1)

A developer starts from a preset but wants to change specific defaults, such as the license, author, or member crate names. They provide explicit variable values that replace the preset defaults.

**Why this priority**: Presets must be useful starting points, not rigid templates. Overrides are required for real-world adoption.

**Independent Test**: Run `truss new myapp --type binary --license Apache-2.0` and verify the generated `Cargo.toml` contains the overridden license.

**Acceptance Scenarios**:

1. **Given** a preset with default `license = "MIT"`, **when** the user overrides it with `--license Apache-2.0`, **then** the scaffolded project uses `Apache-2.0`.
2. **Given** a workspace preset with default member names, **when** the user overrides a member name, **then** the workspace reflects the provided name.
3. **Given** an explicit template option `--template custom-pack`, **when** the user also provides `--type binary`, **then** `truss` resolves the conflict with a clear error or uses the most specific input.

---

### User Story 3 - List and inspect available presets (Priority: P2)

A developer wants to see all available presets and a short description of each before choosing. They run a `list` or `types` command and see names, descriptions, and the packs they map to.

**Why this priority**: Discoverability helps users learn the presets and pick the right one. Not required for the first successful use.

**Independent Test**: Run `truss types` and verify all built-in presets appear with descriptions.

**Acceptance Scenarios**:

1. **Given** a fresh install, **when** the user runs `truss types`, **then** all built-in presets are listed in deterministic order.
2. **Given** a preset named `service`, **when** the user runs `truss types --details service`, **then** the output shows the mapped pack, default variables, and a description.

---

### User Story 4 - Define and reuse custom presets (Priority: P2)

A team wants to define their own preset that combines a specific pack with their standard variable defaults. They create a custom preset file and can use it by name across the team.

**Why this priority**: Allows organizations to standardize scaffolding without forking `truss`. Depends on built-in presets but adds independent value.

**Independent Test**: Add a custom preset `team-service` to the user config, run `truss new app --type team-service`, and verify the project uses the custom defaults.

**Acceptance Scenarios**:

1. **Given** a custom preset in the user config directory, **when** the user runs `truss new app --type team-service`, **then** the project is scaffolded using the custom pack and variables.
2. **Given** a custom preset with the same name as a built-in preset, **when** the user lists presets, **then** `truss` warns or resolves according to a documented precedence rule.
3. **Given** an invalid custom preset (missing pack or malformed variables), **when** it is loaded, **then** `truss` fails with an actionable error.

---

### User Story 5 - Preset-aware sync and check (Priority: P3)

A developer uses a preset when running `truss sync` or `truss check` so the tool can compare the current project against the shape implied by the preset, not just a raw template.

**Why this priority**: Extends presets to the maintenance workflow, but it is optional because `sync` and `check` can still operate on explicit templates.

**Independent Test**: Run `truss sync --type workspace` and verify the sync plan uses the workspace preset as the source template.

**Acceptance Scenarios**:

1. **Given** a project created with `--type binary`, **when** the user runs `truss sync --type binary --dry-run`, **then** the sync plan compares against the binary preset.
2. **Given** a project with a locally recorded preset, **when** the user runs `truss check`, **then** the check reports drift from that preset without requiring `--type`.

### Edge Cases

- What happens when the selected preset name does not exist?
- What happens when a preset references a pack that is not installed or reachable?
- What happens when preset defaults conflict with variables from an existing project during sync?
- What happens when a custom preset file is malformed or has duplicate names?
- What happens when the user provides both `--type` and `--template`?
- What happens when a preset is removed after a project was created with it?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide built-in project-type presets for the most common shapes (e.g., `binary`, `library`, `workspace`, `service`).
- **FR-002**: Each built-in preset MUST map to a pack and a default set of variable bindings.
- **FR-003**: System MUST allow the user to select a preset during `truss new`.
- **FR-004**: System MUST allow explicit variable values to override preset defaults.
- **FR-005**: System MUST list all available presets with names and descriptions.
- **FR-006**: System MUST allow users to define custom presets in the user configuration directory.
- **FR-007**: System MUST validate custom presets on load and fail with a clear error if required fields are missing or malformed.
- **FR-008**: System MUST preserve existing `truss new` behavior when no preset is selected.
- **FR-009**: System MUST support `--type` as an optional flag on `truss sync` and `truss check`.
- **FR-010**: System MUST record the preset used during `truss new` in the generated project so later `sync` and `check` can default to it.
- **FR-011**: System MUST handle name collisions between custom and built-in presets with deterministic precedence documented to the user.
- **FR-012**: System MUST produce deterministic preset listings and scaffold output.

### Key Entities

- **Preset**: A named project shape that references a pack and default variable bindings. Attributes: name, description, pack reference, default variables, optional tags.
- **PresetRegistry**: An ordered collection of built-in and user-defined presets loaded from configuration layers.
- **ProjectPresetRecord**: A file or metadata entry in a generated project that records which preset was used.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can scaffold a common project type in a single command with at most two additional inputs (name and type).
- **SC-002**: All built-in presets are discoverable via one list command with descriptions understandable to a new user.
- **SC-003**: Preset-based scaffolding produces byte-identical output for identical inputs across different machines and runs.
- **SC-004**: 100% of built-in presets can be overridden with explicit variables without requiring the user to bypass the preset system.
- **SC-005**: Custom presets created by one team member are usable by another on a clean machine after copying the config file.

## Assumptions

- Presets are declarative and do not contain implementation logic.
- Built-in presets ship with `truss` and are documented in user-facing help.
- Custom presets live in the platform configuration directory alongside the user registry.
- Preset names are limited to a stable character set (alphanumeric, hyphens, underscores) to avoid filesystem and CLI issues.
- The existing pack resolution and variable substitution systems remain unchanged; presets layer on top of them.

## Out of Scope

- Interactive wizard for selecting preset options beyond a single flag.
- Preset versioning or marketplace distribution in this phase.
- Automatic conversion of an existing project into a preset.
- Preset inheritance or composition beyond simple override of defaults.
