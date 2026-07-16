# Quickstart: Workspace Members

## Prerequisites

```bash
just setup-hooks
cargo build -p truss-cli
```

## Create a project and add members

```bash
# Create a workspace from the default pack
cargo run -p truss-cli -- new demo --path /tmp/demo-proj
cd /tmp/demo-proj

# Add a library crate
cargo run -p truss-cli -- member add mylib --kind lib

# Add a binary crate
cargo run -p truss-cli -- member add mybin --kind bin

# Verify workspace members
cargo run -p truss-cli -- member list
# expected output:
# crates/app
# crates/mylib
# crates/mybin

# Verify it builds
cargo check
```

## Custom member path

```bash
cargo run -p truss-cli -- member add shared-utils --kind lib --member-path libs/shared-utils
```

## Work on a workspace from another directory

```bash
cargo run -p truss-cli -- member list --path /tmp/demo-proj
```

## Remove a member

```bash
# Remove from workspace.members but keep files
cargo run -p truss-cli -- member remove mylib

# Remove from workspace.members and delete the directory
cargo run -p truss-cli -- member remove mylib --delete

# Remove a member that lives under a custom path
cargo run -p truss-cli -- member remove libs/shared-utils --delete
```

## Expected tests

```bash
cargo nextest run --workspace --no-fail-fast
```
