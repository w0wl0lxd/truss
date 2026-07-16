# Tasks: Workspace Members

**Input**: Design documents from `/specs/002-workspace-members/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

## Phase 1: Setup

- [ ] T001 Create feature branch `002-workspace-members` and Spec Kit docs under `specs/002-workspace-members/`

## Phase 2: Foundational

- [ ] T002 [P] Add `crates/truss-core/src/workspace.rs` with Cargo.toml loading, member path validation, and idempotent member insertion
- [ ] T003 [P] Add member scaffold helpers (lib and bin `Cargo.toml` + source file rendering) using the existing rendering context and engine
- [ ] T004 [P] Add `truss_core::workspace` exports in `crates/truss-core/src/lib.rs`

## Phase 3: User Story 1 — Add lib crate (P1)

- [ ] T005 [US1] Implement `truss member add <name> --kind lib [--member-path <REL>] [--path <DIR>]` CLI in `crates/truss-cli/src/main.rs`
- [ ] T006 [P] [US1] Integration test: add lib to fresh project, assert `workspace.members` and file tree
- [ ] T007 [P] [US1] Idempotency test: re-run `member add` and assert no duplicate member or overwritten files

## Phase 4: User Story 2 — Add bin crate (P1)

- [ ] T008 [US2] Implement `--kind bin` scaffold (`src/main.rs`) with `--member-path` support
- [ ] T009 [P] [US2] Integration test: add bin and run `cargo check`

## Phase 5: User Story 3 — List members (P2)

- [ ] T010 [US3] Implement `truss member list [--path <DIR>]` CLI
- [ ] T011 [P] [US3] Test: list members, empty list, missing `[workspace]` error

## Phase 6: User Story 4 — Remove members (P2)

- [ ] T012 [US4] Implement `truss member remove <name> [--path <DIR>] [--delete]` with workspace-root `--path`
- [ ] T013 [P] [US4] Test: remove preserves directory; remove --delete removes directory; missing member errors

## Phase 7: Safety & Quality

- [ ] T014 [P] Test: `member add` fails closed when root `Cargo.toml` has no `[workspace]` table
- [ ] T015 [P] Test: `member remove --delete` refuses to follow a path outside the workspace root
- [ ] T016 [P] Test: `workspace.members` formatting and inline comments are preserved across add/remove
- [ ] T017 [P] Test: `member add` rejects a member path that escapes the workspace root or collides with an existing non-directory file
- [ ] T018 Update `docs/CLI.md` / `README.md` via `just docs` with new member commands
- [ ] T019 `cargo clippy --all-features -- -D warnings` and `cargo nextest run --workspace --no-fail-fast`
- [ ] T020 Conventional commits on branch and open PR

## Dependencies

- T002–T004 before CLI stories
- US1/US2 can proceed after T004
- US4 depends on T002
- T014–T016 can run in parallel with UI tests once the core commands exist

## MVP

T002–T009 (lib/bin add with idempotency and build tests) deliver standalone value.
