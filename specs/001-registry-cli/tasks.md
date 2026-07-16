# Tasks: Registry CLI

**Input**: Design documents from `/specs/001-registry-cli/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

## Phase 1: Setup

- [x] T001 Ensure feature branch `001-registry-cli` and Spec Kit docs exist under `specs/001-registry-cli/`

## Phase 2: Foundational

- [x] T002 [P] Add `ProtectList` load/parse in `crates/truss-core/src/protect.rs` and export from `lib.rs`
- [x] T003 Extend `Registry` with `remove`, `user_path`, source validation on `add` in `crates/truss-core/src/registry.rs`
- [x] T004 Extend `sync` with plan/dry-run/protect in `crates/truss-core/src/sync.rs`

## Phase 3: User Story 1 — List (P1)

- [x] T005 [US1] Add `templates` and `registry list` CLI in `crates/truss-cli/src/main.rs`
- [x] T006 [P] [US1] Core unit/integration tests for listing embedded + registry entries

## Phase 4: User Story 2 — Add (P1)

- [x] T007 [US2] Implement `registry add` CLI with kind/source/force/target
- [x] T008 [P] [US2] Tests: add valid dir, missing path fails, force replace

## Phase 5: User Story 3 — Remove (P2)

- [x] T009 [US3] Implement `registry remove` CLI
- [x] T010 [P] [US3] Tests: remove existing; missing name errors

## Phase 6: User Story 4 — Dry-run & protect (P2)

- [x] T011 [US4] Wire `--dry-run` and `--protect` on sync (and document)
- [x] T012 [P] [US4] Tests: dry-run no disk change; protected file preserved

## Phase 7: Polish

- [x] T013 Update `README.md` / `CLAUDE.md` local docs with registry commands
- [x] T014 `cargo clippy --all-features -- -D warnings` and `cargo nextest run --workspace --no-fail-fast`
- [ ] T015 Conventional commits on branch and open PR

## Dependencies

- T002–T004 before CLI stories
- US1/US2 can proceed after T003
- US4 depends on T002 + T004

## MVP

T002–T008 (list + add with tests) deliver standalone value.
