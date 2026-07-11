# agent-rules

Local agent policy pack for `{{ project_name }}`.

Intended use:

- Drop into a repo as a starting point for agent loader files.
- Optionally gitignore `AGENTS.md` / `CLAUDE.md` if they stay machine-local.
- Copy `.githooks/commit-msg` into the project's `CONTRIBUTING` guidance (and
  install under `.git/hooks/commit-msg` when you want the check active).

Contents:

| Path | Role |
|------|------|
| `AGENTS.md` | No AI attribution, Conventional Commits, secrets, TDD |
| `CLAUDE.md` | Thin pointer to `AGENTS.md` |
| `.githooks/commit-msg` | Sample hook enforcing the above on subjects/footers |

Keep this pack short. Do not expand it into a style guide dump.
