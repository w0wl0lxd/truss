# Implementation Plan: Multi-Crate Scaffold Layouts

**Branch**: `003-multi-crate-layout` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)

## Summary

Allow `truss new` to expand a template into a multi-crate workspace when the
template contains a `layout.toml` descriptor. The descriptor declares members
with their kind, relative path, and optional inter-crate path dependencies.
The implementation reuses the existing member-scaffolding logic from
`002-workspace-members` and extends it to wire `path` dependencies between
members.

## Technical Context

**Language/Version**: Rust edition 2024, workspace toolchain
**Primary dependencies**: clap, toml_edit, minijinja, serde, thiserror, color-eyre
**Storage**: `layout.toml` inside the template pack; generated root `Cargo.toml`
  and member `Cargo.toml` files
**Testing**: cargo nextest, tempfile integration tests, CLI tests, `cargo check`
  on generated workspaces
**Target platform**: Linux primary

**Project type**: CLI + library workspace
**Constraints**: fail closed, no unwrap/panic, no `std::collections::HashMap` or
  `HashSet`, pathsafe on all writes, preserve TOML formatting
**Scale**: workspaces with tens of members per layout

## Constitution Check

- Fail closed / typed errors: yes
- No AI attribution: commits will follow CONTRIBUTING
- Path safety: every member path validated with `ensure_under_root` and symlink checks
- Idempotent edits: `toml_edit` array dedup; layout descriptor is read-only metadata
- Test-first for safety surfaces: layout validation and path dependency tests required

**Gate**: PASS

## Project Structure

### Documentation (this feature)

```text
specs/003-multi-crate-layout/
├── spec.md
├── plan.md
├── tasks.md
└── checklists/
    └── requirements.md
```

### Source code

```text
crates/truss-core/src/
  layout.rs           # NEW: Layout/LayoutMember parsing and validation
  template.rs         # MOD: Template carries optional Layout; filter layout.toml
  workspace.rs        # MOD: add_workspace_member_with_deps for path deps
  lib.rs              # MOD: export layout helpers; route new_workspace through layout
  sync.rs             # unchanged; root files still rendered by sync
crates/truss-cli/src/main.rs   # unchanged for this feature
crates/truss-cli/templates/monorepo/  # NEW: embedded layout template
crates/truss-core/tests/       # integration tests for layout generation
crates/truss-cli/tests/        # CLI tests for `truss new --template monorepo`
```

## Complexity Tracking

- New `layout.rs` module in `truss-core` (~250 lines).
- `Template` gains an optional `layout` field and removes `layout.toml` from generated files.
- `add_workspace_member` refactored to a `with_deps` variant for path dependencies.
- `new_workspace`/`new_workspace_with` branch on `template.layout`.
- New embedded `monorepo` template.
- No new third-party dependencies.

## Layout Descriptor Schema

`layout.toml` lives at the template root and uses an array-of-tables form:

```toml
[[members]]
name = "app"
kind = "bin"
path = "apps/app"
deps = ["shared"]

[[members]]
name = "shared"
kind = "lib"
path = "libs/shared"
```

- `name`: crate name (validated by `workspace::validate_member_name`).
- `kind`: `lib` or `bin`.
- `path`: optional member directory relative to workspace root; defaults to `crates/<name>`.
- `deps`: optional list of member names this crate depends on.

## Implementation Notes

1. **Parsing**: `layout.toml` is parsed when `Template::load` or
   `Template::from_directory` runs. The file is removed from the template's file
   list so it is never copied into the generated workspace.
2. **Validation**: The layout is validated before any file is written.
   Failures include duplicate names, duplicate paths, unknown dependencies,
   path escapes, and invalid member names.
3. **Generation flow**: `new_workspace` first runs `sync_workspace` to render root
   files, then iterates the layout members in declaration order and calls
   `add_workspace_member_with_deps`. Each member's `Cargo.toml` receives a
   `[dependencies]` table with `dep_name = { path = "<relative path>" }` for
   every declared dependency.
4. **Relative path computation**: For a dependency `dep` with path `dep_path`,
   the dependency string in a member at `member_path` is computed as
   `../` repeated for each directory level from `member_path` up to the common
   ancestor, followed by the remaining `dep_path` segments. This mirrors Cargo's
   path-dependency semantics.
5. **Path safety**: Member paths are normalized, checked for `..` traversal, and
   validated to stay under the workspace root. Symlinked ancestors below the root
   are blocked by the existing `add_workspace_member` checks.
6. **Dry-run**: `new_workspace_with` with `dry_run = true` and a layout present
   currently returns an explicit error. `truss new` has no `--dry-run` flag, so
   this surface is not exposed to CLI users for this phase.
