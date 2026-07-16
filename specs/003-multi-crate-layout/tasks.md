# Tasks: Multi-Crate Scaffold Layouts

**Input**: Design documents from `/specs/003-multi-crate-layout/`
**Prerequisites**: spec.md, plan.md, checklists/requirements.md

## Phase 1: Setup

- [x] T001 Create feature branch `003-multi-crate-layout` and Spec Kit docs under `specs/003-multi-crate-layout/`

## Phase 2: Foundational

- [ ] T002 [P] Add `crates/truss-core/src/layout.rs` with `Layout` / `LayoutMember` structs, `layout.toml` parsing, and validation
- [ ] T003 [P] Add `add_workspace_member_with_deps` in `crates/truss-core/src/workspace.rs` to write inter-crate path dependencies
- [ ] T004 [P] Add optional `layout` field to `Template` and filter `layout.toml` from generated files in `crates/truss-core/src/template.rs`
- [ ] T005 [P] Wire `new_workspace` / `new_workspace_with` to detect and apply layouts

## Phase 3: User Story 1 — Generate a multi-crate workspace (P1)

- [ ] T006 [US1] Implement `layout::apply_layout` to scaffold all declared members and update root `Cargo.toml`
- [ ] T007 [P] [US1] Integration test: generate from a `monorepo` layout and assert root `Cargo.toml`, member directories, and `cargo check` pass
- [ ] T008 [P] [US1] Test: `truss new` falls back to single-member behavior when no `layout.toml` is present

## Phase 4: User Story 2 — Wire inter-crate path dependencies (P1)

- [ ] T009 [US2] Compute relative path strings for member-to-member dependencies and write them into each member `Cargo.toml`
- [ ] T010 [P] [US2] Integration test: a binary member depends on a lib member and `cargo check` succeeds
- [ ] T011 [P] [US2] Test: unknown dependency or self-dependency fails closed before any files are written

## Phase 5: User Story 3 — Author layout descriptors in template packs (P2)

- [ ] T012 [US3] Add `crates/truss-cli/templates/monorepo/` with a `layout.toml` and root workspace files
- [ ] T013 [P] [US3] Test: `truss new myapp --template monorepo` from the CLI produces a working workspace
- [ ] T014 [P] [US3] Test: invalid `layout.toml` produces an actionable error

## Phase 6: User Story 4 — Support monorepo directory conventions (P2)

- [ ] T015 [US4] Verify `apps/`, `libs/`, and `tools/` member paths are generated correctly
- [ ] T016 [P] [US4] Integration test: member paths with nested directories preserve the declared structure

## Phase 7: Safety & Quality

- [ ] T017 [P] Test: layout with duplicate member names or paths fails closed
- [ ] T018 [P] Test: layout member path that escapes the workspace root is rejected
- [ ] T019 [P] Test: layout preserves existing path-safety behavior for symlinks and existing files
- [ ] T020 Update `docs/CLI.md` / `README.md` via `just docs` if CLI help or template list changed
- [ ] T021 `cargo clippy --all-features -- -D warnings` and `cargo nextest run --workspace --no-fail-fast`
- [ ] T022 Conventional commits on branch and open PR

## Dependencies

- T002–T004 are foundational for all user stories.
- US2 depends on US1.
- US3 depends on T012 (monorepo template).
- US4 is validated by T015 and T016.
- T017–T019 run once the core layout path exists.

## MVP

T002–T009 deliver the core multi-crate generation and dependency wiring.
