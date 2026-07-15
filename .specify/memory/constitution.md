# truss Constitution

## Core Principles

### I. Fail Closed, Typed Errors

`unwrap` / `expect` / `panic` / `todo` / `unimplemented` are forbidden in production
paths. Missing data, I/O failures, and invalid templates return typed `thiserror`
errors (or `color-eyre` only at the CLI boundary). Prefer reject over silent default.

### II. No AI Attribution

Commits, PRs, changelogs, and authored content never include agent branding, Devin
footers, or bot `Co-Authored-By` trailers. Git author is the human developer identity.

### III. Path Safety First

Template loads and sync writes must not follow untrusted symlinks, must reject `..`
and absolute destinations, and must never write outside the project root.

### IV. Deterministic Sync

`truss sync` / `check` are pure given a template + context + disk state. No hidden
network I/O in the core library. Dry-run and check exit codes must be predictable.

### V. Test-First for Safety Surfaces

Path safety, registry parse, protect-lists, and drift detection require automated
tests (unit + integration) before shipping. Clippy `-D warnings` must stay green.

### VI. Spec Kit & Templates Are Product

Embedded packs (`default`, `nixdex`, `spec-kit`, `agent-rules`) and registry entries
are product surfaces — keep them small, honest, and free of marketing slop.
