# Feature Specification: Generation Hooks

**Feature Branch**: `009-generation-hooks`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Template packs should be able to declare lifecycle hooks (pre-generation and post-generation commands) that `truss` runs around `truss new`, `truss sync`, and `truss update` so pack authors can automate tasks like formatting, dependency installation, or validation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run a post-generation hook after scaffolding (Priority: P1)

A pack author adds a hook that runs `cargo fmt` after `truss new` so generated code is immediately formatted. When a developer runs `truss new demo --template my-pack`, the project is scaffolded and then the hook runs inside the project directory.

**Why this priority**: Post-generation hooks are the most common need and provide immediate value (formatting, license header application, etc.).

**Independent Test**: Add a pack with a post-generation hook, run `truss new`, and verify the hook executed (e.g., a marker file or formatted output was produced).

**Acceptance Scenarios**:

1. **Given** a pack with a post-generation hook, **when** `truss new` completes file writes, **then** the hook runs with the project directory as its working directory.
2. **Given** a post-generation hook that returns a zero exit code, **when** the command completes, **then** `truss new` reports success.
3. **Given** a post-generation hook that returns a non-zero exit code, **when** `truss new` runs, **then** the command reports the hook failure and does not claim success.

---

### User Story 2 - Run a pre-generation validation hook (Priority: P2)

A pack author wants to ensure the environment has a required toolchain (e.g., `rustup`) before generating files. The pre-generation hook runs before any file is written and can abort the operation.

**Why this priority**: Pre-generation hooks prevent broken scaffolding but require the core post-generation story to be in place first.

**Independent Test**: Add a pack with a pre-generation hook that checks for a missing executable, run `truss new`, and verify no files are written and a clear error is shown.

**Acceptance Scenarios**:

1. **Given** a pre-generation hook that exits zero, **when** `truss new` runs, **then** file generation proceeds normally.
2. **Given** a pre-generation hook that exits non-zero, **when** `truss new` runs, **then** no project files are written and the hook's error output is shown.
3. **Given** `truss new --dry-run`, **when** a pre-generation hook is configured, **then** the hook is listed in the dry-run plan but not executed.

---

### User Story 3 - Conditional and command-specific hooks (Priority: P3)

A pack author wants a hook to run only when scaffolding a new project, not during `truss sync`, or only when the user chose a specific prompt option. Hooks support conditions and command restrictions.

**Why this priority**: Conditional hooks reduce unnecessary work and adapt pack behavior to user choices. This is a nice-to-have on top of unconditional hooks.

**Independent Test**: Define a hook restricted to `truss new` and run `truss sync`; verify the hook does not run. Define a hook conditioned on a prompt answer and verify it only runs when the condition is met.

**Acceptance Scenarios**:

1. **Given** a hook configured to run only for `truss new`, **when** `truss sync` runs, **then** the hook is skipped.
2. **Given** a hook conditioned on `include_cli = true`, **when** the user answers `include_cli` with `false`, **then** the hook is skipped.
3. **Given** a hook with no command restriction, **when** it is triggered by any supported command, **then** it runs for all of them.

### Edge Cases

- A hook command is not found on `PATH`. The command fails with a clear error before or after generation as appropriate.
- A hook produces a large amount of output. The output is streamed but not stored by default; it may be truncated in error reports.
- A hook attempts to write outside the project directory. Path-safety validation rejects the generated path before the write occurs.
- A hook attempts to execute an absolute path or a path containing `..`. The hook definition is rejected at load time.
- A hook definition references a prompt variable that does not exist. The command fails with a clear error.
- A post-generation hook fails after files have been written. The generated files remain in place and the user is informed that the hook failed.
- A pre-generation hook fails. No project files are written.
- A hook is declared with an unknown lifecycle phase. The command fails with a parse error.
- A pack has multiple hooks. They run in the order declared in the pack manifest.
- A hook runs in dry-run mode. It is listed but not executed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A pack MAY declare one or more generation hooks in a pack-level hook manifest.
- **FR-002**: The system MUST support a pre-render hook phase that runs before files are generated for `truss new`, `truss sync`, and `truss update`.
- **FR-003**: The system MUST support a post-render hook phase that runs after files are written for `truss new` and `truss sync`.
- **FR-004**: Hooks MUST run with the target project directory as the working directory.
- **FR-005**: The system MUST pass template context values to hooks as environment variables.
- **FR-006**: A hook failure MUST stop the generation process and report an actionable error. Pre-hook failures prevent any file writes; post-hook failures leave already-written files in place.
- **FR-007**: The system MUST support command restrictions so a hook runs only for specified commands (`new`, `sync`, `update`, `check`).
- **FR-008**: The system MUST support conditions based on prompt/context values.
- **FR-009**: In dry-run mode, the system MUST list hooks that would run without executing them.
- **FR-010**: The system MUST validate hook definitions for path safety and reject definitions that would execute outside the project root or pass unsafe arguments.

### Key Entities

- **HookManifest**: A pack-level ordered list of hook definitions.
- **Hook**: A single hook with a lifecycle phase, command to execute, arguments, optional environment variables, command restrictions, and an optional condition.
- **HookContext**: The set of prompt and built-in context values available to the hook as environment variables.
- **HookResult**: The outcome of a hook execution (success or failure with output).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A pack with a post-generation hook (e.g., `cargo fmt`) runs the hook automatically after `truss new` and exits zero when the hook succeeds.
- **SC-002**: A failed hook prevents the command from reporting success and surfaces the hook output clearly.
- **SC-003**: `truss new --dry-run` lists all hooks that would execute without running them.
- **SC-004**: Command-restricted hooks run only for the specified commands in 100% of test cases.
- **SC-005**: Hook execution preserves deterministic ordering and does not allow writes outside the project root.

## Assumptions

- Hooks are optional and declared in a pack manifest. Packs without hooks behave exactly as before.
- Hook commands are external executables expected to be on the user's `PATH`; `truss` does not bundle interpreters.
- Environment variables passed to hooks use upper-cased variable names derived from the prompt/context keys (e.g., `TRUSS_PROJECT_NAME`).
- A hook's arguments may be templated with context values but are validated for path safety after rendering.
- Path safety, typed errors, no unsafe code, and deterministic ordering are enforced as per the project constitution.
- [NEEDS CLARIFICATION: Should hook commands be allowed to use shell syntax (pipes, redirection, subshells), or must they be a single executable with a list of literal arguments?]
- [NEEDS CLARIFICATION: Should common lifecycle commands such as `cargo fmt` or `cargo check` be automatically inferred from the pack, or must every hook be explicitly declared by the pack author?]
- [NEEDS CLARIFICATION: Should hooks be inherited into generated projects so they re-run during `truss sync`, or do they apply only at the moment the pack is rendered?]

## Out of Scope

- Running hooks that modify the pack source itself.
- Long-running background services or daemons started by hooks.
- Hooks that require network access or fetch additional files.
- Hooks that prompt the user for input during execution.
- Sandbox/isolation of hooks beyond path-safety validation.
