# AGENTS.md

`truss` is a Rust workspace (`crates/truss-core`, `crates/truss-cli`) that scaffolds
high-frequency trading (HFT) workspace repositories from a set of templates. It is
intended to be invoked as a standalone CLI (`truss`) and may later be wired into
editor/AI coding hooks.

## Shared Reasoning Memory (Thoughtbox)

Enforced globally (see `~/.agents/AGENTS.md`) — this project does not opt out. Use the
`thoughtbox` MCP knowledge graph for durable, cross-agent facts specific to this repo:
template conventions, registry sync edge cases, and CLI command behavior decisions.
Ephemeral task reasoning still belongs in a thoughtbox session; only graduate what
should outlive the task into the knowledge graph.
