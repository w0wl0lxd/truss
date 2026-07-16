# Quickstart: Registry CLI

## Prerequisites

```bash
just setup-hooks
cargo build -p truss-cli
```

## Register a custom pack

```bash
mkdir -p /tmp/my-pack
cat > /tmp/my-pack/Cargo.toml <<'EOF'
[workspace]
resolver = "3"
EOF
echo '# team rules' > /tmp/my-pack/AGENTS.md
cargo run -p truss-cli -- registry add my-pack --source /tmp/my-pack --kind dir
cargo run -p truss-cli -- templates
```

## Scaffold from it

```bash
cargo run -p truss-cli -- new demo --template my-pack --path /tmp/demo-proj
test -f /tmp/demo-proj/AGENTS.md
```

## Protect + dry-run

```bash
mkdir -p /tmp/demo-proj/.truss
echo 'AGENTS.local.md' > /tmp/demo-proj/.truss/protect
echo local > /tmp/demo-proj/AGENTS.local.md
cargo run -p truss-cli -- sync --path /tmp/demo-proj --template my-pack --dry-run
# AGENTS.local.md must still say "local" after real sync:
cargo run -p truss-cli -- sync --path /tmp/demo-proj --template my-pack
```

## Remove

```bash
cargo run -p truss-cli -- registry remove my-pack
```

## Expected tests

```bash
cargo nextest run --workspace --no-fail-fast
```
