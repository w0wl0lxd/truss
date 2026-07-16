# Research: Git-based Remote Templates

## Existing tooling conventions

### cargo-generate

- Treats a Git repository as a template source by default.
- CLI: `cargo generate --git https://github.com/owner/repo.git --branch main --subfolder templates/rust-service`.
- Supports URL shorthands: `gh:owner/repo`, `gl:owner/repo`, `bb:owner/repo`, `sr:~owner/repo`.
- Clones into a temporary directory, renders the template, and removes `.git` before copying to the project.
- Supports `--init` to generate into the current directory without creating a sub-project folder.
- Supports `cargo-generate.toml` for include/exclude rules and favorite templates.

### Cookiecutter

- CLI: `cookiecutter gh:audreyfeldroy/cookiecutter-pypackage` or `cookiecutter https://github.com/.../repo.git --checkout develop`.
- Supports branch/tag/commit via `--checkout`.
- Supports zip files and Mercurial in addition to Git.
- Uses `cookiecutter.json` for prompts.

## Implications for truss

- Users expect `--git`, `--branch`/`--pointer`, and `--subfolder` semantics.
- A registry entry is the natural place to persist these values so users do not retype URLs.
- The `.git` directory must never leak into the generated project.
- Caching the repository locally avoids repeated full clones and enables offline usage.

## Technical options

### Git implementation

| Option | Pros | Cons |
|--------|------|------|
| `gix` (pure Rust) | No native libgit2 dependency; integrates with Cargo ecosystem | Large dependency tree; feature flags must be carefully selected |
| `git2` (libgit2 bindings) | Mature, well-known API | Requires native libgit2; build environment complexity |
| Shell out to `git` CLI | Smallest dependency footprint | Requires `git` on PATH; harder to test and secure |

**Recommendation**: Delegate to the system `git` binary through `std::process::Command` with validated arguments. This avoids pulling in `gix`'s large dependency tree and long compile times while relying on a tool that is already present on developer machines. A pure-Rust `gix` backend can be revisited if `truss` later needs to ship without an external `git` dependency.

### Cache strategy

| Option | Pros | Cons |
|--------|------|------|
| Clone to temp on every use | Simple; no cache invalidation | Slow; no offline support |
| Bare mirror + worktree checkout | Fast updates; supports multiple refs | More complex `gix` API surface |
| Clone/full worktree with fetch | Simpler API; good enough for CLI scale | Larger disk usage than bare mirror |

**Recommendation**: A full worktree clone in `$XDG_CACHE_HOME/truss/git/<entry-key>` with a `fetch` before checkout. This balances API simplicity and update performance.

### URL shorthand expansion

- `gh:owner/repo` -> `https://github.com/owner/repo.git`
- `gl:owner/repo` -> `https://gitlab.com/owner/repo.git`
- `bb:owner/repo` -> `https://bitbucket.org/owner/repo.git`
- `sr:owner/repo` -> `https://git.sr.ht/~owner/repo`
- `owner/repo` (no prefix) -> `https://github.com/owner/repo.git` as a common default, with a documented fallback for local paths.

## Security considerations

- Validate URLs before passing them to `gix`; reject `file://` and local paths for `kind = "git"` to prevent accidental use of an untrusted local repo.
- Resolve `subfolder` as a normalized relative path and ensure it does not escape the repository root.
- Never copy `.git` or VCS metadata into generated projects.
- Treat SSH URLs with the same validation as HTTPS; rely on the user's SSH agent for authentication rather than prompting for credentials.

## Open questions (to resolve during planning)

- Should the cache key be the entry name or a hash of the normalized URL? Entry name is simpler and matches registry remove semantics; URL hash would survive entry renames.
- Should `pointer` default to `HEAD` or the remote default branch? Defaulting to the default branch is the least surprising behavior.
- Should `truss registry add` gain a `--subfolder` flag, or should we reuse `--target` with a different meaning for `git` entries? A dedicated `--subfolder` flag is clearer and avoids confusing `file` entry semantics.
