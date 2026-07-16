# Feature Specification: Multi-Crate Scaffold Layouts

**Feature Branch**: `003-multi-crate-layout`

**Created**: 2026-07-16

**Status**: Draft

**Input**: Allow `truss new` to generate workspaces with multiple pre-defined
members from a layout descriptor stored in a template pack. Support common
monorepo conventions (`apps/`, `libs/`, `tools/`) and wire inter-crate path
dependencies at generation time. The descriptor may be provided as a dedicated
`layout.toml` file or as frontmatter in the template entry point.

## Clarifications

### Session 2026-07-16

- **Q1**: What descriptor format should the specification target?
  - **A1**: The feature is descriptor-format-agnostic. Concrete support starts
    with a `layout.toml` file in the template root because it is explicit and
    version-controllable. Frontmatter support may be added later if template
    authors prefer a single entry-point file.
- **Q2**: Does the layout descriptor apply to `truss sync` or `truss check`?
  - **A2**: No. The descriptor is consumed only during `truss new` to expand a
    template into a multi-member workspace. `sync` and `check` continue to
    compare existing files against the pack; they do not add, remove, or re-wire
    workspace members.
- **Q3**: Can a user generate only a subset of the declared layout members?
  - **A3**: No. `truss new` generates every member declared in the layout. A
    selector flag is out of scope for this phase.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Generate a multi-crate workspace from a layout (Priority: P1)

A developer runs `truss new myapp --template monorepo` where the `monorepo`
template includes a layout descriptor. `truss new` scaffolds the root workspace,
then generates every declared member crate, updates the root `Cargo.toml`
`workspace.members` array, and wires declared inter-crate dependencies.

**Why this priority**: This is the core value of the phase — turning
`truss new` from a single-crate scaffold into a one-command monorepo bootstrap.

**Independent Test**: Instantiate a template with a layout containing at least
three members across different directories (`apps/`, `libs/`, `tools/`), then
run `cargo check` in the generated workspace and assert it builds without
manual edits.

**Acceptance Scenarios**:

1. **Given** a template with a valid layout descriptor, **when** `truss new
   myapp --template <name>` runs, **then** the generated workspace contains all
   declared member crates, the root `Cargo.toml` lists every member path, and
   `cargo check` succeeds.
2. **Given** a layout member declared at `libs/shared`, **when** the workspace is
   generated, **then** the directory `myapp/libs/shared` exists with a valid
   `Cargo.toml` and `src/lib.rs` (or `src/main.rs` for binaries).
3. **Given** a template with no layout descriptor, **when** `truss new` runs,
   **then** behavior matches the current single-member scaffold.

---

### User Story 2 — Wire inter-crate path dependencies (Priority: P1)

The layout descriptor declares dependencies between members (e.g., `apps/cli`
depends on `libs/shared`). At generation time, `truss new` writes the correct
`[dependencies]` or `[dependencies.workspace]` entries in each member
`Cargo.toml` so the workspace compiles as a unit.

**Why this priority**: A multi-crate workspace without wired dependencies is
just a collection of folders. Path dependencies are what make the layout
useful.

**Independent Test**: Create a layout where one lib is consumed by an app and a
tool, instantiate it, and assert `cargo check` resolves both dependency edges.

**Acceptance Scenarios**:

1. **Given** a layout where `apps/cli` declares a dependency on `libs/shared`,
   **when** the workspace is generated, **then** `apps/cli/Cargo.toml` contains a
   path dependency pointing to `libs/shared` and `cargo check` succeeds.
2. **Given** a member that has no declared dependencies, **when** its
   `Cargo.toml` is generated, **then** the dependency table is absent or empty.
3. **Given** a layout dependency that references a member not declared in the
   layout, **when** `truss new` runs, **then** it fails closed with a clear
   error before writing any files.

---

### User Story 3 — Author layout descriptors in template packs (Priority: P2)

A template maintainer adds a layout descriptor to a template pack, placing it in
the template root (e.g., `layout.toml`). The descriptor lists members, their
kinds, their target directories, and optional inter-crate dependencies. No
pack code changes are required beyond adding the descriptor and any supporting
template files.

**Why this priority**: Layouts must be data-driven so teams can maintain their
own monorepo templates without forking `truss`.

**Independent Test**: Copy the default template, add a `layout.toml`, register
it as a local pack, and run `truss new` against it.

**Acceptance Scenarios**:

1. **Given** a template pack with a syntactically valid layout descriptor,
   **when** `truss new` uses the pack, **then** the descriptor is loaded and the
   workspace is generated according to its contents.
2. **Given** a template pack with an invalid or missing required field in the
   layout descriptor, **when** `truss new` runs, **then** it fails before writing
   the workspace and reports which descriptor or field is invalid.
