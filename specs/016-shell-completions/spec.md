# Feature Specification: Shell Completions

**Feature Branch**: `016-shell-completions`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Add shell completion generation to `truss` so users can generate completion scripts for bash, zsh, fish, and PowerShell and autocomplete commands, flags, and template names."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Generate a completion script for a target shell (Priority: P1)

A developer runs `truss completions --shell zsh` and receives a completion script on stdout that they can redirect to their shell's completion directory. The script works in the target shell for all static commands and flags.

**Why this priority**: Generating the script is the core value of the feature and can be tested independently.

**Independent Test**: Run `truss completions --shell bash` and verify the output begins with a valid bash completion function declaration.

**Acceptance Scenarios**:

1. **Given** a request for `bash` completions, **when** the command runs, **then** the output is a valid bash completion script for `truss`.
2. **Given** a request for `zsh` completions, **when** the command runs, **then** the output is a valid zsh completion script.
3. **Given** a request for an unsupported shell, **when** the command runs, **then** it fails with a typed error listing supported shells.

---

### User Story 2 - Static command and flag completion (Priority: P1)

A user presses tab after typing `truss ` in a supported shell and sees the top-level commands (new, sync, check, registry, etc.). They press tab after `truss new --` and see all available flags.

**Why this priority**: Command and flag completion are the minimum expected behavior for shell completions.

**Independent Test**: Source the generated bash script, type `truss <TAB><TAB>`, and verify the list includes all documented subcommands.

**Acceptance Scenarios**:

1. **Given** the generated script is sourced in bash, **when** the user types `truss ` and presses tab, **then** all top-level commands are suggested.
2. **Given** the generated script is sourced in zsh, **when** the user types `truss new --` and presses tab, **then** all `new` flags are suggested.
3. **Given** a flag that accepts no value, **when** the user requests completion, **then** no spurious file or value suggestions are shown.

---

### User Story 3 - Dynamic completion for template names (Priority: P2)

A user presses tab after `truss new app --template ` and sees the names of all installed registry entries and embedded packs. The completion reflects the current local registry.

**Why this priority**: Template names change over time and cannot be hardcoded; dynamic completion greatly improves the CLI experience.

**Independent Test**: Add a custom registry entry, source the script, type `truss new app --template <TAB>`, and verify the custom name appears.

**Acceptance Scenarios**:

1. **Given** a registry containing `default`, `agent-rules`, and a custom `team-pack`, **when** the user requests completion for `--template`, **then** all three names are suggested.
2. **Given** a registry entry is removed, **when** the user requests completion, **then** the removed name no longer appears.
3. **Given** no registry file exists, **when** the user requests completion, **then** the embedded pack names are still suggested.

---

### User Story 4 - Dynamic completion for variable names and shell presets (Priority: P2)

A user presses tab after `truss new app --type ` and sees the available project type presets. When completing variables, the script suggests variable names for the selected template where feasible.

**Why this priority**: Extends dynamic completion to project presets and reduces typing for frequently used variables.

**Independent Test**: With presets `binary`, `library`, and `workspace` configured, type `truss new app --type <TAB>` and verify all three are suggested.

**Acceptance Scenarios**:

1. **Given** built-in presets `binary` and `library`, **when** the user requests completion for `--type`, **then** both presets appear.
2. **Given** a custom preset `service`, **when** the user requests completion, **then** the custom preset appears alongside built-ins.
3. **Given** a template that declares variables, **when** the user requests completion after a documented variable prefix, **then** variable names are suggested where the shell supports it.

---

### User Story 5 - Install completions into a system directory (Priority: P3)

A user runs a command that writes the completion script directly to the correct location for their shell (e.g., `/usr/share/zsh/site-functions` or `~/.config/fish/completions`) so they do not need to manually copy the file.

**Why this priority**: Nice-to-have convenience, especially for package managers and installers. Not required for basic script generation.

**Independent Test**: Run `truss completions --shell fish --install` and verify the file is written to the user's fish completions directory.

**Acceptance Scenarios**:

1. **Given** a supported shell with a known user-level completion directory, **when** `--install` is requested, **then** the script is written to that directory.
2. **Given** a shell with no known completion directory, **when** `--install` is requested, **then** the command fails and prints instructions for manual installation.
3. **Given** the target file already exists, **when** `--install` is requested without force, **then** the command fails; with `--force`, the file is overwritten.

### Edge Cases

- What happens when the user requests a shell that is not supported?
- What happens when the generated script is sourced in a shell older than the one targeted?
- What happens when dynamic completion is requested but the registry file is malformed?
- What happens when a template name contains spaces or special characters?
- What happens when the completion script is generated for `powershell` on a non-Windows system?
- What happens when a flag has both a long and short form?
- What happens when the user requests completion for a subcommand that does not exist?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a `completions` command that generates a completion script for a requested shell.
- **FR-002**: System MUST support `bash`, `zsh`, `fish`, and `PowerShell` as target shells.
- **FR-003**: The generated script MUST complete all top-level `truss` subcommands.
- **FR-004**: The generated script MUST complete flags for each subcommand.
- **FR-005**: The generated script MUST dynamically complete `--template` values from the local registry and embedded packs.
- **FR-006**: The generated script MUST dynamically complete `--type` values from the project-type preset registry.
- **FR-007**: System MUST output the completion script to stdout by default.
- **FR-008**: System MUST support an optional `--install` flag that writes the script to the shell's user completion directory when the location is known.
- **FR-009**: System MUST fail closed with a typed error for unsupported shells or when install target directory cannot be determined.
- **FR-010**: System MUST produce deterministic completion script output for the same CLI version and configuration.
- **FR-011**: System MUST preserve long and short flag aliases in the generated script.
- **FR-012**: System MUST handle dynamic completion failures (e.g., malformed registry) gracefully, falling back to static command/flag completion.

### Key Entities

- **CompletionScript**: A shell-specific script that registers tab-completion behavior for `truss`. Attributes: target shell, command list, flag mappings, dynamic completion hooks.
- **ShellTarget**: A supported shell environment. Attributes: name, user completion directory path, script syntax.
- **CompletionContext**: The runtime state used for dynamic completion. Attributes: current command, current word, local registry entries, preset list.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The generated completion script for each supported shell can be sourced without syntax errors in the target shell.
- **SC-002**: Tab completion lists all documented `truss` subcommands and their flags.
- **SC-003**: Tab completion for `--template` reflects the current local registry within 500 milliseconds.
- **SC-004**: Tab completion for `--type` reflects the current preset list within 500 milliseconds.
- **SC-005**: Generation of a completion script completes in under 1 second.

## Assumptions

- Shell completions are generated from the CLI's command and flag definitions, not hand-maintained per shell.
- Dynamic completion relies on the existing registry and preset configuration; no network is required.
- The install target is the user's local shell completion directory; system-wide installation requires manual steps or elevated privileges.
- PowerShell completion uses the `Register-ArgumentCompleter` mechanism.
- Static completions are deterministic and ordered; dynamic completions use ordered registry entries.

## Out of Scope

- Automatic activation of completions after package installation (e.g., modifying the user's shell profile).
- Completions for arbitrary user-defined aliases or third-party plugins.
- Network-based dynamic completion (e.g., fetching marketplace template names live from the internet).
