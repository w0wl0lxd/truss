# Writing template packs

A `truss` template pack is a directory of files.  When a pack is rendered,
each file is written to the same relative path inside the target project.
Files containing MiniJinja syntax (`{{ ... }}`, `{% ... %}`, `{# ... #}`) are
rendered through the template engine; all other files are copied as-is.

## Embedded packs

`truss` ships with a few built-in packs:

| Pack | Purpose |
|------|---------|
| `default` | A minimal Rust workspace. |
| `spec-kit` | A starter set for writing project specs. |
| `agent-rules` | Team conventions and agent loader files. |

List them with:

```bash
truss templates
```

## Template context

The following variables are available inside template files:

| Variable | Source | Example |
|----------|--------|---------|
| `project_name` | CLI prompt / argument | `my-project` |
| `author` | `--author` if given, otherwise workspace `Cargo.toml` `[workspace.package].authors[0]` (or `[package].authors[0]`), otherwise `$USER` env | `w0w` |
| `license` | `--license` if given, otherwise workspace `Cargo.toml` `license` | `MIT` (or empty) |
| `repository` | workspace `Cargo.toml` `repository` or `new` prompt | `https://github.com/example/my-project` |
| `edition` | `--edition` if given, otherwise workspace `Cargo.toml` `edition`, otherwise `CARGO_PKG_EDITION` (fallback `2024`) | `2024` |
| `extra` | `IndexMap<String, String>` | custom key/value pairs |

For `sync` and `check`, `truss` reads the existing `Cargo.toml` and extracts
the `workspace.package` (or `package`) values so the rendered template uses the
project's own metadata.

## A minimal custom pack

```text
my-pack/
├── Cargo.toml
├── crates
│   └── app
│       ├── Cargo.toml
│       └── src
│           └── main.rs
└── justfile
```

`my-pack/Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/app"]

[workspace.package]
version = "0.1.0"
edition = "{{ edition }}"
{% if author %}authors = ["{{ author }}"]{% endif %}
{% if license %}license = "{{ license }}"{% endif %}
```

`my-pack/crates/app/Cargo.toml`:

```toml
[package]
name = "{{ project_name }}"
version.workspace = true
edition.workspace = true
{% if author %}authors.workspace = true{% endif %}
{% if license %}license.workspace = true{% endif %}
```

`my-pack/crates/app/src/main.rs`:

```rust
fn main() {
    println!("Hello from {{ project_name }}!");
}
```

## Registering and using a pack

Register the pack with a name:

```bash
truss registry add my-pack --source ./my-pack --kind dir
truss templates
```

Create a project from it:

```bash
truss new demo --template my-pack
cd demo
cargo check
```

Re-apply the template later to pull in updates:

```bash
truss sync --path demo --template my-pack --dry-run
truss sync --path demo --template my-pack
```

## JSON-described packs

A pack directory may carry a `truss-pack.json` manifest. When it does, the
manifest -- not the directory layout -- decides which sources become which
files in the generated project, and the manifest itself is never emitted.
`truss new`, `sync`, `check` and `update` all pick this up automatically.

```json
{
  "name": "service",
  "version": "1.0.0",
  "description": "HTTP service scaffold",
  "variables": [
    { "name": "has_cli", "type": "bool", "default": false },
    { "name": "port", "type": "integer", "default": 8080, "required": true },
    { "name": "lang", "type": "string", "choices": ["rust", "go"], "default": "rust" }
  ],
  "files": [
    { "source": "src", "destination": "src" },
    { "source": "src/cli.rs", "destination": "src/cli.rs", "condition": "has_cli" },
    { "source": "logo.png", "destination": "assets/logo.png", "is_template": false }
  ]
}
```

### Variables

| Field | Meaning |
| --- | --- |
| `name` | ASCII letters, digits and `_`, not starting with a digit. A hyphen is subtraction in an expression, so hyphenated names are rejected. |
| `type` | `string`, `integer` or `bool`. |
| `required` | Reject generation when no value and no `default` is supplied. |
| `default` | Used when the caller supplies no value. Checked against `type`, `regex` and `choices`. |
| `regex` | Value must match this pattern. |
| `choices` | Value must be one of these. |
| `description` | Prompt label. |

### File mappings

| Field | Meaning |
| --- | --- |
| `source` | Path inside the pack. A file, or a directory to expand recursively. Must not escape the pack, and must not be a symlink. |
| `destination` | Normalized relative path inside the generated project. Must be unique. |
| `condition` | Expression deciding whether the mapping applies. |
| `is_template` | Render the body through the engine. Defaults to `true`; set `false` to copy bytes verbatim. |

Destinations are always rendered, so `{{ project_name }}.rs` works as a file
name even when `is_template` is `false`.

### Conditions

A condition is a minijinja expression. It sees every pack variable plus the
built-in context fields (`project_name`, `author`, `license`, `repository`,
`edition`), so both of these are valid:

```json
{ "condition": "has_cli and lang == \"rust\"" }
{ "condition": "edition == \"2024\"" }
```

A variable the context does not supply falls back to its `default`; with
neither, it stays undefined, which reads as false.

When mappings overlap, the one with the longest matching `destination` owns the
file. Given a mapping for `src` and another for `src/cli.rs`, a false condition
on `src/cli.rs` excludes that file even though the `src` mapping applies -- and
no file is ever emitted twice.

Run `truss pack validate <PATH>` to check a manifest, its sources, its
destinations and that every template compiles, before publishing the pack.

## File modes

- Packs loaded from a directory preserve the original file modes (e.g. `0o755` for an executable script).
- Embedded packs default to `0o644`.
- `truss` sets permissions on each generated file when writing.

## Protecting local files

If a template contains a file you want to keep local edits to, protect it:

### Via the command line

```bash
truss sync --path demo --template my-pack --protect AGENTS.local.md
```

Repeat `--protect` for each path.

### Via `.truss/protect`

Create `demo/.truss/protect` with one relative path per line:

```text
AGENTS.local.md
README.md
```

Lines starting with `#` and blank lines are ignored.  Protected files appear as
`SkipProtected` in `sync --dry-run` and are never overwritten.

## Valid template paths

All template-relative paths must be relative and must not contain `..` or
absolute components.  This prevents a pack from writing outside the project root.

## Tips

- Keep packs focused: one pack per project shape (service, library, CLI, etc.).
- Use `{{ project_name }}` in directory and file contents where it makes sense.
- Use `truss check` in CI to detect drift between a project and its pack.
- Store team packs in a shared Git repository and register them with `--kind git`.
- Pin a specific branch or tag with `--pointer` and select a sub-directory with
  `--subfolder` when the pack lives in a monorepo.
- `truss` caches Git packs under `$XDG_CACHE_HOME/truss/git/<name>` and updates
  them on each use, so a slow network only hurts the first clone.
