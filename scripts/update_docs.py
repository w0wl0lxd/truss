#!/usr/bin/env python3
"""Regenerate README.md and docs/CLI.md from the source of truth.

The template is docs/README.template.md.  Marked sections like

    <!-- doc-gen: KEY -->
    ... default content ...
    <!-- /doc-gen: KEY -->

are replaced with values derived from Cargo.toml, crate manifests, and the
truss CLI itself.  Re-run after changing commands, workspace metadata, or the
template.
"""

import os
import re
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "docs" / "README.template.md"
README = ROOT / "README.md"
CLI_DOC = ROOT / "docs" / "CLI.md"
CARGO_TOML = ROOT / "Cargo.toml"


def run_truss(args: list[str]) -> str:
    """Run the truss CLI and return stdout."""
    env = os.environ.copy()
    env["NO_COLOR"] = "1"
    result = subprocess.run(
        ["cargo", "run", "-q", "--bin", "truss", "--", *args, "--help"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"cargo run failed for args {args!r}:\n{result.stderr}"
        )
    return result.stdout


def parse_commands(help_text: str) -> list[tuple[str, str]]:
    """Extract command names and descriptions from a clap --help block."""
    commands: list[tuple[str, str]] = []
    in_commands = False
    for line in help_text.splitlines():
        stripped = line.rstrip()
        if stripped == "Commands:":
            in_commands = True
            continue
        if in_commands:
            if not stripped:
                break
            if not stripped.startswith("  "):
                break
            # Lines look like: "  new     Create a new project..."
            parts = stripped.lstrip().split(None, 1)
            if len(parts) < 2:
                continue
            name, desc = parts[0], parts[1].strip()
            if name == "help":
                continue
            commands.append((name, desc))
    return commands


def collect_cli(args: list[str] = None) -> list[tuple[list[str], str]]:
    """Recursively collect (command_path, help_text) for every command."""
    if args is None:
        args = []
    help_text = run_truss(args)
    entries = [(args, help_text)]
    for name, _desc in parse_commands(help_text):
        child = args + [name]
        entries.extend(collect_cli(child))
    return entries


def slug_for_path(path: list[str]) -> str:
    return "-".join(["truss", *path]).lower()


def format_cli_reference(entries: list[tuple[list[str], str]]) -> str:
    """Format the full nested CLI reference for docs/CLI.md."""
    lines: list[str] = ["# CLI Reference\n", "> Generated from `truss --help`.\n"]
    for path, help_text in entries:
        level = len(path) + 1  # top starts at h2
        prefix = "#" * level
        invocation = " ".join(["truss", *path]) if path else "truss"
        lines.append(f"{prefix} `{invocation}`\n")
        lines.append("```text")
        lines.append(help_text.rstrip())
        lines.append("```\n")
    return "\n".join(lines) + "\n"


def format_cli_summary(entries: list[tuple[list[str], str]]) -> str:
    """Format a short top-level command table for the README."""
    if not entries:
        return ""
    top_help = entries[0][1]
    top_commands = {name: desc for name, desc in parse_commands(top_help)}
    top = [(path, help) for path, help in entries if len(path) == 1]
    if not top:
        return ""
    lines = ["| Command | Description |", "|---------|-------------|"]
    for path, _help_text in top:
        name = path[0]
        desc = top_commands.get(name, "")
        anchor = slug_for_path([name])
        lines.append(f"| [`truss {name}`](docs/CLI.md#{anchor}) | {desc} |")
    lines.append("")
    lines.append("See [docs/CLI.md](docs/CLI.md) for the complete command reference.")
    return "\n".join(lines)


def read_toml(path: Path) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def get_metadata() -> dict[str, str]:
    cargo = read_toml(CARGO_TOML)
    pkg = cargo.get("workspace", {}).get("package", {})
    return {
        "version": pkg.get("version", ""),
        "edition": pkg.get("edition", ""),
        "rust_version": pkg.get("rust-version", ""),
        "license": pkg.get("license", ""),
        "description": pkg.get("description", ""),
        "authors": ", ".join(pkg.get("authors", [])),
    }


def get_crates_table() -> str:
    cargo = read_toml(CARGO_TOML)
    members = cargo.get("workspace", {}).get("members", [])
    rows: list[tuple[str, str]] = []
    for member in members:
        manifest = ROOT / member / "Cargo.toml"
        desc = ""
        if manifest.exists():
            pkg = read_toml(manifest).get("package", {})
            name = pkg.get("name", member)
            desc = pkg.get("description", "")
        else:
            name = member
        rows.append((name, desc))
    if not rows:
        return ""
    lines = ["| Crate | Description |", "|-------|-------------|"]
    for name, desc in rows:
        lines.append(f"| `{name}` | {desc} |")
    return "\n".join(lines)


def get_embedded_packs() -> str:
    env = os.environ.copy()
    env["NO_COLOR"] = "1"
    result = subprocess.run(
        ["cargo", "run", "-q", "--bin", "truss", "--", "templates"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        raise RuntimeError(f"cargo run templates failed:\n{result.stderr}")
    lines = result.stdout.splitlines()
    # Skip header and keep only embedded/built-in packs
    packs: list[tuple[str, str, str]] = []
    for line in lines[1:]:
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        name, kind, source = parts
        if kind != "embedded":
            continue
        packs.append((name, kind, source))
    if not packs:
        return ""
    out = ["| Pack | Kind | Source |", "|------|------|--------|"]
    for name, kind, source in packs:
        out.append(f"| `{name}` | {kind} | {source} |")
    return "\n".join(out)


def replace_marker(text: str, key: str, value: str) -> str:
    """Replace content between <!-- doc-gen: KEY --> and <!-- /doc-gen: KEY -->."""
    start = r"<!--\s*doc-gen:\s*" + re.escape(key) + r"\s*-->"
    end = r"<!--\s*/doc-gen:\s*" + re.escape(key) + r"\s*-->"
    pattern = re.compile(r"(" + start + r"\n)(.*?)(\n" + end + r")", re.DOTALL)
    return pattern.sub(r"\g<1>" + value + r"\g<3>", text)


def main() -> int:
    if not TEMPLATE.exists():
        raise FileNotFoundError(f"Missing template: {TEMPLATE}")

    template = TEMPLATE.read_text(encoding="utf-8")
    entries = collect_cli()

    metadata = get_metadata()
    replacements = {
        "truss_version": metadata["version"],
        "truss_edition": metadata["edition"],
        "truss_rust_version": metadata["rust_version"],
        "truss_license": metadata["license"],
        "truss_description": metadata["description"],
        "truss_authors": metadata["authors"],
        "crates_table": get_crates_table(),
        "embedded_packs": get_embedded_packs(),
        "cli_summary": format_cli_summary(entries),
        "cli_reference": format_cli_reference(entries),
    }

    # The CLI reference block that lives in the README is the summary plus a
    # link to the full generated docs/CLI.md.
    replacements["cli_reference"] = (
        "## Available commands\n\n" + replacements["cli_summary"]
    )

    output = template
    for key, value in replacements.items():
        output = replace_marker(output, key, value)

    README.write_text(output, encoding="utf-8")

    CLI_DOC.write_text(format_cli_reference(entries), encoding="utf-8")

    print(f"generated {README}")
    print(f"generated {CLI_DOC}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
