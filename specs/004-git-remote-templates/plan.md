# Implementation Plan: Git-based Remote Templates

**Branch**: `004-git-remote-templates` | **Date**: 2026-07-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/004-git-remote-templates/spec.md`

## Summary

Add `kind = "git"` registry entries so `truss new`, `truss sync`, and `truss check` can use templates stored in remote Git repositories. The implementation extends the existing `RegistryEntry` with optional `pointer` and `subfolder` fields, resolves the repository into a local cache with `gix`, and reuses the existing `Template::from_directory` loader after excluding VCS metadata. Existing `dir` and `file` entries and embedded templates remain unchanged.

## Technical Context

**Language/Version**: Rust 1.94+ (2024 edition)

**Primary Dependencies**:

- The system `git` binary invoked through `std::process::Command` for clone, fetch, ref resolution, and checkout. A pure-Rust `gix` dependency was considered, but its compile-time and binary-size cost outweigh the benefit for a scaffolding CLI that already targets developers with Git installed.
- `directories` (already in use) to locate the platform cache directory.

**Storage**: Local filesystem cache under `$XDG_CACHE_HOME/truss/git/<entry-key>`.

**Testing**: `cargo nextest run --workspace --no-fail-fast`. Integration tests will create local bare Git repositories using the `git` CLI and register them via `file://` URLs.

**Target Platform**: Cross-platform CLI (Linux, macOS, Windows).

**Project Type**: Rust CLI/library.

**Performance Goals**: Second use of a `git` template should fetch only new commits, not perform a full clone. Cold clone performance is acceptable for a scaffolding CLI.

**Constraints**:

- Git operations are delegated to the system `git` binary via `std::process::Command` with validated arguments; no shell interpolation.
- No credentials or tokens stored by `truss`; rely on system SSH agent or public HTTPS.
- Path-safety rules from project constitution apply: `subfolder` must be normalized, `..` rejected, and generated files kept under the project root.

**Scale/Scope**: Individual team template repositories; no support for giant monorepos or submodules in the MVP.

## Constitution Check

- **Fail closed**: Invalid URLs, missing refs, and path traversal in `subfolder` must error before any write.
- **No unsafe code**: `gix` is pure Rust.
- **Path safety**: Reuse existing `normalize_relative_path` for `subfolder`; reject `file://` and local paths for `git` sources.
- **Typed errors**: Add `Error` variants for network, git, and cache failures.

## Project Structure

### Documentation (this feature)

```text
specs/004-git-remote-templates/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── tasks.md
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```text
crates/truss-core/src/
├── registry.rs          # extend Kind, RegistryEntry, validation, to_template
├── git.rs               # NEW: GitUrl, GitCache, clone/fetch/checkout helpers
├── template.rs          # skip .git/ directory in from_directory
└── lib.rs               # no changes expected

crates/truss-cli/src/main.rs  # add --subfolder and --pointer to registry add

crates/truss-core/Cargo.toml  # add gix dependency
crates/truss-cli/Cargo.toml   # inherit workspace dependencies
```

### Tests

```text
crates/truss-core/tests/
├── git_remote.rs        # NEW: integration tests for git template resolution
└── registry_protect.rs  # extend with git entry validation tests

crates/truss-cli/tests/cli.rs  # add git-template CLI tests
```

## Complexity Tracking

No constitution violations expected. The feature adds one new module and extends two existing modules.
