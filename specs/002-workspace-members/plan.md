# Implementation Plan: Workspace Members

**Branch**: `002-workspace-members` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)

## Summary

Add `truss member add|list|remove` commands. `add` edits the root `Cargo.toml`
`workspace.members` array and scaffolds a minimal member crate. `list` prints
members. `remove` deletes the entry and optionally the directory. `--path` is the
workspace root (consistent with `sync`/`check`); `add` additionally supports
`--member-path` to override `crates/<name>`. All edits are idempotent and preserve
formatting and comments.

## Technical Context

**Language/Version**: Rust edition 2024, workspace toolchain
**Primary dependencies**: clap, toml_edit, thiserror, color-eyre
**Storage**: root `Cargo.toml` in the target project
**Testing**: cargo nextest, tempfile integration tests
**Target platform**: Linux primary

**Project type**: CLI + library workspace
**Constraints**: fail closed, no unwrap/panic, no HashMap, pathsafe on all writes,
  preserve TOML formatting
**Scale**: workspaces with ≤ hundreds of members

## Constitution Check

- Fail closed / typed errors: yes
- No AI attribution: commits will follow CONTRIBUTING
- Path safety: member paths validated with `ensure_under_root`
- Idempotent edits: `toml_edit` array dedup
- Test-first for safety surfaces: member add/list/remove tests required

**Gate**: PASS

## Project Structure

### Documentation (this feature)

```text
specs/002-workspace-members/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── cli.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source code

```text
crates/truss-core/src/
  lib.rs              # export member add/list/remove
  workspace.rs        # NEW: Cargo.toml member editing + member scaffolding
crates/truss-cli/src/main.rs  # `member` subcommand
crates/truss-core/tests/      # integration tests
crates/truss-cli/tests/       # CLI tests
```

## Complexity Tracking

- New module `workspace.rs` in `truss-core`.
- Three new CLI subcommands under `member`.
- No new third-party dependencies (uses existing `toml_edit`).

## Phase 0 / 1 outputs

See research.md, data-model.md, contracts/cli.md, quickstart.md.