3. **Given** a template pack without a layout descriptor, **when** `truss new`
   runs, **then** it falls back to the existing single-member behavior.

---

### User Story 4 — Support common monorepo directory conventions (Priority: P2)

Layouts may organize members under conventional directories such as `apps/`,
`libs/`, and `tools/`. The directory structure described in the layout is
preserved during scaffold.

**Why this priority**: `apps/`, `libs/`, and `tools/` are the most common
monorepo layouts in the Rust ecosystem; supporting them makes templates
immediately recognizable to users.

**Independent Test**: Generate a layout with one member in each of `apps/`,
`libs/`, and `tools/` and assert the directories are created and the root
`Cargo.toml` contains the correct relative paths.

**Acceptance Scenarios**:

1. **Given** a layout declaring `apps/cli`, `libs/shared`, and `tools/dev`,
   **when** `truss new` runs, **then** those directories are created and
   `workspace.members` lists `apps/cli`, `libs/shared`, and `tools/dev`.
2. **Given** a layout member declared at a custom relative path, **when** the
   workspace is generated, **then** the member is created at that path and added
   to `workspace.members` using the same relative path.

---

### Edge Cases

- A layout descriptor references a path outside the generated workspace root.
- A layout declares a member at a path that conflicts with a pack file or a
  previously scaffolded member.
- A layout declares duplicate member names or paths.
- A layout member kind is unsupported or missing.
- A template pack provides both a layout descriptor and a single-member
  template; the layout descriptor takes precedence.
- A `Cargo.toml` produced by the layout contains `workspace = true` dependencies
  without a matching `[workspace.dependencies]` table.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `truss new` MUST detect a layout descriptor in the selected
  template pack.
- **FR-002**: When a layout descriptor is present, `truss new` MUST generate the
  root workspace and every declared member before returning.
- **FR-003**: The root `Cargo.toml` MUST include every layout member in
  `workspace.members`.
- **FR-004**: Each member crate MUST be scaffolded at the path declared in the
  layout descriptor.
- **FR-005**: Member `Cargo.toml` files MUST declare path dependencies for every
  inter-crate dependency listed in the layout descriptor.
- **FR-006**: The layout descriptor MUST support common member directories
  including, but not limited to, `apps/`, `libs/`, and `tools/`.
- **FR-007**: `truss new` MUST fail closed when the layout descriptor is invalid,
  references an undeclared member, or would place files outside the workspace
  root.
- **FR-008**: When no layout descriptor is present, `truss new` MUST fall back to
  the existing single-member scaffold behavior.
- **FR-009**: Path dependencies written by `truss new` MUST be relative to the
  member crate and resolve correctly within the generated workspace.
- **FR-010**: Generation MUST be idempotent at the workspace root level:
  re-running `truss new` on an existing directory must be rejected or guarded by
  existing protections; it must not silently corrupt a generated layout.

### Key Entities

- **Layout descriptor**: A template-pack metadata artifact that lists members,
  their kinds, target directories, and inter-crate dependencies.
- **Layout member**: A single crate declared in the layout, with a name, kind,
  target path, and optional dependency list.
- **Generated workspace**: The output directory produced by `truss new`,
  containing the root `Cargo.toml` and all member crates.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A layout with at least one `apps/`, one `libs/`, and one `tools/`
  member can be generated in a single `truss new` invocation and passes
  `cargo check` without manual edits.
- **SC-002**: Adding a layout descriptor to an existing template pack produces a
  multi-member workspace with no changes to `truss` source code.
- **SC-003**: All inter-crate dependencies declared in a layout resolve and
  compile in the generated workspace.
- **SC-004**: `truss new` fails closed on an invalid layout descriptor with an
  actionable error message.
- **SC-005**: Existing single-member templates continue to work exactly as they
  do today.

## Assumptions

- The template pack format already supports a root metadata file (e.g.,
  `layout.toml`) or frontmatter in a recognized entry file.
- Members are Cargo crates; path dependencies use Cargo's standard `[dependencies]`
  or `[dependencies.workspace]` forms.
- Layout descriptors do not require network access; all dependencies are either
  path dependencies or already-resolved external crates.
- This feature depends on the workspace member scaffolding introduced in
  `002-workspace-members`.

## Out of Scope

- Reconciling a layout descriptor against an existing workspace during `truss sync`
  or `truss check`.
- Generating a subset of layout members at `new` time.
- Adding, removing, or renaming members after the initial scaffold.
- Git-based or registry-based remote templates (covered in a future phase).
- Build-tool specific files beyond standard Cargo crate scaffolding.
