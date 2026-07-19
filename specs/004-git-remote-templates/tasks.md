# Tasks: Git-based Remote Templates

**Input**: Design documents from `/specs/004-git-remote-templates/`

**Prerequisites**: spec.md, plan.md, data-model.md, research.md, checklists/requirements.md

## Phase 1: Setup

- [x] T001 [P] Create `004-git-remote-templates` branch and add a `git` binary availability check to `crates/truss-core/src/git.rs` (no new Cargo dependencies)
- [x] T002 [P] Add `GitCache` root directory helper and registry `GitCache` key sanitization in `crates/truss-core/src/git.rs`

## Phase 2: Foundational

- [x] T003 Add `Kind::Git` to `crates/truss-core/src/registry.rs` and extend `RegistryEntry` with optional `subfolder` field
- [x] T004 [P] Validate `git` registry entries: URL syntax, no local/file paths, normalized `subfolder`, empty `targets`
- [x] T005 Add `Error` variants for git/network/cache failures in `crates/truss-core/src/error.rs`
- [x] T006 [P] Implement `git.rs` `GitUrl` parsing and shorthand expansion (`gh:`, `gl:`, `bb:`, `sr:`, `owner/repo`)

## Phase 3: User Story 1 — Register and use a remote Git template (P1)

- [x] T007 [US1] Implement `GitCache::resolve` to clone or fetch a remote repository into the cache directory using the `git` CLI
- [x] T008 [US1] Implement ref resolution with `git rev-parse` and fail closed on missing refs
- [x] T009 [US1] Wire `RegistryEntry::to_template` for `Kind::Git` to resolve cache and load the worktree as a `dir` template
- [x] T010 [P] [US1] Integration test: register a local bare repo via `file://` URL and run `truss new` successfully
- [x] T011 [P] [US1] Integration test: `truss sync --dry-run` reports no drift for an up-to-date git template
- [x] T012 [US1] Update `truss-core/tests/registry_protect.rs` to verify `dir` and `file` entries are unaffected

## Phase 4: User Story 2 — Pin a ref and select a subfolder (P1)

- [x] T013 [US2] Add `pointer` resolution to `GitCache::resolve` and CLI `--pointer` flag to `truss registry add`
- [x] T014 [US2] Add `subfolder` support and CLI `--subfolder` flag to `truss registry add`
- [x] T015 [US2] Update `Template::from_directory` to skip `.git/` directory contents
- [x] T016 [P] [US2] Integration test: `truss new` from a specific tag ref and subfolder produces expected files
- [x] T017 [P] [US2] Integration test: missing ref or missing subfolder fails before writing files

## Phase 5: User Story 3 — Cache and update remote templates (P2)

- [x] T018 [US3] Implement fetch-on-use in `GitCache::resolve` to update an existing cache
- [ ] T019 [US3] Cache fallback: use cached worktree when network is unavailable and ref is present
- [x] T020 [US3] Remove cached directory on `truss registry remove <name>`
- [x] T021 [P] [US3] Integration test: second `truss new` against the same entry does not perform a full re-clone
- [x] T022 [P] [US3] Integration test: cache is removed when the registry entry is removed

## Phase 6: User Story 4 — Shorthand Git URLs (P3)

- [x] T023 [US4] Expand `gh:`, `gl:`, `bb:`, and `sr:` shorthands in `GitUrl` parsing
- [x] T024 [US4] Treat bare `owner/repo` as GitHub shorthand with explicit documentation
- [x] T025 [P] [US4] Unit tests for shorthand expansion and full URL passthrough

## Phase 7: Safety, Quality, and Documentation

- [x] T026 [P] Test: path traversal in `subfolder` is rejected
- [ ] T027 [P] Test: `file://` and local filesystem paths are rejected for `kind = "git"`
- [x] T028 Update `docs/REGISTRY.md` and `docs/TEMPLATES.md` with `git` registry entries
- [x] T029 Regenerate `README.md` and `docs/CLI.md` via `just docs` if CLI help changes
- [x] T030 Run `cargo fmt --all`, `cargo clippy --all-features -- -D warnings`, and `cargo nextest run --workspace --no-fail-fast`
- [x] T031 Conventional commits and open PR for `004-git-remote-templates`

## Dependencies

- T001–T006 are foundational for all user stories.
- US2 depends on US1 (T007–T012).
- US3 depends on US1.
- US4 depends on T006 and US1.
- T026–T027 can run in parallel once core git resolution exists.

## MVP

T001–T012 deliver the core git-based template generation and preserve existing registry behavior.
