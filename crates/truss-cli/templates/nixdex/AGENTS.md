# AGENTS.md

`{{ project_name }}` reimplements nix package indexing as a fast Rust
workspace. Scope: evaluate package sets, build indexes, query them.

## Constraints

- No AI attribution in commits, PRs, or docs.
- No `unwrap` / `expect` / `panic` / `todo` / `unsafe` in library or app code.
- Prefer streaming eval (`nix-eval-jobs`) over materializing full trees in RAM.
- Never commit secrets, `.env`, or credentials.

## Layout

- `crates/app` — CLI and orchestration.
- Eval pipelines use `nix-eval-jobs` from the flake devShell.

Author: {{ author }}
License: {{ license }}
