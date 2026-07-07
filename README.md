# AxisFlow

n8n-like flow automation engine, built in Rust. Nodes are standalone CLI
binaries (`af-*`); flows are declarative YAML DAGs executed by `axisflow`.

## Install

```bash
git clone https://github.com/your-org/axisflow.git
cd axisflow
cargo build --release
./target/release/axisflow --version
```

## Quick start

```bash
# Build all included nodes
cargo build

# See what nodes are available
PATH=target/debug:$PATH ./target/debug/axisflow nodes

# Run the echo demo
PATH=target/debug:$PATH ./target/debug/axisflow run examples/echo-demo.yaml
```

## Concepts

- **Flow Spec** — a YAML file describing a DAG of nodes. See `docs/flow-spec.md`.
- **Node** — a standalone CLI binary named `af-<name>` that reads JSON on stdin
  and writes JSON on stdout. See `docs/node-contract.md`.
- **Orchestrator** (`axisflow` binary) — validates & executes a Flow Spec,
  spawning node subprocesses in topological order with bounded concurrency.

## CLI

```
axisflow <COMMAND>

Commands:
  run       Execute a Flow Spec        axisflow run flow.yaml
  nodes     List all af-* on PATH      axisflow nodes
  validate  Validate without running   axisflow validate flow.yaml
  help      Print help
```

## Included nodes

| Node | What |
|---|---|
| `af-echo` | Echo a message (reference / test node) |
| `af-http` | HTTP client (GET / POST / PUT / DELETE / PATCH) |
| `af-jq` | JSON path extractor (dot-path syntax) |
| `af-db` | PostgreSQL query execution |
| `af-file` | File read / write |
| `af-vault` | Credential storage (resolves `vault://` refs) |

## Writing a flow

```yaml
# examples/echo-demo.yaml
version: 1
name: echo-demo
nodes:
  - id: a
    use: echo
    with:
      message: "hello from AxisFlow"
  - id: b
    use: echo
    inputs:
      message: a.echo           # wire upstream output
```

```bash
axisflow run examples/echo-demo.yaml
```

## Secrets (vault)

Write secrets to a JSON file (default `~/.axisflow/vault.json` or
`AXISFLOW_VAULT_FILE`):

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

The Orchestrator resolves `vault://` refs at runtime by calling `af-vault`.

## Building a custom node

1. Implement the Node Contract: `--describe` (manifest), `--version`, and
   stdin/stdout JSON execution.
2. Name your binary `af-<name>`.
3. Put it on `PATH` or `AXISFLOW_NODE_PATH`.

Minimal example: see `crates/af-echo/src/main.rs`.

## Environment

| Variable | Purpose |
|---|---|
| `AXISFLOW_NODE_PATH` | Colon-separated directories to search for `af-*` binaries |
| `AXISFLOW_VAULT_FILE` | Path to vault JSON file (default `~/.axisflow/vault.json`) |

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Node Contract](docs/node-contract.md)
- [Flow Spec](docs/flow-spec.md)

## License

MIT OR Apache-2.0
