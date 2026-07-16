# Feature Specification: Template-Defined Prompts

**Feature Branch**: `005-template-defined-prompts`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Template authors should be able to declare custom prompts/variables inside a template pack so that `truss new` and `truss sync` can ask users project-specific questions and feed the answers into the template renderer.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Declare and answer custom prompts (Priority: P1)

A template author adds a pack-level prompt manifest that asks for values such as service name, description, or whether to include a CLI. When a developer runs `truss new --template my-pack`, `truss` asks those questions, applies defaults from the workspace or CLI arguments, and renders the project using the answers.

**Why this priority**: This is the core value of the feature. Without it, every pack is limited to the built-in context variables and cannot capture project-specific choices.

**Independent Test**: Create a pack with two custom prompts, run `truss new`, answer them, and verify the rendered files contain the answers.

**Acceptance Scenarios**:

1. **Given** a pack containing a prompt manifest with a variable `description`, **when** the user runs `truss new demo --template that-pack`, **then** the user is asked for a description and the value appears in the generated `Cargo.toml` or `README.md`.
2. **Given** a pack with a prompt that has a default value, **when** the user accepts the default, **then** the default value is used for rendering.
3. **Given** a pack with no custom prompts, **when** the user runs `truss new`, **then** scaffolding completes using only the built-in context variables.

---

### User Story 2 - Validate prompt answers and support choices (Priority: P2)

Template authors want to restrict answers (e.g., a license chosen from a list) or enforce rules (e.g., a non-empty service name). `truss` validates answers before rendering and re-prompts or fails with a clear message when validation fails.

**Why this priority**: Validation prevents broken projects and reduces support load, but a pack can still be useful without it.

**Independent Test**: Create a pack with a choice prompt for license and a required text prompt. Provide an invalid answer and confirm the command fails; provide valid answers and confirm rendering succeeds.

**Acceptance Scenarios**:

1. **Given** a choice prompt with options `MIT`, `Apache-2.0`, and `GPL-3.0`, **when** the user selects `MIT`, **then** `{{ license }}` renders to `MIT`.
2. **Given** a required prompt with an empty answer and no default, **when** the user submits an empty value, **then** the command reports the missing value and does not write files.
3. **Given** a validation rule that rejects names containing spaces, **when** the user enters `my project`, **then** the command reports the constraint and prompts again.

---

### User Story 3 - Conditional prompts and non-interactive use (Priority: P3)

A pack can ask follow-up questions only when a previous answer warrants them (e.g., ask for CLI framework only if the user chose to include a CLI). Teams running `truss` in CI can supply all answers via CLI flags or environment variables so no interactive prompt is shown.

**Why this priority**: Conditional prompts improve ergonomics for complex packs; non-interactive support unlocks CI usage. Neither is required for the basic prompting flow.

**Independent Test**: Define a conditional prompt that appears only when `include_cli` is true. Run `truss new` with `include_cli=false` and confirm the follow-up is skipped; run with all values supplied via flags and confirm no prompts appear.

**Acceptance Scenarios**:

1. **Given** a conditional prompt that depends on `include_cli = true`, **when** the user answers `include_cli` with `false`, **then** the dependent prompt is skipped.
2. **Given** all custom prompt values supplied through CLI flags or environment variables, **when** the user runs `truss new` in a non-interactive shell, **then** scaffolding completes with no prompts.
3. **Given** a missing required value in non-interactive mode, **when** the command runs, **then** it fails with a clear list of missing variables.

### Edge Cases

- A pack declares a prompt whose answer is referenced in a template but not declared: the command fails with a clear error before writing files.
- A prompt default references another prompt answer: defaults are resolved in declared order.
- A prompt manifest is malformed: the command fails with a parse error and does not render.
- Built-in context variables (`project_name`, `author`, `license`, `edition`, `repository`) are present alongside custom prompts and retain their existing precedence.
- A user provides an extra/unknown CLI flag: it is ignored or warned, not treated as a prompt answer.
- A prompt answer contains path-unsafe characters or traversal sequences: the value is sanitized before being used in file paths; if a path would escape the project root, the command fails.
- Rendering a templated file consumes the prompt answers in deterministic order.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A template pack MAY include a prompt manifest that declares user-facing variables and their prompts.
- **FR-002**: The system MUST prompt the user for each declared variable during `truss new` and `truss sync` when a value is not already supplied by a CLI argument, environment variable, or existing workspace metadata.
- **FR-003**: Each prompt MUST have a human-readable label, an optional default value, and an optional validation rule or list of allowed choices.
- **FR-004**: Prompt answers MUST be made available to the pack's rendering context using the declared variable names.
- **FR-005**: The system MUST support deterministic ordering of prompts exactly as declared in the manifest.
- **FR-006**: The system MUST validate prompt answers against the pack's declared constraints before rendering.
- **FR-007**: The system MUST allow all prompt values to be supplied non-interactively via CLI arguments or environment variables.
- **FR-008**: For `truss sync` and `truss check`, the system MUST reuse existing project metadata as defaults and only prompt for variables that cannot be inferred from the workspace.
- **FR-009**: The prompt manifest and any rendered prompt values used in paths MUST be subject to the project's path-safety rules; traversal and absolute paths are rejected.

### Key Entities

- **PromptManifest**: A pack-level collection of prompt definitions, ordered and owned by the pack.
- **Prompt**: A single user-facing question with a variable name, label, default value, kind (text, choice, boolean), validation constraints, and optional condition.
- **PromptAnswer**: A name/value pair supplied by the user, the environment, or workspace metadata.
- **RenderingContext**: The full set of values (built-in + prompt answers) used to render template files.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can add custom prompts to a pack and scaffold a project with those answers in under 2 minutes.
- **SC-002**: 100% of prompt answers that violate declared constraints are rejected before any file is written.
- **SC-003**: CI/non-interactive `truss new` with all values supplied completes with zero prompts in under 30 seconds.
- **SC-004**: `truss sync` reuses workspace metadata and prompts only for missing values in at least 95% of common cases.
- **SC-005**: The order of prompts shown to the user matches the pack manifest order 100% of the time.

## Assumptions

- A pack that does not declare custom prompts continues to work exactly as before.
- Built-in context variables cannot be redefined by a prompt manifest; custom variables are merged without overwriting built-ins.
- Text, choice, and boolean prompt kinds are supported in the first release; other kinds are out of scope.
- Default values may reference built-in context variables or earlier prompt answers and are resolved sequentially.
- The project constitution applies: no unsafe code, typed errors, path-safety validation, and deterministic ordering.
- The prompt manifest is a `[prompts]` section inside a pack-level `truss.toml` file in the template root. A standalone `prompts.toml` is out of scope for the first release.
- Custom prompts are strictly additive and cannot override built-in context variables (`project_name`, `author`, `license`, `edition`, `repository`). A prompt that declares a variable whose name collides with a built-in is a validation error.
- Conditional prompts in the first release support only simple dependencies: a prompt is shown only when a previous prompt answer equals one of a declared set of values. Arbitrary boolean expressions are out of scope.

## Out of Scope

- Interactive TUI widgets beyond text/choice/boolean prompts.
- Prompt answers persisted across projects or in user preferences.
- Internationalization of prompt labels.
- Secret/hidden prompts (passwords) in the first release.
- Runtime prompt validation that requires network or external services.
