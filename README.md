# Ngalir

n8n-like flow automation engine, built in Rust. Nodes are standalone CLI
binaries (`na-*`); flows are declarative YAML DAGs executed by `ngalir`.

## Install

```bash
git clone https://github.com/your-org/ngalir.git
cd ngalir
cargo build --release
./target/release/ngalir --version
```

## Quick start

```bash
# Build all included nodes
cargo build

# See what nodes are available
PATH=target/debug:$PATH ./target/debug/ngalir nodes

# Run the echo demo
PATH=target/debug:$PATH ./target/debug/ngalir run examples/echo-demo.yaml
```

## Concepts

- **Flow Spec** — a YAML file describing a DAG of nodes. See `docs/flow-spec.md`.
- **Node** — a standalone CLI binary named `na-<name>` that reads JSON on stdin
  and writes JSON on stdout. See `docs/node-contract.md`.
- **Orchestrator** (`ngalir` binary) — validates & executes a Flow Spec,
  spawning node subprocesses in topological order with bounded concurrency.

## CLI

```
ngalir <COMMAND>

Commands:
  run       Execute a Flow Spec        ngalir run flow.yaml
  nodes     List all na-* on PATH      ngalir nodes
  validate  Validate without running   ngalir validate flow.yaml
  help      Print help
```

## Included nodes

| Node | What |
|---|---|
| `na-echo` | Echo a message (reference / test node) |
| `na-http` | HTTP client (GET / POST / PUT / DELETE / PATCH) |
| `na-jsonpath` | JSON path extractor (dot-path syntax) |
| `na-db` | PostgreSQL query execution |
| `na-file` | File read / write |
| `na-vault` | Credential storage (resolves `vault://` refs) |

## Writing a flow

```yaml
# examples/echo-demo.yaml
version: 1
name: echo-demo
nodes:
  - id: a
    use: echo
    with:
      message: "hello from Ngalir"
  - id: b
    use: echo
    inputs:
      message: a.echo           # wire upstream output
```

```bash
ngalir run examples/echo-demo.yaml
```

## Secrets (vault)

Write secrets to a JSON file (default `~/.ngalir/vault.json` or
`NGALIR_VAULT_FILE`):

```json
{
  "api_key": "sk-xxx",
  "db/prod": "postgresql://user:pass@host/db"
}
```

Then reference them in flows via `vault://`:

```yaml
nodes:
  - id: query
    use: db
    with:
      connection: vault://db/prod
      query: "SELECT * FROM users"
```

The Orchestrator resolves `vault://` refs at runtime by calling `na-vault`.

## Building a custom node

1. Implement the Node Contract: `--describe` (manifest), `--version`, and
   stdin/stdout JSON execution.
2. Name your binary `na-<name>`.
3. Put it on `PATH` or `NGALIR_NODE_PATH`.

Minimal example: see `crates/na-echo/src/main.rs`.

## Environment

| Variable | Purpose |
|---|---|
| `NGALIR_NODE_PATH` | Colon-separated directories to search for `na-*` binaries |
| `NGALIR_VAULT_FILE` | Path to vault JSON file (default `~/.ngalir/vault.json`) |

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Node Contract](docs/node-contract.md)
- [Flow Spec](docs/flow-spec.md)

## License

MIT
