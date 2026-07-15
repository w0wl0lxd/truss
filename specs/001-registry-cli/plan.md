# Implementation Plan: Registry CLI

**Branch**: `001-registry-cli` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)

## Summary

Expose the existing `Registry` library over CLI (`list` / `add` / `remove`),
plus `sync --dry-run` and a protect-list for skip-on-sync. No new network deps.
Reuse pathsafe validation and current template resolve order.

## Technical Context

**Language/Version**: Rust edition 2024, workspace toolchain  
**Primary dependencies**: clap, thiserror, color-eyre, directories, serde_json, indexmap, minijinja, rust-embed  
**Storage**: `~/.config/truss/registry.json` (user); optional read `/etc/nixos/truss/registry.json`  
**Testing**: cargo nextest, tempfile integration tests  
**Target platform**: Linux (NixOS primary)  
**Project type**: CLI + library workspace  
**Constraints**: fail closed, no unwrap/panic, no HashMap, pathsafe on all writes  
**Scale**: dozens of registry entries, templates ≤ hundreds of files  

## Constitution Check

- Fail closed / typed errors: yes  
- No AI attribution: commits will follow CONTRIBUTING  
- Path safety: registry add validates sources; protect paths validated via pathsafe  
- Deterministic sync: dry-run pure  
- Test-first for safety surfaces: registry + dry-run + protect tests required  

**Gate**: PASS  

## Project Structure

### Documentation (this feature)

```text
specs/001-registry-cli/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── cli.md
├── checklists/requirements.md
└── tasks.md
```

### Source code

```text
crates/truss-core/src/
  registry.rs      # remove(); list helper; validate source on add
  sync.rs          # dry_run + protect_list
  protect.rs       # NEW: load .truss/protect + CLI paths
  lib.rs
crates/truss-cli/src/main.rs  # Registry + templates + dry-run/protect flags
crates/truss-core/tests/
crates/truss-cli/tests/
```

## Complexity Tracking

None — sits on existing registry/sync modules.

## Phase 0 / 1 outputs

See research.md, data-model.md, contracts/cli.md, quickstart.md.
