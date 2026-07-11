# AGENTS.md

Local agent policy pack. Keep this short and enforceable.

## Non-negotiable

- No AI attribution in commits, PRs, changelogs, or docs (`Generated with…`,
  agent `Co-Authored-By`, etc.).
- Author only as the human Git identity on the machine.
- Conventional Commits subjects (`feat:`, `fix:`, `chore:`, …).
- Never commit secrets, `.env`, PEM keys, tokens, or credentials.
- TDD for production paths: failing test first, then minimum code, then
  `cargo nextest` / project equivalent.

## Preferred workflow

1. Reproduce or specify the change with a test or acceptance check.
2. Implement the smallest correct fix.
3. Run lint + tests before proposing a commit.
4. Stage explicit paths only. No bulk `git add -A` of secrets or scratch files.
